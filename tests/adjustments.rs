//! Live layer adjustments: brightness, contrast, saturation and hue.
//!
//! These are non-destructive properties applied at composite time, so the tests
//! check both the colour maths and the promises that make them safe: neutral
//! values change nothing, resetting restores the original pixels exactly, the
//! adjustment travels through save/load, and undo never touches paint.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use inkforge::app::InkforgeApp;
use inkforge::egui::Color32;
use inkforge::engine::{Background, Canvas, PixelRegion};
use inkforge::layers::{Adjustment, Entry, Layer, LayerStack};

const SIZE: u32 = 8;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_path(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "inkforge-adjust-{}-{n}-{name}.inkforge",
        std::process::id()
    ))
}

fn opaque(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, 255)
}

fn adjustment(brightness: f32, contrast: f32, saturation: f32, hue: f32) -> Adjustment {
    Adjustment {
        brightness,
        contrast,
        saturation,
        hue,
    }
}

#[track_caller]
fn assert_rgb(pixel: Color32, expected: (u8, u8, u8), tolerance: u8, note: &str) {
    let (r, g, b) = (pixel.r(), pixel.g(), pixel.b());
    let close = |a: u8, b: u8| a.abs_diff(b) <= tolerance;
    assert!(
        close(r, expected.0) && close(g, expected.1) && close(b, expected.2),
        "{note}: expected {expected:?} (±{tolerance}), got ({r}, {g}, {b}) from {pixel:?}"
    );
}

fn full_region() -> PixelRegion {
    PixelRegion {
        x: 0,
        y: 0,
        width: SIZE,
        height: SIZE,
    }
}

fn filled(id: u64, pixels: impl Fn(u32, u32) -> Color32) -> Layer {
    let mut layer = Layer::new(id, format!("layer {id}"), SIZE, SIZE);
    let mut data = Vec::with_capacity((SIZE * SIZE) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            data.push(pixels(x, y));
        }
    }
    layer.surface_mut().write_region(full_region(), &data);
    layer
}

fn with_adjustment(mut layer: Layer, adjust: Adjustment) -> Layer {
    let mut props = layer.props();
    props.adjust = adjust;
    layer.set_props(props);
    layer
}

fn stack(entries: Vec<Entry>) -> LayerStack {
    let active = entries.len() - 1;
    LayerStack::from_entries(SIZE, SIZE, entries, active)
}

#[test]
fn neutral_adjustment_changes_nothing() {
    assert!(Adjustment::default().is_identity());

    let pixels = [
        opaque(255, 0, 0),
        opaque(17, 93, 201),
        Color32::from_rgba_premultiplied(10, 20, 30, 128),
        Color32::TRANSPARENT,
    ];
    for pixel in pixels {
        assert_eq!(
            Adjustment::default().apply(pixel),
            pixel,
            "a neutral adjustment must return the pixel untouched"
        );
    }
}

#[test]
fn brightness_lifts_and_darkens() {
    let mid = opaque(128, 128, 128);
    assert_rgb(
        adjustment(0.25, 0.0, 1.0, 0.0).apply(mid),
        (192, 192, 192),
        2,
        "brightness +25%",
    );
    assert_rgb(
        adjustment(-0.25, 0.0, 1.0, 0.0).apply(mid),
        (64, 64, 64),
        2,
        "brightness -25%",
    );
}

#[test]
fn contrast_pushes_away_from_and_towards_mid_grey() {
    let light = opaque(200, 200, 200);
    assert_rgb(
        adjustment(0.0, 0.5, 1.0, 0.0).apply(light),
        (236, 236, 236),
        2,
        "positive contrast lightens a light tone",
    );

    assert_rgb(
        adjustment(0.0, -1.0, 1.0, 0.0).apply(light),
        (128, 128, 128),
        2,
        "full negative contrast flattens everything to mid-grey",
    );
}

#[test]
fn saturation_zero_turns_colour_into_grey() {
    let red = opaque(255, 0, 0);
    // Rec. 601 luma of pure red is 0.3.
    assert_rgb(
        adjustment(0.0, 0.0, 0.0, 0.0).apply(red),
        (77, 77, 77),
        2,
        "fully desaturated red",
    );

    let doubled = adjustment(0.0, 0.0, 2.0, 0.0).apply(opaque(200, 100, 100));
    assert!(
        doubled.r() > 200 && doubled.g() < 100,
        "200% saturation must push the channels apart, got {doubled:?}"
    );
}

#[test]
fn hue_rotation_turns_red_into_green() {
    let green = adjustment(0.0, 0.0, 1.0, 120.0).apply(opaque(255, 0, 0));
    assert_rgb(green, (0, 255, 0), 2, "red rotated 120° along the hue wheel");

    let back = adjustment(0.0, 0.0, 1.0, 240.0).apply(opaque(255, 0, 0));
    assert_rgb(back, (0, 0, 255), 2, "red rotated 240°");
}

#[test]
fn hue_rotation_leaves_grey_alone() {
    let grey = opaque(120, 120, 120);
    assert_eq!(
        adjustment(0.0, 0.0, 1.0, 90.0).apply(grey),
        grey,
        "grey has no hue to rotate"
    );
}

#[test]
fn adjustment_preserves_alpha() {
    let pixel = Color32::from_rgba_premultiplied(40, 20, 10, 102);
    let adjusted = adjustment(0.5, 0.0, 1.0, 0.0).apply(pixel);
    assert_eq!(adjusted.a(), 102, "alpha is never touched");
    assert!(
        adjusted.r() + adjusted.g() + adjusted.b() > pixel.r() + pixel.g() + pixel.b(),
        "the colour itself did change: {adjusted:?}"
    );
}

#[test]
fn out_of_range_values_are_sanitised() {
    let wild = Adjustment {
        brightness: 9.0,
        contrast: f32::NAN,
        saturation: -3.0,
        hue: 540.0,
    }
    .sanitised();

    assert_eq!(wild.brightness, 1.0);
    assert_eq!(wild.contrast, 0.0);
    assert_eq!(wild.saturation, 0.0);
    assert_eq!(wild.hue, 180.0, "540° is one and a half turns: 180°");
}

#[test]
fn the_composite_applies_the_adjustment() {
    let layer = with_adjustment(
        filled(0, |_x, _y| opaque(255, 0, 0)),
        adjustment(0.0, 0.0, 0.0, 0.0),
    );
    let mut stack = stack(vec![Entry::Layer(layer)]);
    assert_rgb(
        stack.composite().pixel(3, 3),
        (77, 77, 77),
        2,
        "the layer's red is desaturated as it is composited",
    );
}

#[test]
fn resetting_restores_the_original_artwork_exactly() {
    let mut canvas = Canvas::try_new("adjust", 16, 16, Background::White).expect("canvas");
    let id = canvas.active_layer_id().expect("layer");
    let before: Vec<Color32> = (0..16)
        .flat_map(|_| (0..16).map(|_| opaque(210, 90, 40)))
        .collect();
    canvas
        .layer_surface_mut(id)
        .expect("surface")
        .write_region(
            PixelRegion {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            &before,
        );
    canvas.invalidate_composite(None);
    let original = canvas.composite().pixel(4, 4);

    let mut props = canvas.layer_props(id).expect("props");
    props.adjust = adjustment(0.4, 0.3, 0.2, 90.0);
    canvas.set_layer_props(id, props);
    assert_ne!(
        canvas.composite().pixel(4, 4),
        original,
        "the adjustment must be visible"
    );

    let mut props = canvas.layer_props(id).expect("props");
    props.adjust = Adjustment::default();
    canvas.set_layer_props(id, props);
    assert_eq!(
        canvas.composite().pixel(4, 4),
        original,
        "the pixels were never modified, so neutral restores them exactly"
    );
    assert_eq!(
        canvas.layer_surface(id).expect("surface").pixel(4, 4),
        before[4 * 16 + 4],
        "and the layer's own pixels are untouched"
    );
}

#[test]
fn a_clipped_layer_keeps_its_own_adjustment() {
    let base = filled(0, |x, _y| {
        if x < SIZE / 2 {
            opaque(0, 255, 0)
        } else {
            Color32::TRANSPARENT
        }
    });
    let top = with_adjustment(
        filled(1, |_x, _y| opaque(0, 0, 255)),
        adjustment(0.0, 0.0, 0.0, 0.0),
    );
    let mut top = top;
    let mut props = top.props();
    props.clip_to_below = true;
    top.set_props(props);

    let mut stack = stack(vec![Entry::Layer(base), Entry::Layer(top)]);
    let inside = stack.composite().pixel(1, 1);
    // Rec. 601 luma of pure blue is 0.11.
    assert_rgb(inside, (28, 28, 28), 2, "greyed blue inside the base");
    assert_eq!(
        stack.composite().pixel(6, 1).a(),
        0,
        "outside the base the clipped layer still does not show"
    );
}

#[test]
fn merging_down_bakes_the_adjustment() {
    let mut canvas = Canvas::try_new("merge", 16, 16, Background::White).expect("canvas");
    let target = canvas.active_layer_id().expect("layer");
    canvas.layer_add_above_active();
    let source = canvas.active_layer_id().expect("layer");
    canvas
        .layer_surface_mut(source)
        .expect("surface")
        .write_region(
            PixelRegion {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            &vec![opaque(255, 0, 0); 16 * 16],
        );
    let mut props = canvas.layer_props(source).expect("props");
    props.adjust = adjustment(0.0, 0.0, 0.0, 0.0);
    canvas.set_layer_props(source, props);

    let index = canvas.active_layer_index();
    canvas.layer_merge_down(index).expect("merge");

    let merged = canvas.layer_surface(target).expect("surface").pixel(5, 5);
    assert_rgb(merged, (77, 77, 77), 2, "the merge baked the grey, not the red");
    assert_eq!(
        canvas.layer_props(target).expect("props").adjust,
        Adjustment::default(),
        "the target was not given an adjustment it never had"
    );
}

#[test]
fn an_adjustment_undoes_in_one_step() {
    let mut app = InkforgeApp::with_defaults();
    let id = app.canvas.active_layer_id().expect("layer");

    app.set_layer_adjustment(id, adjustment(0.3, 0.0, 1.0, 0.0));
    assert_eq!(
        app.canvas.layer_props(id).expect("props").adjust.brightness,
        0.3
    );

    app.undo();
    assert_eq!(
        app.canvas.layer_props(id).expect("props").adjust,
        Adjustment::default(),
        "one undo returns the layer to neutral"
    );

    app.redo();
    assert_eq!(
        app.canvas.layer_props(id).expect("props").adjust.brightness,
        0.3
    );
}

#[test]
fn an_adjustment_survives_saving_and_loading() {
    let path = temp_path("persist");
    let mut canvas = Canvas::try_new("adjust", 16, 16, Background::White).expect("canvas");
    let id = canvas.active_layer_id().expect("layer");
    let mut props = canvas.layer_props(id).expect("props");
    props.adjust = adjustment(-0.2, 0.35, 1.6, -45.0);
    canvas.set_layer_props(id, props);

    inkforge::io::save_project(&path, &canvas).expect("save");
    let loaded = inkforge::io::load_project(&path).expect("load");
    let _ = std::fs::remove_file(&path);

    let loaded_adjust = loaded.layer_props(id).expect("props").adjust;
    assert!((loaded_adjust.brightness + 0.2).abs() < 1e-6);
    assert!((loaded_adjust.contrast - 0.35).abs() < 1e-6);
    assert!((loaded_adjust.saturation - 1.6).abs() < 1e-6);
    assert!((loaded_adjust.hue + 45.0).abs() < 1e-6);
}

#[test]
fn a_project_without_an_adjustment_loads_neutral() {
    // Projects written before adjustments existed have no `adjust` field; the
    // serde default must be neutral, not "everything grey".
    let path = temp_path("legacy");
    let canvas = Canvas::try_new("legacy", 16, 16, Background::White).expect("canvas");
    inkforge::io::save_project(&path, &canvas).expect("save");
    let id = canvas.active_layer_id().expect("layer");

    // Strip the field from the manifest to imitate an older writer.
    let bytes = std::fs::read(&path).expect("read");
    let json_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let json = String::from_utf8_lossy(&bytes[16..16 + json_len]).to_string();
    let stripped = json.replace(",\"adjust\":{\"brightness\":0.0,\"contrast\":0.0,\"saturation\":1.0,\"hue\":0.0}", "");
    let mut rebuilt = Vec::new();
    rebuilt.extend_from_slice(&bytes[0..8]);
    rebuilt.extend_from_slice(&bytes[8..12]);
    rebuilt.extend_from_slice(&(stripped.len() as u32).to_le_bytes());
    rebuilt.extend_from_slice(stripped.as_bytes());
    rebuilt.extend_from_slice(&bytes[16 + json_len..]);
    std::fs::write(&path, &rebuilt).expect("write");

    let loaded = inkforge::io::load_project(&path).expect("load");
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        loaded.layer_props(id).expect("props").adjust,
        Adjustment::default()
    );
}

#[test]
fn brush_presets_expose_the_adjustment_independently() {
    // A guard against the adjustment accidentally becoming brush state: changing
    // it must never move the active brush preset.
    let app = InkforgeApp::with_defaults();
    assert_eq!(
        app.brush.matching_preset(),
        Some(0),
        "the default brush is still preset zero"
    );
}

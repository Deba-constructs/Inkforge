//! Blend-mode maths, checked channel by channel against the W3C compositing
//! formulas, plus the properties that make them usable: opacity, transparency
//! and saving.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use inkforge::egui::Color32;
use inkforge::engine::{Background, Canvas, PixelRegion};
use inkforge::layers::{composite_pixel, BlendMode, Entry, Layer, LayerStack};

const SIZE: u32 = 8;

fn opaque(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, 255)
}

/// Composite `source` over `backdrop` at full opacity, which is what a layer
/// with no pixels below it and no transparency does.
fn over(backdrop: Color32, source: Color32, mode: BlendMode) -> Color32 {
    composite_pixel(backdrop, source, mode, 1.0)
}

#[track_caller]
fn assert_pixel(pixel: Color32, expected: (u8, u8, u8), tolerance: u8, note: &str) {
    let (r, g, b) = (pixel.r(), pixel.g(), pixel.b());
    let close = |a: u8, b: u8| a.abs_diff(b) <= tolerance;
    assert!(
        close(r, expected.0) && close(g, expected.1) && close(b, expected.2),
        "{note}: expected {expected:?} (±{tolerance}), got ({r}, {g}, {b}) from {pixel:?}"
    );
}

#[test]
fn multiply_darkens() {
    let pixel = over(opaque(128, 128, 128), opaque(128, 128, 128), BlendMode::Multiply);
    assert_pixel(pixel, (64, 64, 64), 2, "multiply of two mid greys");
}

#[test]
fn screen_lightens() {
    let pixel = over(opaque(128, 128, 128), opaque(128, 128, 128), BlendMode::Screen);
    assert_pixel(pixel, (192, 192, 192), 2, "screen of two mid greys");
}

#[test]
fn overlay_uses_the_backdrop_as_its_condition() {
    // Backdrop below the midpoint doubles the product; source is irrelevant to
    // which half of the curve applies.
    let dark = over(opaque(64, 64, 64), opaque(191, 191, 191), BlendMode::Overlay);
    assert_pixel(dark, (96, 96, 96), 2, "overlay on a dark backdrop");

    let light = over(opaque(191, 191, 191), opaque(64, 64, 64), BlendMode::Overlay);
    assert_pixel(light, (159, 159, 159), 2, "overlay on a light backdrop");
}

#[test]
fn hard_light_uses_the_source_as_its_condition() {
    let pixel = over(opaque(64, 64, 64), opaque(191, 191, 191), BlendMode::HardLight);
    assert_pixel(pixel, (159, 159, 159), 2, "hard light, source above the midpoint");
}

#[test]
fn soft_light_matches_the_specified_curve() {
    // D(0.25) = 0.5, so a white source lifts a quarter-brightness backdrop to
    // exactly half.
    let pixel = over(opaque(64, 64, 64), opaque(255, 255, 255), BlendMode::SoftLight);
    assert_pixel(pixel, (128, 128, 128), 2, "soft light with a white source");
}

#[test]
fn linear_dodge_adds_and_clamps() {
    let half = over(opaque(128, 128, 128), opaque(128, 128, 128), BlendMode::LinearDodge);
    assert_pixel(half, (255, 255, 255), 1, "adding two mid greys saturates");

    let low = over(opaque(20, 20, 20), opaque(30, 30, 30), BlendMode::LinearDodge);
    assert_pixel(low, (50, 50, 50), 1, "linear dodge in range");
}

#[test]
fn colour_dodge_and_burn_are_inverses_at_the_midpoint() {
    let dodge = over(opaque(64, 64, 64), opaque(128, 128, 128), BlendMode::ColorDodge);
    assert_pixel(dodge, (128, 128, 128), 2, "colour dodge 0.25 / (1 - 0.5)");

    let burn = over(opaque(191, 191, 191), opaque(128, 128, 128), BlendMode::ColorBurn);
    assert_pixel(burn, (128, 128, 128), 2, "colour burn 1 - (0.25 / 0.5)");
}

#[test]
fn darken_lighten_difference_and_exclusion() {
    let darken = over(opaque(64, 64, 64), opaque(191, 191, 191), BlendMode::Darken);
    assert_pixel(darken, (64, 64, 64), 1, "darken keeps the lower channel");

    let lighten = over(opaque(64, 64, 64), opaque(191, 191, 191), BlendMode::Lighten);
    assert_pixel(lighten, (191, 191, 191), 1, "lighten keeps the higher channel");

    let difference = over(opaque(191, 191, 191), opaque(64, 64, 64), BlendMode::Difference);
    assert_pixel(difference, (127, 127, 127), 2, "difference");

    let exclusion = over(opaque(128, 128, 128), opaque(128, 128, 128), BlendMode::Exclusion);
    assert_pixel(exclusion, (128, 128, 128), 2, "exclusion at the midpoint");
}

#[test]
fn luminosity_takes_lightness_from_one_side_and_colour_from_the_other() {
    // Luminosity: the backdrop's colour with the source's lightness. White over
    // pure red is white at red's lightness, i.e. white.
    let luminosity = over(opaque(255, 0, 0), opaque(255, 255, 255), BlendMode::Luminosity);
    assert_pixel(luminosity, (255, 255, 255), 3, "white luminosity over red");

    // Color: the source's colour with the backdrop's lightness, so white over
    // red becomes the grey of red's luma.
    let color = over(opaque(255, 0, 0), opaque(255, 255, 255), BlendMode::Color);
    assert_pixel(color, (77, 77, 77), 4, "white colour at red's lightness");

    // Hue keeps the backdrop's saturation and lightness and takes the source's
    // hue, so green over red is a green of the same lightness.
    let hue = over(opaque(255, 0, 0), opaque(0, 255, 0), BlendMode::Hue);
    assert!(
        hue.g() > hue.r() && hue.g() > hue.b(),
        "hue of green over red should read green, got {hue:?}"
    );
}

#[test]
fn saturation_keeps_the_backdrops_lightness_and_hue() {
    let pixel = over(opaque(255, 0, 0), opaque(128, 128, 128), BlendMode::Saturation);
    assert!(
        pixel.r() >= pixel.g() && pixel.r() >= pixel.b(),
        "a desaturated source should leave the red hue in place, got {pixel:?}"
    );
}

#[test]
fn a_transparent_backdrop_leaves_the_blend_function_nothing_to_do() {
    for mode in BlendMode::ALL {
        let pixel = composite_pixel(Color32::TRANSPARENT, opaque(200, 100, 50), mode, 1.0);
        assert_pixel(pixel, (200, 100, 50), 1, mode.label());
        assert_eq!(pixel.a(), 255, "{} over nothing keeps its alpha", mode.label());
    }
}

#[test]
fn layer_opacity_scales_premultiplied_colour_as_well_as_alpha() {
    // With premultiplied storage, half-opaque white is (128, 128, 128, 128).
    // If the colour were not scaled, it would read (255, 255, 255, 128) and
    // paint far too bright over whatever is underneath.
    let pixel = composite_pixel(Color32::TRANSPARENT, Color32::WHITE, BlendMode::Normal, 0.5);
    assert!(
        (126..=129).contains(&pixel.r()),
        "half-opaque white should store premultiplied colour, got {pixel:?}"
    );
    assert!(
        (126..=129).contains(&pixel.a()),
        "half-opaque white should store half alpha, got {pixel:?}"
    );
}

// ---- through the stack ----------------------------------------------------

fn full_region() -> PixelRegion {
    PixelRegion {
        x: 0,
        y: 0,
        width: SIZE,
        height: SIZE,
    }
}

/// A layer filled with one colour on the left half, transparent on the right.
fn half_layer(id: u64, left: Color32, right: Color32) -> Layer {
    let mut layer = Layer::new(id, format!("layer {id}"), SIZE, SIZE);
    let mut pixels = Vec::with_capacity((SIZE * SIZE) as usize);
    for _ in 0..SIZE {
        for x in 0..SIZE {
            pixels.push(if x < SIZE / 2 { left } else { right });
        }
    }
    layer.surface_mut().write_region(full_region(), &pixels);
    layer
}

#[test]
fn blending_happens_when_the_stack_composites_not_before() {
    let bottom = half_layer(0, opaque(100, 100, 100), opaque(100, 100, 100));
    let mut top = half_layer(1, opaque(200, 200, 200), opaque(200, 200, 200));
    let mut props = top.props();
    props.blend = BlendMode::Multiply;
    top.set_props(props);

    let mut stack = LayerStack::from_entries(
        SIZE,
        SIZE,
        vec![Entry::Layer(bottom), Entry::Layer(top)],
        1,
    );
    let pixel = stack.composite().pixel(2, 2);
    assert_pixel(pixel, (78, 78, 78), 3, "multiply through the stack");
}

// ---- persistence ---------------------------------------------------------

fn temp_path(name: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "inkforge-blend-{}-{n}-{name}.inkforge",
        std::process::id()
    ))
}

#[test]
fn every_blend_mode_survives_a_save_and_load() {
    for mode in BlendMode::ALL {
        let path = temp_path(mode.key());
        let mut canvas =
            Canvas::try_new("blend test", 16, 16, Background::White).expect("canvas");
        let id = canvas.active_layer_id().expect("layer");
        let mut props = canvas.layer_props(id).expect("props");
        props.blend = mode;
        canvas.set_layer_props(id, props);

        inkforge::io::save_project(&path, &canvas).expect("save");
        let loaded = inkforge::io::load_project(&path).expect("load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            loaded.layer_props(id).expect("props").blend,
            mode,
            "{} did not survive the round trip",
            mode.label()
        );
    }
}

#[test]
fn an_older_projects_missing_blend_key_loads_as_normal() {
    assert_eq!(BlendMode::from_key(""), BlendMode::Normal);
    assert_eq!(BlendMode::from_key("something-else"), BlendMode::Normal);
    // "add" is what earlier INKFORGE builds wrote for linear dodge.
    assert_eq!(BlendMode::from_key("add"), BlendMode::LinearDodge);
    assert_eq!(BlendMode::from_key("Colour Dodge"), BlendMode::ColorDodge);
}

#[test]
fn every_mode_has_a_unique_key_and_a_label() {
    let mut keys: Vec<&str> = BlendMode::ALL.iter().map(|mode| mode.key()).collect();
    keys.sort_unstable();
    let count = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), count, "two modes share a key");
    assert_eq!(count, 17, "the W3C set has 17 distinct formulas");
    for mode in BlendMode::ALL {
        assert!(!mode.label().is_empty());
        assert_eq!(BlendMode::from_key(mode.key()), mode);
    }
}

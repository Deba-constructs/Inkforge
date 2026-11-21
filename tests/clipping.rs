//! Clip to layer below (alpha inheritance): a layer shows only where the layer
//! underneath has paint, through groups, hidden bases, stacking and saving.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use inkforge::app::InkforgeApp;
use inkforge::egui::Color32;
use inkforge::engine::{Background, Canvas, PixelRegion};
use inkforge::layers::{BlendMode, Entry, Layer, LayerStack};

const SIZE: u32 = 8;

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

fn clipped(mut layer: Layer) -> Layer {
    let mut props = layer.props();
    props.clip_to_below = true;
    layer.set_props(props);
    layer
}

fn stack(entries: Vec<Entry>) -> LayerStack {
    let active = entries.len() - 1;
    LayerStack::from_entries(SIZE, SIZE, entries, active)
}

/// Paint on the left half only, transparent on the right.
fn left_only(x: u32, _y: u32) -> Color32 {
    if x < SIZE / 2 {
        Color32::from_rgba_unmultiplied(255, 0, 0, 255)
    } else {
        Color32::TRANSPARENT
    }
}

#[test]
fn a_clipped_layer_only_shows_where_the_base_has_paint() {
    let base = filled(0, left_only);
    let top = clipped(filled(1, |_x, _y| Color32::from_rgba_unmultiplied(0, 0, 255, 255)));

    let mut stack = stack(vec![Entry::Layer(base), Entry::Layer(top)]);

    let inside = stack.composite().pixel(1, 1);
    assert_eq!(inside.a(), 255, "the clipped paint shows over the base");
    assert!(inside.b() > inside.r(), "and it keeps its own colour, got {inside:?}");

    let outside = stack.composite().pixel(6, 1);
    assert_eq!(
        outside.a(),
        0,
        "outside the base's paint the clipped layer must not show, got {outside:?}"
    );
}

#[test]
fn several_clipped_layers_inherit_from_the_same_base() {
    let base = filled(0, left_only);
    let first = clipped(filled(1, |_x, _y| Color32::from_rgba_unmultiplied(0, 0, 255, 255)));
    let second = clipped(filled(2, |_x, _y| Color32::from_rgba_unmultiplied(0, 255, 0, 255)));

    let mut stack = stack(vec![
        Entry::Layer(base),
        Entry::Layer(first),
        Entry::Layer(second),
    ]);

    assert_eq!(stack.composite().pixel(1, 1).a(), 255, "both children show");
    assert_eq!(
        stack.composite().pixel(6, 1).a(),
        0,
        "and neither shows outside the base"
    );
}

#[test]
fn hiding_the_base_hides_its_clipped_children() {
    let mut base = filled(0, left_only);
    let top = clipped(filled(1, |_x, _y| Color32::from_rgba_unmultiplied(0, 0, 255, 255)));

    let mut props = base.props();
    props.visible = false;
    base.set_props(props);

    let mut stack = stack(vec![Entry::Layer(base), Entry::Layer(top)]);
    assert_eq!(
        stack.composite().pixel(1, 1).a(),
        0,
        "a hidden base takes its clipped children with it"
    );
}

#[test]
fn a_clipped_layer_with_nothing_below_draws_nothing() {
    let top = clipped(filled(1, |_x, _y| Color32::from_rgba_unmultiplied(0, 0, 255, 255)));
    let mut stack = stack(vec![Entry::Layer(top)]);
    assert_eq!(stack.composite().pixel(1, 1).a(), 0);
}

#[test]
fn clipping_works_inside_a_group() {
    let base = filled(0, left_only);
    let top = clipped(filled(1, |_x, _y| Color32::from_rgba_unmultiplied(0, 0, 255, 255)));

    let mut grouped = base;
    let group_id = 99;
    grouped.set_group(Some(group_id));
    let mut child = top;
    child.set_group(Some(group_id));

    let category = inkforge::layers::Group::new(group_id, "Group");
    let mut stack = stack(vec![
        Entry::Group(category),
        Entry::Layer(grouped),
        Entry::Layer(child),
    ]);

    assert_eq!(stack.composite().pixel(1, 1).a(), 255, "inside the group");
    assert_eq!(stack.composite().pixel(6, 1).a(), 0, "clipped inside the group");

    // Folding the group away hides the whole run.
    let group = stack.group(group_id).expect("group").id();
    let mut props = stack.group_props(group).expect("props");
    props.visible = false;
    stack.set_group_props(group, props);
    assert_eq!(stack.composite().pixel(1, 1).a(), 0, "hidden group");
}

#[test]
fn a_clipped_layer_keeps_its_own_blend_mode_and_opacity() {
    // The clipped oil stroke multiplies against the base underneath.
    let base = filled(0, |_x, _y| Color32::from_rgba_unmultiplied(128, 128, 128, 255));
    let mut top = clipped(filled(1, |_x, _y| Color32::from_rgba_unmultiplied(128, 128, 128, 255)));
    let mut props = top.props();
    props.blend = BlendMode::Multiply;
    top.set_props(props);

    let mut stack = stack(vec![Entry::Layer(base), Entry::Layer(top)]);
    let pixel = stack.composite().pixel(1, 1);
    assert!(
        (60..=70).contains(&pixel.r()),
        "multiply should still apply inside the base's coverage, got {pixel:?}"
    );
}

#[test]
fn clipping_a_layer_below_transparent_pixels_shows_nothing_there() {
    // A fully transparent base is the strictest form of "no paint below".
    let base = filled(0, |_x, _y| Color32::TRANSPARENT);
    let top = clipped(filled(1, |_x, _y| Color32::from_rgba_unmultiplied(0, 0, 255, 255)));
    let mut stack = stack(vec![Entry::Layer(base), Entry::Layer(top)]);
    for x in 0..SIZE {
        assert_eq!(stack.composite().pixel(x, 4).a(), 0, "column {x}");
    }
}

// ---- app, undo and persistence -------------------------------------------

fn temp_path(name: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "inkforge-clip-{}-{n}-{name}.inkforge",
        std::process::id()
    ))
}

#[test]
fn clipping_is_one_undo_step() {
    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let id = app.canvas.active_layer_id().expect("layer");

    assert!(
        !app.canvas.layer_props(id).expect("props").clip_to_below,
        "a new layer starts unclipped"
    );

    app.set_layer_clip(id, true);
    assert!(app.canvas.layer_props(id).expect("props").clip_to_below);

    app.undo();
    assert!(
        !app.canvas.layer_props(id).expect("props").clip_to_below,
        "undo must take the clip back without touching pixels"
    );

    app.redo();
    assert!(app.canvas.layer_props(id).expect("props").clip_to_below);
}

#[test]
fn clipping_survives_saving_and_loading() {
    let path = temp_path("persist");
    let mut canvas =
        Canvas::try_new("clip test", 16, 16, Background::White).expect("canvas");
    canvas.layer_add_above_active();
    let id = canvas.active_layer_id().expect("layer");
    let mut props = canvas.layer_props(id).expect("props");
    props.clip_to_below = true;
    props.blend = BlendMode::Multiply;
    canvas.set_layer_props(id, props);

    inkforge::io::save_project(&path, &canvas).expect("save");
    let loaded = inkforge::io::load_project(&path).expect("load");
    let _ = std::fs::remove_file(&path);

    let props = loaded.layer_props(id).expect("props");
    assert!(props.clip_to_below, "the clip must survive saving");
    assert_eq!(props.blend, BlendMode::Multiply, "and the blend mode too");
}

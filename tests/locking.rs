//! Tests for "Lock transparent pixels" (CSP's alpha lock) on a layer.

use std::path::PathBuf;

use inkforge::egui::{pos2, Color32};
use inkforge::engine::{Background, BrushSettings, Canvas, ShapeKind};
use inkforge::io::{load_project, save_project};

fn brush(size: f32) -> BrushSettings {
    let mut brush = BrushSettings::default();
    brush.set_size(size);
    brush.set_hardness(1.0);
    brush.set_opacity(1.0);
    brush
}

fn canvas(width: u32, height: u32) -> Canvas {
    Canvas::try_new("Test", width, height, Background::Transparent).expect("valid canvas")
}

fn layer(canvas: &Canvas) -> u64 {
    canvas.active_layer_id().expect("an active paint layer")
}

fn alpha(canvas: &Canvas, x: u32, y: u32) -> u8 {
    canvas
        .layer_surface(layer(canvas))
        .expect("layer surface")
        .pixel(x, y)
        .a()
}

fn pixel(canvas: &Canvas, x: u32, y: u32) -> Color32 {
    canvas
        .layer_surface(layer(canvas))
        .expect("layer surface")
        .pixel(x, y)
}

fn set_locked(canvas: &mut Canvas, locked: bool) {
    let id = layer(canvas);
    let mut props = canvas.layer_props(id).expect("layer properties");
    props.alpha_locked = locked;
    canvas.set_layer_props(id, props);
}

/// An opaque red disc centred on (32, 32), radius 10.
fn red_disc(canvas: &mut Canvas) {
    let id = layer(canvas);
    let dab = brush(20.0).dab(Color32::from_rgb(220, 30, 30), false);
    canvas.paint_layer(id, pos2(32.0, 32.0), pos2(32.0, 32.0), &dab);
    assert_eq!(alpha(canvas, 32, 32), 255, "the disc should be opaque");
}

fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "inkforge-lock-{}-{name}-{nanos}.inkforge",
        std::process::id()
    ))
}

#[test]
fn the_lock_is_off_by_default() {
    let canvas = canvas(64, 64);
    let props = canvas.layer_props(layer(&canvas)).expect("properties");
    assert!(!props.alpha_locked);
}

#[test]
fn locked_paint_recolours_existing_paint_without_growing_it() {
    let mut canvas = canvas(96, 64);
    red_disc(&mut canvas);
    set_locked(&mut canvas, true);

    let id = layer(&canvas);
    let dab = brush(10.0).dab(Color32::from_rgb(20, 30, 220), false);
    canvas.paint_layer(id, pos2(0.0, 32.0), pos2(95.0, 32.0), &dab);

    // Inside the disc: recoloured, alpha untouched.
    let centre = pixel(&canvas, 32, 32);
    assert_eq!(centre.a(), 255, "the lock must keep the alpha channel");
    assert!(
        centre.b() > 180 && centre.r() < 60,
        "the disc should take the new colour, got {centre:?}"
    );

    // Outside the disc: no new pixels, even though the stroke ran over it.
    assert_eq!(alpha(&canvas, 80, 32), 0);
    assert_eq!(alpha(&canvas, 5, 32), 0);
}

#[test]
fn locked_eraser_cannot_remove_paint() {
    let mut canvas = canvas(64, 64);
    red_disc(&mut canvas);
    set_locked(&mut canvas, true);

    let id = layer(&canvas);
    let eraser = brush(20.0).dab(Color32::BLACK, true);
    canvas.paint_layer(id, pos2(32.0, 32.0), pos2(32.0, 32.0), &eraser);

    let kept = pixel(&canvas, 32, 32);
    assert_eq!(kept.a(), 255, "a locked layer keeps its coverage");
    assert!(kept.r() > 180, "the colour must not be erased either");
}

#[test]
fn locked_fill_spares_transparent_pixels() {
    let mut canvas = canvas(64, 64);
    red_disc(&mut canvas);
    set_locked(&mut canvas, true);
    canvas.select_all();

    let id = layer(&canvas);
    canvas
        .fill_selection(id, Color32::from_rgb(20, 200, 60))
        .expect("a selection to fill");

    let inside = pixel(&canvas, 32, 32);
    assert!(inside.g() > 180 && inside.r() < 60, "got {inside:?}");
    assert_eq!(inside.a(), 255);
    assert_eq!(alpha(&canvas, 6, 6), 0, "the fill must not create pixels");
}

#[test]
fn unlocking_lets_paint_reach_new_pixels_again() {
    let mut canvas = canvas(96, 64);
    red_disc(&mut canvas);
    set_locked(&mut canvas, true);
    set_locked(&mut canvas, false);

    let id = layer(&canvas);
    let dab = brush(10.0).dab(Color32::from_rgb(20, 30, 220), false);
    canvas.paint_layer(id, pos2(80.0, 32.0), pos2(80.0, 32.0), &dab);

    assert_eq!(alpha(&canvas, 80, 32), 255);
}

#[test]
fn locked_shapes_stay_inside_existing_paint() {
    let mut canvas = canvas(96, 64);
    red_disc(&mut canvas);
    set_locked(&mut canvas, true);

    let dab = brush(8.0).dab(Color32::from_rgb(240, 240, 40), false);
    canvas.paint_shape(ShapeKind::Line, pos2(0.0, 32.0), pos2(95.0, 32.0), &dab);

    assert_eq!(alpha(&canvas, 32, 32), 255, "the disc is recoloured");
    assert_eq!(alpha(&canvas, 70, 32), 0, "nothing outside the disc appears");
}

#[test]
fn the_lock_survives_a_project_round_trip() {
    let mut canvas = canvas(64, 64);
    red_disc(&mut canvas);
    set_locked(&mut canvas, true);

    let path = scratch("round-trip");
    save_project(&path, &canvas).expect("save");

    let loaded = load_project(&path).expect("load");
    let props = loaded
        .layer_props(layer(&loaded))
        .expect("layer properties after loading");
    assert!(props.alpha_locked, "the lock must survive saving");

    let _ = std::fs::remove_file(&path);
}

//! Unit tests for the document model and the viewport transform.

use inkforge::egui::{pos2, vec2, Rect};
use inkforge::engine::{
    validate_canvas_size, Background, BrushSettings, Canvas, Viewport, MAX_CANVAS_DIM,
    MAX_CANVAS_PIXELS, MAX_SIZE, MIN_CANVAS_DIM,
};

fn view() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))
}

#[test]
fn canvas_rejects_out_of_range_sizes() {
    assert!(Canvas::try_new("Too narrow", 0, 600, Background::White).is_err());
    assert!(Canvas::try_new("Too tall", 600, MAX_CANVAS_DIM + 1, Background::White).is_err());
    assert!(Canvas::try_new("Smallest", MIN_CANVAS_DIM, MIN_CANVAS_DIM, Background::White).is_ok());

    // The area limit is checked without allocating, so a 400 MP request is a
    // readable error rather than a failed allocation.
    assert!(validate_canvas_size(MAX_CANVAS_DIM, MAX_CANVAS_DIM).is_err());
    let pixels = MAX_CANVAS_DIM as u64 * MAX_CANVAS_DIM as u64;
    assert!(pixels > MAX_CANVAS_PIXELS);

    // Tall pages the presets actually offer must pass: webtoon and A0 at 300 dpi.
    assert!(validate_canvas_size(800, 12_800).is_ok());
    assert!(validate_canvas_size(9_933, 14_043).is_ok());
    assert!(validate_canvas_size(7_280, 5_150).is_ok());
}

#[test]
fn blank_canvas_name_falls_back_to_untitled() {
    let canvas = Canvas::try_new("   ", 800, 600, Background::Transparent).unwrap();
    assert_eq!(canvas.name(), "Untitled");
    assert_eq!(canvas.dimensions_label(), "800 × 600 px");
    assert!(canvas.background().fill().is_none());
}

#[test]
fn fit_centres_the_page_without_upscaling_past_the_window() {
    let doc = vec2(1920.0, 1080.0);
    let mut viewport = Viewport::new();
    viewport.fit(view(), doc);

    let page = viewport.page_rect(view(), doc);
    assert!((page.center() - view().center()).length() < 0.01);
    assert!((page.width() / page.height() - 16.0 / 9.0).abs() < 0.001);
    assert!(page.width() <= view().width() + 0.01);
    assert!(page.height() <= view().height() + 0.01);
}

#[test]
fn zooming_at_the_cursor_keeps_the_canvas_point_under_it() {
    let doc = vec2(1200.0, 800.0);
    let mut viewport = Viewport::new();
    viewport.fit(view(), doc);

    let anchor = pos2(312.0, 244.0);
    let before = viewport.screen_to_canvas(view(), doc, anchor);
    viewport.zoom_at(1.7, anchor, view());
    let after = viewport.screen_to_canvas(view(), doc, anchor);

    assert!(
        (before - after).length() < 0.05,
        "anchor drifted: {before:?} → {after:?}"
    );
}

#[test]
fn screen_and_canvas_coordinates_round_trip() {
    let doc = vec2(1024.0, 768.0);
    let mut viewport = Viewport::new();
    viewport.fit(view(), doc);
    viewport.pan_by(vec2(37.0, -19.0));
    viewport.zoom_at(1.3, pos2(400.0, 300.0), view());

    let canvas_point = pos2(512.5, 384.5);
    let screen = viewport.canvas_to_screen(view(), doc, canvas_point);
    let back = viewport.screen_to_canvas(view(), doc, screen);

    assert!((canvas_point - back).length() < 0.01);
}

#[test]
fn zoom_stays_within_limits() {
    let mut viewport = Viewport::new();
    for _ in 0..200 {
        viewport.zoom_in();
    }
    assert!(viewport.zoom() <= 32.0 + f32::EPSILON);

    for _ in 0..400 {
        viewport.zoom_out();
    }
    assert!(viewport.zoom() >= 0.02 - f32::EPSILON);
}

#[test]
fn brush_rejects_non_finite_input() {
    let mut brush = BrushSettings::default();
    brush.set_size(f32::NAN);
    assert_eq!(brush.size(), 1.0);

    brush.set_size(10_000.0);
    assert_eq!(brush.size(), MAX_SIZE);

    brush.set_opacity(f32::INFINITY);
    assert_eq!(brush.opacity(), 1.0);

    brush.set_smoothing(0.5);
    brush.scale_size(2.0);
    assert!(brush.size() > 1.0);
}

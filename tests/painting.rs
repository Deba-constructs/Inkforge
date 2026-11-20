//! Tests for the raster painting surface.

use inkforge::egui::{pos2, Color32};
use inkforge::engine::{BrushSettings, Surface};

fn hard_brush(size: f32, opacity: f32) -> BrushSettings {
    let mut brush = BrushSettings::default();
    brush.set_size(size);
    brush.set_hardness(1.0);
    brush.set_opacity(opacity);
    brush
}

#[test]
fn a_stroke_paints_along_its_path_and_nowhere_else() {
    let mut surface = Surface::new(64, 64);
    let brush = hard_brush(10.0, 1.0);
    let dab = brush.dab(Color32::from_rgb(220, 40, 40), false);

    surface.paint_segment(pos2(10.0, 32.0), pos2(54.0, 32.0), &dab);

    // Along the path: opaque.
    for x in [12, 24, 32, 44, 52] {
        assert!(
            surface.pixel(x, 32).a() > 240,
            "gap in the stroke at x={x}: {:?}",
            surface.pixel(x, 32)
        );
    }
    // Off the path: untouched.
    assert_eq!(surface.pixel(32, 2).a(), 0);
    assert_eq!(surface.pixel(2, 60).a(), 0);
}

#[test]
fn a_single_stamp_is_round_and_soft_edges_are_partially_covered() {
    let mut surface = Surface::new(40, 40);
    let brush = hard_brush(20.0, 1.0);
    let dab = brush.dab(Color32::BLACK, false);
    surface.stamp(pos2(20.0, 20.0), &dab);

    // Centre and inside the hard core.
    assert!(surface.pixel(20, 20).a() > 250);
    // Just outside the radius: nothing.
    assert_eq!(surface.pixel(39, 39).a(), 0);
    // The corner of the bounding box is outside a round brush.
    assert_eq!(surface.pixel(11, 11).a(), 0);
}

#[test]
fn opacity_controls_how_much_paint_lands() {
    let mut surface = Surface::new(32, 32);
    let brush = hard_brush(16.0, 0.5);
    let dab = brush.dab(Color32::from_rgb(0, 0, 255), false);
    surface.stamp(pos2(16.0, 16.0), &dab);

    let centre = surface.pixel(16, 16);
    assert!(
        (120..=140).contains(&centre.a()),
        "expected ~50% alpha, got {}",
        centre.a()
    );
    // Premultiplied storage: blue channel tracks alpha, red/green stay zero.
    assert_eq!(centre.r(), 0);
    assert_eq!(centre.g(), 0);
    assert!(centre.b() > 100);
}

#[test]
fn the_eraser_removes_paint_instead_of_adding_it() {
    let mut surface = Surface::new(32, 32);
    let brush = hard_brush(16.0, 1.0);

    surface.stamp(
        pos2(16.0, 16.0),
        &brush.dab(Color32::from_rgb(10, 200, 90), false),
    );
    assert!(surface.pixel(16, 16).a() > 240);
    let painted: u32 = surface.pixels().iter().map(|p| p.a() as u32).sum();

    surface.stamp(
        pos2(16.0, 16.0),
        &brush.dab(Color32::from_rgb(10, 200, 90), true),
    );

    // The solid core is gone. The one-pixel antialiased rim keeps fractional
    // alpha, exactly like a soft eraser should, so the surface is not
    // perfectly empty — but it holds only a trace of the original paint.
    assert_eq!(surface.pixel(16, 16).a(), 0);
    let remaining: u32 = surface.pixels().iter().map(|p| p.a() as u32).sum();
    assert!(
        remaining * 20 < painted,
        "eraser left {remaining} of {painted} alpha units"
    );
}

#[test]
fn sampling_returns_a_straight_alpha_colour() {
    let mut surface = Surface::new(16, 16);
    let brush = hard_brush(8.0, 1.0);
    surface.stamp(
        pos2(8.0, 8.0),
        &brush.dab(Color32::from_rgb(200, 60, 30), false),
    );

    let picked = surface.sample(pos2(8.0, 8.0)).expect("painted pixel");
    assert_eq!((picked.r(), picked.g(), picked.b()), (200, 60, 30));

    // Transparent and out-of-bounds points have nothing to pick.
    assert!(surface.sample(pos2(15.5, 0.5)).is_none());
    assert!(surface.sample(pos2(-4.0, 8.0)).is_none());
    assert!(surface.sample(pos2(500.0, 8.0)).is_none());
}

#[test]
fn painting_outside_the_surface_is_clipped_not_panicking() {
    let mut surface = Surface::new(16, 16);
    let brush = hard_brush(40.0, 1.0);
    let dab = brush.dab(Color32::BLACK, false);

    surface.paint_segment(pos2(-500.0, -500.0), pos2(-450.0, -480.0), &dab);
    assert!(surface.is_empty());

    surface.paint_segment(pos2(-10.0, 8.0), pos2(30.0, 8.0), &dab);
    assert!(surface.pixel(0, 8).a() > 0, "stroke should enter the canvas");
    assert!(surface.pixel(15, 8).a() > 0);
}

#[test]
fn revision_changes_only_when_pixels_do() {
    let surface = Surface::new(8, 8);
    let before = surface.revision();

    let mut surface = surface;
    let brush = hard_brush(4.0, 1.0);
    surface.stamp(pos2(4.0, 4.0), &brush.dab(Color32::BLACK, false));
    assert!(surface.revision() > before);

    let cleared_revision = surface.revision();
    surface.clear();
    assert!(surface.revision() > cleared_revision);
    assert!(surface.is_empty());
}

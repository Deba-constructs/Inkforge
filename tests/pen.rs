//! Tests for tablet pressure and tilt shaping the brush.

use inkforge::app::InkforgeApp;
use inkforge::egui::{pos2, Vec2};
use inkforge::engine::{PenSample, Surface, MAX_SIZE, MIN_DAB_RADIUS};

/// An app with a hard, fully opaque, 40 px brush, so a dab's footprint is its
/// radius and its alpha is its opacity.
fn app_with(size: f32) -> InkforgeApp {
    let mut app = InkforgeApp::with_defaults();
    app.brush.set_size(size);
    app.brush.set_hardness(1.0);
    app.brush.set_opacity(1.0);
    app
}

#[test]
fn a_mouse_uses_the_brush_exactly_as_configured() {
    let app = app_with(40.0);
    let dab = app.current_dab(false);

    assert_eq!(dab.radius, 20.0);
    assert_eq!(dab.opacity, 1.0);
}

#[test]
fn a_light_touch_paints_a_thinner_and_fainter_mark() {
    let mut app = app_with(40.0);
    let mut full = Surface::new(64, 64);
    full.stamp(pos2(32.0, 32.0), &app.current_dab(false));

    app.set_pen_sample(PenSample::pen(0.5, Vec2::ZERO));
    let mut half = Surface::new(64, 64);
    half.stamp(pos2(32.0, 32.0), &app.current_dab(false));

    assert!(
        half.pixel(32, 32).a() < full.pixel(32, 32).a(),
        "half pressure must be fainter: {} vs {}",
        half.pixel(32, 32).a(),
        full.pixel(32, 32).a()
    );
    assert!(full.pixel(48, 32).a() > 0, "the full dab reaches 16 px out");
    assert_eq!(
        half.pixel(48, 32).a(),
        0,
        "the half-pressure dab must stop short of it"
    );
}

#[test]
fn a_feather_touch_still_leaves_a_hairline() {
    let mut app = app_with(40.0);
    app.set_pen_sample(PenSample::pen(0.0, Vec2::ZERO));
    let dab = app.current_dab(false);

    assert!(dab.radius >= MIN_DAB_RADIUS);
    assert!(dab.radius < 4.0, "radius {} is no longer a fine line", dab.radius);
    assert!(dab.opacity > 0.0, "a touch must still put down some paint");
}

#[test]
fn tilt_widens_the_tip_without_darkening_it() {
    let mut app = app_with(40.0);
    let upright = app.current_dab(false);

    // 60° is the angle that counts as fully tilted.
    app.set_pen_sample(PenSample::pen(1.0, Vec2::new(0.0, 60.0)));
    let leaned = app.current_dab(false);

    assert!(
        (leaned.radius - 26.0).abs() < 0.01,
        "the default 30% tilt amount should widen 20 px to 26 px, got {}",
        leaned.radius
    );
    assert_eq!(leaned.opacity, upright.opacity);
}

#[test]
fn tilt_never_pushes_the_brush_past_its_largest_size() {
    let mut app = app_with(MAX_SIZE);
    app.pen.set_size_from_tilt(1.0);
    app.set_pen_sample(PenSample::pen(1.0, Vec2::new(-90.0, -90.0)));

    assert!(app.current_dab(false).radius <= MAX_SIZE * 0.5);
}

#[test]
fn zero_percent_amounts_ignore_the_pen_entirely() {
    let mut app = app_with(40.0);
    app.pen.set_size_from_pressure(0.0);
    app.pen.set_opacity_from_pressure(0.0);
    app.pen.set_size_from_tilt(0.0);
    app.set_pen_sample(PenSample::pen(0.05, Vec2::new(45.0, -45.0)));

    let dab = app.current_dab(false);
    assert_eq!(dab.radius, 20.0);
    assert_eq!(dab.opacity, 1.0);
}

#[test]
fn switching_tablet_input_off_restores_the_plain_brush() {
    let mut app = app_with(40.0);
    app.pen.set_enabled(false);
    app.set_pen_sample(PenSample::pen(0.1, Vec2::new(60.0, 0.0)));

    let dab = app.current_dab(false);
    assert_eq!(dab.radius, 20.0);
    assert_eq!(dab.opacity, 1.0);
}

#[test]
fn a_bad_reading_falls_back_to_full_pressure() {
    let sample = PenSample::pen(f32::NAN, Vec2::new(f32::INFINITY, f32::NAN));

    assert_eq!(sample.pressure(), 1.0);
    assert_eq!(sample.tilt(), Vec2::ZERO);

    let mut app = app_with(40.0);
    app.set_pen_sample(sample);
    assert_eq!(app.current_dab(false).radius, 20.0);
}

#[test]
fn a_stroke_follows_the_pressure_it_is_painted_at() {
    let mut app = app_with(40.0);

    // A full-pressure stroke leaves paint 18 px above its path.
    app.set_pen_sample(PenSample::MOUSE);
    app.begin_stroke(pos2(400.0, 400.0));
    app.extend_stroke(pos2(460.0, 400.0));
    app.end_stroke();
    assert!(
        app.canvas.composite_mut().sample(pos2(430.0, 418.0)).is_some(),
        "the full-pressure stroke should cover the sample point"
    );

    // The same path at 25% pressure is thinner than that.
    app.set_pen_sample(PenSample::pen(0.25, Vec2::ZERO));
    app.begin_stroke(pos2(400.0, 500.0));
    app.extend_stroke(pos2(460.0, 500.0));
    app.end_stroke();
    assert!(
        app.canvas.composite_mut().sample(pos2(430.0, 518.0)).is_none(),
        "the light stroke must not reach 18 px above its path"
    );
}

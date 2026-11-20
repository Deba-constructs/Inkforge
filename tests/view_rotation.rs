//! Canvas rotation: the viewport transform that turns the sheet, and the
//! pointer mapping that has to keep painting under the cursor while it is
//! turned.
//!
//! The maths is checked directly (a stroke's landing point is decided by these
//! two functions), including the inverse mapping the canvas view uses for every
//! pointer position and the cursor-anchored zoom that must not drift.

use inkforge::egui::{pos2, vec2, Rect};
use inkforge::engine::Viewport;

fn view_rect() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0))
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

fn assert_point(actual: inkforge::egui::Pos2, expected: (f32, f32)) {
    assert!(
        close(actual.x, expected.0) && close(actual.y, expected.1),
        "expected ({}, {}), got ({}, {})",
        expected.0,
        expected.1,
        actual.x,
        actual.y
    );
}

#[test]
fn a_fresh_view_is_upright() {
    let view = Viewport::new();
    assert_eq!(view.rotation(), 0.0);
    assert!(!view.rotated());
    assert_eq!(view.rotation_label(), "0°");
}

#[test]
fn a_quarter_turn_moves_the_page_corners_the_way_it_looks() {
    let mut view = Viewport::new();
    view.fit(view_rect(), vec2(400.0, 200.0));
    // Fit leaves the page centred, so the mapping is easy to reason about.
    view.set_rotation(std::f32::consts::FRAC_PI_2);

    // A 90° turn clockwise sends the page's top-left corner to where its
    // top-right corner was, relative to the page centre.
    let centre = view.canvas_to_screen(view_rect(), vec2(400.0, 200.0), pos2(200.0, 100.0));
    assert_point(centre, (500.0, 400.0));

    let top_left = view.canvas_to_screen(view_rect(), vec2(400.0, 200.0), pos2(0.0, 0.0));
    assert!(
        top_left.x > centre.x && top_left.y < centre.y,
        "top-left should land up and to the right of centre, got {top_left:?}"
    );

    let top_right = view.canvas_to_screen(view_rect(), vec2(400.0, 200.0), pos2(400.0, 0.0));
    assert!(
        top_right.x > centre.x && top_right.y > centre.y,
        "top-right should land down and to the right of centre, got {top_right:?}"
    );
}

#[test]
fn screen_and_canvas_stay_exact_inverses_at_every_angle() {
    let doc = vec2(517.0, 211.0);
    for degrees in [-150.0_f32, -90.0, -37.0, 0.0, 12.0, 90.0, 133.0, 179.0] {
        let mut view = Viewport::new();
        view.fit(view_rect(), doc);
        view.zoom_at(1.7, pos2(420.0, 300.0), view_rect());
        view.pan_by(vec2(35.0, -22.0));
        view.set_rotation(degrees.to_radians());

        for canvas in [
            pos2(0.0, 0.0),
            pos2(517.0, 211.0),
            pos2(258.5, 105.5),
            pos2(400.0, 30.0),
        ] {
            let screen = view.canvas_to_screen(view_rect(), doc, canvas);
            let back = view.screen_to_canvas(view_rect(), doc, screen);
            assert!(
                close(back.x, canvas.x) && close(back.y, canvas.y),
                "{degrees}°: {canvas:?} → {screen:?} → {back:?}"
            );
        }
    }
}

#[test]
fn turning_the_view_keeps_the_point_under_the_cursor_while_zooming() {
    let doc = vec2(800.0, 600.0);
    let mut view = Viewport::new();
    view.fit(view_rect(), doc);
    view.set_rotation(0.7);

    let anchor = pos2(612.0, 333.0);
    let before = view.screen_to_canvas(view_rect(), doc, anchor);
    view.zoom_at(1.8, anchor, view_rect());
    let after = view.screen_to_canvas(view_rect(), doc, anchor);
    assert!(
        close(before.x, after.x) && close(before.y, after.y),
        "cursor-anchored zoom drifted: {before:?} → {after:?}"
    );
}

#[test]
fn rotation_accumulates_and_stays_within_half_a_turn() {
    let mut view = Viewport::new();
    view.rotate_by(0.3);
    view.rotate_by(0.2);
    assert!(close(view.rotation(), 0.5));

    view.rotate_by(std::f32::consts::PI * 1.9);
    assert!(
        view.rotation() > -std::f32::consts::PI && view.rotation() <= std::f32::consts::PI,
        "angle not wrapped: {}",
        view.rotation()
    );
}

#[test]
fn angles_near_a_right_angle_snap_onto_it() {
    let mut view = Viewport::new();

    view.set_rotation(std::f32::consts::FRAC_PI_2 + 0.01);
    assert_eq!(view.rotation(), std::f32::consts::FRAC_PI_2);

    view.set_rotation(0.01);
    assert_eq!(view.rotation(), 0.0);
    assert_eq!(view.rotation_label(), "0°");

    // A deliberate angle is left alone.
    view.set_rotation(0.5);
    assert!(close(view.rotation(), 0.5));
}

#[test]
fn resetting_puts_the_canvas_back_upright() {
    let mut view = Viewport::new();
    view.set_rotation(0.8);
    assert!(view.rotated());
    view.reset_rotation();
    assert!(!view.rotated());
    assert_eq!(view.rotation(), 0.0);
}

#[test]
fn nonsense_angles_are_ignored() {
    let mut view = Viewport::new();
    view.set_rotation(0.4);
    view.set_rotation(f32::NAN);
    view.set_rotation(f32::INFINITY);
    assert!(close(view.rotation(), 0.4));
}

#[test]
fn fitting_a_turned_page_uses_its_turned_bounds() {
    // A wide page turned on its side needs the fit to think height, not width.
    let doc = vec2(1000.0, 200.0);
    let view_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 600.0));

    let mut upright = Viewport::new();
    upright.fit(view_rect, doc);
    let upright_zoom = upright.zoom();

    let mut turned = Viewport::new();
    turned.set_rotation(std::f32::consts::FRAC_PI_2);
    turned.fit(view_rect, doc);

    assert!(
        turned.zoom() < upright_zoom,
        "a page turned on its side must fit into the height: {} vs {}",
        turned.zoom(),
        upright_zoom
    );
    // 600 px of view × 0.88 margin over the 1000 px the turned page is tall.
    assert!(close(turned.zoom(), 0.528), "got {}", turned.zoom());
}

#[test]
fn the_page_rectangle_itself_is_not_turned() {
    let mut view = Viewport::new();
    view.fit(view_rect(), vec2(400.0, 200.0));
    let before = view.page_rect(view_rect(), vec2(400.0, 200.0));
    view.set_rotation(0.6);
    let after = view.page_rect(view_rect(), vec2(400.0, 200.0));
    assert!(close(before.width(), after.width()));
    assert!(close(before.height(), after.height()));
    assert!(close(before.center().x, after.center().x));
}

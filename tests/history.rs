//! Tests for tile-based undo/redo, now that edits live inside layers.

use inkforge::egui::{pos2, Color32, Pos2};
use inkforge::engine::{Background, BrushSettings, Canvas};
use inkforge::history::{Edit, History, Recorder};

fn brush() -> BrushSettings {
    let mut brush = BrushSettings::default();
    brush.set_size(10.0);
    brush.set_hardness(1.0);
    brush.set_opacity(1.0);
    brush
}

fn canvas(width: u32, height: u32) -> Canvas {
    Canvas::try_new("Test", width, height, Background::Transparent).expect("valid canvas")
}

/// Alpha of a pixel on the active layer.
fn layer_alpha(canvas: &Canvas, x: u32, y: u32) -> u8 {
    canvas
        .layer_surface(canvas.active_layer_id().expect("an active layer"))
        .expect("active layer surface")
        .pixel(x, y)
        .a()
}

/// Paint one dab at `at` and record it as a single undoable edit.
fn paint(canvas: &mut Canvas, history: &mut History, at: Pos2, erase: bool) -> bool {
    let dab = brush().dab(Color32::from_rgb(200, 30, 30), erase);
    let layer = canvas.active_layer_id().expect("an active layer");

    let mut recorder = Recorder::new();
    if let Some(surface) = canvas.layer_surface(layer) {
        recorder.record_segment(surface, at, at, dab.radius);
    }
    canvas.paint_layer(layer, at, at, &dab);

    let patches = match canvas.layer_surface(layer) {
        Some(surface) => recorder.finish(surface),
        None => return false,
    };
    if patches.is_empty() {
        return false;
    }

    let label = if erase { "erase" } else { "brush stroke" };
    history.push(Edit::pixels(layer, patches, label));
    true
}

#[test]
fn a_stroke_can_be_undone_and_redone() {
    let mut canvas = canvas(64, 64);
    let mut history = History::new();

    assert!(!history.can_undo());
    assert!(paint(&mut canvas, &mut history, pos2(32.0, 32.0), false));
    assert!(layer_alpha(&canvas, 32, 32) > 240);
    assert!(history.can_undo());
    assert!(!history.can_redo());

    assert_eq!(history.undo(&mut canvas), Some("brush stroke"));
    assert_eq!(layer_alpha(&canvas, 32, 32), 0);
    assert!(history.can_redo());

    assert_eq!(history.redo(&mut canvas), Some("brush stroke"));
    assert!(layer_alpha(&canvas, 32, 32) > 240);
}

#[test]
fn undo_restores_only_the_painted_area() {
    let mut canvas = canvas(96, 96);
    let mut history = History::new();

    paint(&mut canvas, &mut history, pos2(20.0, 20.0), false);
    paint(&mut canvas, &mut history, pos2(70.0, 70.0), false);

    // Undo the second dab only.
    history.undo(&mut canvas);
    assert!(layer_alpha(&canvas, 20, 20) > 240, "first dab must survive");
    assert_eq!(layer_alpha(&canvas, 70, 70), 0, "second dab must be gone");
}

#[test]
fn erasing_is_undoable_too() {
    let mut canvas = canvas(48, 48);
    let mut history = History::new();

    paint(&mut canvas, &mut history, pos2(24.0, 24.0), false);
    paint(&mut canvas, &mut history, pos2(24.0, 24.0), true);
    assert_eq!(layer_alpha(&canvas, 24, 24), 0);

    history.undo(&mut canvas);
    assert!(
        layer_alpha(&canvas, 24, 24) > 240,
        "undoing an erase must bring the paint back"
    );

    history.undo(&mut canvas);
    assert_eq!(layer_alpha(&canvas, 24, 24), 0);
}

#[test]
fn a_new_edit_clears_the_redo_stack() {
    let mut canvas = canvas(64, 64);
    let mut history = History::new();

    paint(&mut canvas, &mut history, pos2(16.0, 16.0), false);
    history.undo(&mut canvas);
    assert!(history.can_redo());

    paint(&mut canvas, &mut history, pos2(48.0, 48.0), false);
    assert!(!history.can_redo(), "redo must not survive a new edit");
}

#[test]
fn edits_that_change_nothing_are_not_recorded() {
    let mut canvas = canvas(32, 32);
    let mut history = History::new();

    // Entirely off-canvas: no tiles touched, no history entry.
    assert!(!paint(&mut canvas, &mut history, pos2(-500.0, -500.0), false));
    assert!(!history.can_undo());
}

#[test]
fn history_is_capped_by_memory_and_keeps_the_newest_edits() {
    let mut canvas = canvas(128, 128);
    let mut history = History::with_memory_limit(1);

    for i in 0..5 {
        let at = pos2(10.0 + i as f32 * 16.0, 10.0);
        assert!(paint(&mut canvas, &mut history, at, false));
    }

    assert_eq!(
        history.undo_depth(),
        1,
        "a 1-byte cap may keep only the newest edit"
    );
    assert!(history.can_undo());

    // The newest edit is the one that undoes.
    assert_eq!(history.undo(&mut canvas), Some("brush stroke"));
    assert_eq!(layer_alpha(&canvas, 74, 10), 0);
}

#[test]
fn undo_with_empty_history_is_a_no_op() {
    let mut canvas = canvas(16, 16);
    let mut history = History::new();
    assert_eq!(history.undo(&mut canvas), None);
    assert_eq!(history.redo(&mut canvas), None);
    assert!(!history.can_undo());
    assert!(!history.can_redo());
}

#[test]
fn a_stroke_only_touches_the_layer_it_started_on() {
    let mut canvas = canvas(64, 64);
    let lower = canvas.active_layer_id().expect("an active layer");
    canvas.layer_add_above_active();
    let upper = canvas.active_layer_id().expect("an active layer");

    canvas.paint_layer(upper, pos2(32.0, 32.0), pos2(32.0, 32.0), &brush().dab(Color32::RED, false));

    assert_eq!(canvas.layer_surface(lower).unwrap().pixel(32, 32).a(), 0);
    assert!(canvas.layer_surface(upper).unwrap().pixel(32, 32).a() > 240);
}

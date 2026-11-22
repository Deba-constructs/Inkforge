//! Tests for selection combine modes, the polygon lasso and selections that
//! travel with a flipped or turned document.

use inkforge::app::{InkforgeApp, Tool};
use inkforge::egui::pos2;
use inkforge::engine::{
    Background, BrushSettings, Canvas, Selection, SelectionCombine, SelectionMode, Transform,
};

fn canvas(width: u32, height: u32) -> Canvas {
    Canvas::try_new("Test", width, height, Background::White).expect("valid canvas")
}

fn app(width: u32, height: u32) -> InkforgeApp {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = canvas(width, height);
    app.view.request_fit();
    let mut brush = BrushSettings::default();
    brush.set_size(6.0);
    app.brush = brush;
    app
}

fn rect(a: (f32, f32), b: (f32, f32)) -> Selection {
    Selection::from_corners(pos2(a.0, a.1), pos2(b.0, b.1), 128, 128)
        .expect("a usable rectangle")
}

#[test]
fn adding_a_selection_keeps_both_areas() {
    let mut app = app(128, 128);
    app.select_tool(Tool::Selection);

    app.begin_selection_drag(pos2(10.0, 10.0));
    app.update_selection_drag(pos2(40.0, 40.0));
    app.end_selection_drag();
    assert!(app.canvas.selection().is_some());

    app.set_selection_combine(SelectionCombine::Add);
    app.begin_selection_drag(pos2(80.0, 80.0));
    app.update_selection_drag(pos2(110.0, 110.0));
    app.end_selection_drag();

    let selection = app.canvas.selection().expect("combined selection");
    assert!(selection.contains(20, 20), "the first area is still selected");
    assert!(selection.contains(95, 95), "the second area was added");
    assert!(!selection.contains(60, 60), "the gap stays unselected");

    // One undo step for the second selection, and undo puts the first back.
    app.history.undo(&mut app.canvas);
    let restored = app.canvas.selection().expect("first selection restored");
    assert!(restored.contains(20, 20));
    assert!(!restored.contains(95, 95));
}

#[test]
fn subtracting_and_intersecting_cut_the_selection_down() {
    let outer = rect((10.0, 10.0), (100.0, 100.0));
    let inner = rect((40.0, 40.0), (70.0, 70.0));

    let subtracted = outer
        .combined(&inner, SelectionCombine::Subtract)
        .expect("something is left");
    assert!(subtracted.contains(20, 20), "outside the hole is kept");
    assert!(!subtracted.contains(50, 50), "the hole is gone");

    let intersected = outer
        .combined(&inner, SelectionCombine::Intersect)
        .expect("the overlap exists");
    assert!(intersected.contains(50, 50));
    assert!(!intersected.contains(20, 20));
    assert_eq!(intersected.bounds().width, 30, "trimmed to the overlap");

    let disjoint = outer
        .combined(&rect((110.0, 110.0), (120.0, 120.0)), SelectionCombine::Intersect);
    assert!(disjoint.is_none(), "no overlap leaves nothing selected");
}

#[test]
fn a_polygon_lasso_closes_on_its_first_corner_and_is_undoable() {
    let mut app = app(128, 128);
    app.select_tool(Tool::Selection);
    app.set_selection_mode(SelectionMode::Polygon);
    app.select_all();

    app.add_polygon_point(pos2(10.0, 10.0));
    app.add_polygon_point(pos2(90.0, 10.0));
    app.add_polygon_point(pos2(90.0, 90.0));
    app.add_polygon_point(pos2(10.0, 90.0));
    assert!(app.polygon_in_progress());
    app.add_polygon_point(pos2(10.0, 10.0));

    assert!(!app.polygon_in_progress(), "the first corner closes it");
    let selection = app.canvas.selection().expect("polygon selection");
    assert!(selection.is_polygon());
    assert!(selection.contains(50, 50));
    assert!(!selection.contains(110, 110));

    app.history.undo(&mut app.canvas);
    let restored = app.canvas.selection().expect("select all is back");
    assert!(restored.contains(120, 120), "undo returns the old selection");
}

#[test]
fn a_polygon_needs_three_corners_and_can_be_cancelled() {
    let mut app = app(128, 128);
    app.select_tool(Tool::Selection);
    app.select_all();

    app.add_polygon_point(pos2(10.0, 10.0));
    app.add_polygon_point(pos2(40.0, 10.0));
    app.finish_polygon();

    let untouched = app.canvas.selection().expect("selection is unchanged");
    assert!(untouched.contains(120, 120), "two corners select nothing");

    app.add_polygon_point(pos2(10.0, 10.0));
    app.add_polygon_point(pos2(40.0, 10.0));
    app.cancel_polygon();
    assert!(!app.polygon_in_progress());
    assert!(app.canvas.selection().is_some(), "cancelling keeps the selection");
}

#[test]
fn a_selection_follows_a_quarter_turn() {
    let mut app = app(40, 20);
    let before = Selection::from_corners(pos2(0.0, 0.0), pos2(10.0, 8.0), 40, 20)
        .expect("a usable selection");
    let original_bounds = before.bounds();
    app.canvas.set_selection(Some(before));

    app.transform_document(Transform::Rotate90Cw);
    assert_eq!((app.canvas.width(), app.canvas.height()), (20, 40));

    let turned = app.canvas.selection().expect("the selection travelled");
    // Source (x, y) maps to destination (height - 1 - y, x).
    assert!(turned.contains(19, 0), "the old top-left corner is now top-right");
    assert!(turned.contains(12, 9), "the old bottom-left corner stays selected");
    assert!(!turned.contains(0, 0), "the rest of the page is not selected");
    assert_eq!(turned.bounds().width, 8);
    assert_eq!(turned.bounds().height, 10);

    app.history.undo(&mut app.canvas);
    let back = app.canvas.selection().expect("undo brings it back");
    assert_eq!((app.canvas.width(), app.canvas.height()), (40, 20));
    assert!(back.contains(0, 0));
    assert!(!back.contains(20, 15));
    assert_eq!(back.bounds(), original_bounds);
}

#[test]
fn a_selection_survives_a_horizontal_flip() {
    let mut app = app(64, 32);
    app.canvas
        .set_selection(Some(rect((0.0, 0.0), (10.0, 10.0))));
    app.transform_document(Transform::FlipHorizontal);

    let flipped = app.canvas.selection().expect("the selection travelled");
    assert_eq!(flipped.bounds().x, 54, "it moved to the other edge");
    assert_eq!(flipped.bounds().width, 10);
    assert!(flipped.contains(63, 5));
    assert!(!flipped.contains(0, 5));
}

#[test]
fn a_feathered_selection_keeps_its_soft_edge_through_a_transform() {
    let mut canvas = canvas(32, 32);
    let soft = Selection::rectangle(pos2(4.0, 4.0), pos2(24.0, 24.0), 32, 32, 4.0)
        .expect("a feathered rectangle");
    canvas.set_selection(Some(soft));
    canvas.transform(Transform::Rotate180);

    let turned = canvas.selection().expect("the selection travelled");
    assert!(turned.is_feathered(), "the soft edge is still a mask");
    assert_eq!(turned.bounds().width, 20, "the whole box came across");
    assert_eq!(turned.bounds().height, 20);
    assert_eq!(turned.coverage(16, 16), 1.0, "the middle stays fully selected");
    let soft = (4..24)
        .map(|x| turned.coverage(x, 16))
        .any(|coverage| coverage > 0.0 && coverage < 1.0);
    assert!(soft, "the feathered edge survived the turn");
}

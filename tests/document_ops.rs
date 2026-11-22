//! Document operations that change the page itself: resize, crop, trim, print
//! setup, and the selection as an undoable step.

use inkforge::app::InkforgeApp;
use inkforge::egui::Color32;
use inkforge::engine::{Anchor, Background, Canvas, PixelRegion};

const RED: Color32 = Color32::from_rgb(0xFF, 0, 0);

/// Paint a solid block into the active layer, the way a stroke would.
fn paint_block(app: &mut InkforgeApp, x: u32, y: u32, width: u32, height: u32) {
    let id = app.canvas.active_layer_id().expect("a layer");
    let surface = app.canvas.layer_surface_mut(id).expect("surface");
    let pixels = vec![RED; (width * height) as usize];
    surface.write_region(
        PixelRegion {
            x,
            y,
            width,
            height,
        },
        &pixels,
    );
    app.canvas.invalidate_composite(None);
    app.canvas.mark_modified();
}

fn pixel(app: &InkforgeApp, x: u32, y: u32) -> Color32 {
    let id = app.canvas.active_layer_id().expect("a layer");
    app.canvas.layer_surface(id).expect("surface").pixel(x, y)
}

#[test]
fn growing_the_page_keeps_artwork_anchored_and_undoes() {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("Resize", 200, 100, Background::White).unwrap();
    paint_block(&mut app, 5, 5, 2, 2);

    // 40 px wider and 10 px taller, centred: the origin shifts by (20, 5).
    assert!(app.resize_canvas(240, 110, Anchor::Center));
    assert_eq!((app.canvas.width(), app.canvas.height()), (240, 110));
    assert_eq!(pixel(&app, 25, 10), RED, "the block moved with the anchor");
    assert_eq!(pixel(&app, 5, 5), Color32::TRANSPARENT);

    app.undo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (200, 100));
    assert_eq!(pixel(&app, 5, 5), RED, "undo restores the original page");

    app.redo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (240, 110));
    assert_eq!(pixel(&app, 25, 10), RED);
}

#[test]
fn shrinking_with_a_corner_anchor_crops_the_opposite_sides() {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("Crop", 100, 80, Background::White).unwrap();
    paint_block(&mut app, 70, 50, 2, 2);

    // Anchored bottom-right, shrinking keeps the bottom-right corner, so the
    // painted block shifts to stay near it.
    assert!(app.resize_canvas(50, 40, Anchor::BottomRight));
    assert_eq!((app.canvas.width(), app.canvas.height()), (50, 40));
    assert_eq!(pixel(&app, 20, 10), RED);

    app.undo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (100, 80));
    assert_eq!(pixel(&app, 70, 50), RED);
}

#[test]
fn cropping_to_the_selection_keeps_the_selected_area() {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("Selection crop", 60, 40, Background::White).unwrap();
    paint_block(&mut app, 12, 8, 4, 4);

    app.canvas.set_selection(Some(
        inkforge::engine::Selection::from_corners(
            inkforge::egui::pos2(10.0, 6.0),
            inkforge::egui::pos2(30.0, 26.0),
            60,
            40,
        )
        .expect("selection"),
    ));

    app.crop_to_selection();
    assert_eq!((app.canvas.width(), app.canvas.height()), (20, 20));
    assert_eq!(
        pixel(&app, 2, 2),
        RED,
        "the block keeps its place in the crop"
    );
    assert!(
        app.canvas.selection().is_none(),
        "the crop clears the selection"
    );

    app.undo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (60, 40));
    assert_eq!(pixel(&app, 12, 8), RED);

    app.redo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (20, 20));
    assert_eq!(pixel(&app, 2, 2), RED);
}

#[test]
fn cropping_without_a_selection_changes_nothing() {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("No selection", 40, 30, Background::White).unwrap();
    app.crop_to_selection();

    assert_eq!((app.canvas.width(), app.canvas.height()), (40, 30));
    assert!(!app.history.can_undo(), "nothing was recorded");
}

#[test]
fn trimming_removes_the_transparent_margin() {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("Trim", 60, 60, Background::White).unwrap();
    paint_block(&mut app, 20, 30, 5, 4);

    app.trim_transparent();
    assert_eq!(
        (app.canvas.width(), app.canvas.height()),
        (5, 4),
        "the page shrinks to the painted pixels"
    );
    assert_eq!(pixel(&app, 0, 0), RED);

    app.undo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (60, 60));
    assert_eq!(pixel(&app, 20, 30), RED);
}

#[test]
fn trimming_an_empty_document_changes_nothing() {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("Empty", 30, 30, Background::White).unwrap();
    app.trim_transparent();

    assert_eq!((app.canvas.width(), app.canvas.height()), (30, 30));
    assert!(!app.history.can_undo());
}

#[test]
fn a_full_canvas_selection_crops_to_the_same_page() {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("Whole", 25, 25, Background::White).unwrap();
    app.select_all();
    let (width, height) = (app.canvas.width(), app.canvas.height());
    app.crop_to_selection();

    assert_eq!((app.canvas.width(), app.canvas.height()), (width, height));
    assert_eq!(
        app.history.undo_depth(),
        1,
        "only the select-all step was recorded"
    );
}

#[test]
fn print_setup_changes_from_the_canvas_window_and_undoes() {
    let mut app = InkforgeApp::with_defaults();
    app.request_canvas_size();
    app.canvas_size.dpi = 300.0;
    app.canvas_size.bleed_mm = 3.0;
    app.canvas_size.safe_mm = 5.0;
    app.apply_canvas_size();

    assert!((app.canvas.dpi() - 300.0).abs() < 0.01);
    assert!(app.canvas.bleed_px() > 30.0, "3 mm at 300 dpi is ~35 px");
    assert!(app.canvas.safe_px() > 50.0);
    assert!(app.dialog.is_none(), "applying closes the window");

    app.undo();
    assert!((app.canvas.dpi() - 72.0).abs() < 0.01);
    assert_eq!(app.canvas.bleed_px(), 0.0);
    assert_eq!(app.canvas.safe_px(), 0.0);
}

#[test]
fn the_canvas_window_resizes_with_its_anchor() {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("Window", 80, 60, Background::White).unwrap();
    paint_block(&mut app, 0, 0, 1, 1);

    app.request_canvas_size();
    app.canvas_size.width = 100;
    app.canvas_size.height = 60;
    app.canvas_size.anchor = Anchor::TopLeft;
    app.apply_canvas_size();

    assert_eq!((app.canvas.width(), app.canvas.height()), (100, 60));
    assert_eq!(
        pixel(&app, 0, 0),
        RED,
        "top-left anchoring adds margin right"
    );
    app.undo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (80, 60));
}

#[test]
fn selections_are_undoable() {
    let mut app = InkforgeApp::with_defaults();
    assert!(app.canvas.selection().is_none());

    app.select_all();
    assert!(app.canvas.selection().is_some());

    app.deselect();
    assert!(app.canvas.selection().is_none());

    app.undo();
    assert!(
        app.canvas.selection().is_some(),
        "undo brings the selection back"
    );
    app.undo();
    assert!(app.canvas.selection().is_none(), "undo removes select-all");

    app.redo();
    assert!(app.canvas.selection().is_some());
}

#[test]
fn a_selection_no_op_records_nothing() {
    let mut app = InkforgeApp::with_defaults();
    app.deselect();
    assert!(
        !app.history.can_undo(),
        "clearing an empty selection is free"
    );
}

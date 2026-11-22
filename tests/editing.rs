//! Tests for Milestone 5: rectangle selection, flood fill and shapes.

use inkforge::app::{InkforgeApp, Tool};
use inkforge::egui::{pos2, Color32, Pos2};
use inkforge::engine::{
    shape::shape_path, Background, BrushSettings, Canvas, Selection, ShapeKind,
};

fn canvas(width: u32, height: u32) -> Canvas {
    Canvas::try_new("Test", width, height, Background::White).expect("valid canvas")
}

fn brush(size: f32) -> BrushSettings {
    let mut brush = BrushSettings::default();
    brush.set_size(size);
    brush.set_hardness(1.0);
    brush.set_opacity(1.0);
    brush
}

fn active(canvas: &Canvas) -> u64 {
    canvas.active_layer_id().expect("an active paint layer")
}

fn select(canvas: &mut Canvas, a: Pos2, b: Pos2) {
    let selection = Selection::from_corners(a, b, canvas.width(), canvas.height())
        .expect("a usable selection");
    canvas.set_selection(Some(selection));
}

fn small_app(width: u32, height: u32) -> InkforgeApp {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = canvas(width, height);
    app.view.request_fit();
    app.brush = brush(6.0);
    app
}

// ---- selection -----------------------------------------------------------

#[test]
fn a_selection_normalises_corners_and_clamps_to_the_canvas() {
    let selection = Selection::from_corners(pos2(30.0, 20.0), pos2(10.0, 5.0), 64, 64)
        .expect("valid selection");
    let rect = selection.rect();
    assert_eq!(rect.x, 10);
    assert_eq!(rect.y, 5);
    assert_eq!(rect.width, 20);
    assert_eq!(rect.height, 15);

    let clamped = Selection::from_corners(pos2(-40.0, -40.0), pos2(200.0, 200.0), 32, 32)
        .expect("clamped selection");
    assert_eq!(clamped.rect().x, 0);
    assert_eq!(clamped.rect().width, 32);
}

#[test]
fn a_click_sized_drag_does_not_create_a_selection() {
    assert!(Selection::from_corners(pos2(10.0, 10.0), pos2(10.5, 10.5), 64, 64).is_none());
    assert!(Selection::from_corners(pos2(10.0, 10.0), pos2(60.0, 11.0), 64, 64).is_none());
}

#[test]
fn painting_stops_at_the_selection_edge() {
    let mut canvas = canvas(64, 64);
    select(&mut canvas, pos2(8.0, 8.0), pos2(24.0, 24.0));
    let layer = active(&canvas);

    let dab = brush(12.0).dab(Color32::RED, false);
    // Centre inside the selection, radius overlapping the right edge at x = 24.
    canvas.paint_layer(layer, pos2(20.0, 16.0), pos2(20.0, 16.0), &dab);

    assert!(canvas.layer_surface(layer).unwrap().pixel(20, 16).a() > 0);
    assert!(
        canvas.layer_surface(layer).unwrap().pixel(25, 16).a() == 0,
        "nothing may be painted outside the selection"
    );
    assert_eq!(canvas.layer_surface(layer).unwrap().pixel(40, 40).a(), 0);
}

#[test]
fn selecting_everything_and_clearing_works() {
    let mut canvas = canvas(32, 32);
    canvas.select_all();
    assert_eq!(canvas.selection().expect("selection").rect().width, 32);

    canvas.clear_selection();
    assert!(canvas.selection().is_none());
}

// ---- lasso and feathering ------------------------------------------------

fn triangle() -> Vec<Pos2> {
    vec![pos2(10.0, 10.0), pos2(50.0, 10.0), pos2(30.0, 50.0)]
}

#[test]
fn a_lasso_covers_the_inside_of_its_polygon_only() {
    let selection = Selection::lasso(&triangle(), 64, 64, 0.0).expect("lasso selection");

    assert!(selection.is_polygon());
    assert!(selection.coverage(30, 20) > 0.9, "inside the triangle");
    assert_eq!(selection.coverage(55, 55), 0.0, "outside the triangle");
    assert_eq!(selection.coverage(12, 40), 0.0, "outside the triangle");
}

#[test]
fn feathering_ramps_the_coverage_at_the_edge() {
    let hard = Selection::rectangle(pos2(10.0, 10.0), pos2(50.0, 50.0), 64, 64, 0.0)
        .expect("hard rectangle");
    assert!(!hard.is_feathered());
    assert_eq!(hard.coverage(10, 30), 1.0, "a hard edge is fully covered");

    let soft = Selection::rectangle(pos2(10.0, 10.0), pos2(50.0, 50.0), 64, 64, 6.0)
        .expect("feathered rectangle");
    assert!(soft.is_feathered());
    assert_eq!(soft.coverage(30, 30), 1.0, "the centre stays fully selected");
    let edge = soft.coverage(10, 30);
    assert!(
        edge > 0.0 && edge < 0.95,
        "the edge should be partially covered, got {edge}"
    );
}

#[test]
fn painting_is_clipped_to_the_lasso_shape() {
    let mut canvas = canvas(64, 64);
    canvas.set_selection(Selection::lasso(&triangle(), 64, 64, 0.0));
    let layer = active(&canvas);

    let dab = brush(6.0).dab(Color32::RED, false);
    canvas.paint_layer(layer, pos2(30.0, 20.0), pos2(30.0, 20.0), &dab);
    canvas.paint_layer(layer, pos2(52.0, 45.0), pos2(52.0, 45.0), &dab);

    let surface = canvas.layer_surface(layer).unwrap();
    assert!(surface.pixel(30, 20).a() > 0, "inside the lasso is painted");
    assert_eq!(
        surface.pixel(52, 45).a(),
        0,
        "outside the lasso is untouched"
    );
}

#[test]
fn a_feathered_lasso_paints_a_soft_edge() {
    let mut canvas = canvas(64, 64);
    canvas.set_selection(Selection::lasso(&triangle(), 64, 64, 8.0));
    let layer = active(&canvas);

    // Radius 10 around the top edge (y = 10) of the triangle covers y ≈ 0..20.
    let dab = brush(20.0).dab(Color32::RED, false);
    canvas.paint_layer(layer, pos2(30.0, 10.0), pos2(30.0, 10.0), &dab);

    let surface = canvas.layer_surface(layer).unwrap();
    let inside = surface.pixel(30, 18).a();
    let edge = surface.pixel(30, 11).a();
    assert!(inside > 0, "inside the lasso is painted");
    assert!(
        edge > 0 && edge < inside,
        "the feathered edge should be softer than the middle (edge {edge}, inside {inside})"
    );
}

#[test]
fn fill_stops_at_the_lasso_shape() {
    let mut canvas = canvas(64, 64);
    canvas.set_selection(Selection::lasso(&triangle(), 64, 64, 0.0));
    let layer = active(&canvas);

    let mask = canvas
        .fill_mask_at(pos2(30.0, 20.0), 16)
        .expect("the lasso area is fillable");
    canvas.apply_fill(layer, &mask, Color32::BLUE).expect("applied");

    let surface = canvas.layer_surface(layer).unwrap();
    assert_eq!(surface.pixel(30, 20).b(), 255, "inside is filled");
    assert_eq!(surface.pixel(12, 45).a(), 0, "outside the lasso is untouched");
}

#[test]
fn a_lasso_drag_through_the_app_creates_a_polygon_selection() {
    let mut app = small_app(128, 128);
    app.select_tool(Tool::Selection);
    app.set_selection_mode(inkforge::engine::SelectionMode::Lasso);

    app.begin_lasso(pos2(10.0, 10.0));
    app.extend_lasso(pos2(80.0, 10.0));
    app.extend_lasso(pos2(80.0, 80.0));
    app.extend_lasso(pos2(10.0, 80.0));
    app.end_lasso();

    let selection = app.canvas.selection().expect("lasso selection");
    assert!(selection.is_polygon());
    assert!(selection.contains(40, 40));
    assert!(!selection.contains(100, 100));

    // A lasso click without a path clears instead of leaving a stray selection.
    app.select_tool(Tool::Selection);
    app.begin_lasso(pos2(20.0, 20.0));
    app.end_lasso();
    assert!(app.canvas.selection().is_none());
}

// ---- fill ----------------------------------------------------------------

#[test]
fn fill_covers_the_matching_area_and_keeps_other_colours() {
    let mut canvas = canvas(48, 48);
    let layer = active(&canvas);

    // A red blob on an otherwise empty (white-backed) canvas.
    canvas.paint_layer(layer, pos2(24.0, 24.0), pos2(24.0, 24.0), &brush(10.0).dab(Color32::RED, false));

    let mask = canvas
        .fill_mask_at(pos2(1.0, 1.0), 16)
        .expect("the background is fillable");
    assert!(
        mask.count() < 48 * 48,
        "the fill must stop at the red blob, not flood the canvas"
    );
    canvas.apply_fill(layer, &mask, Color32::BLUE).expect("fill applied");

    let surface = canvas.layer_surface(layer).unwrap();
    assert_eq!(surface.pixel(1, 1).b(), 255, "the corner is filled blue");
    assert_eq!(surface.pixel(24, 24).r(), 255, "the blob keeps its colour");
}

#[test]
fn fill_is_clipped_to_the_selection() {
    let mut canvas = canvas(48, 48);
    select(&mut canvas, pos2(4.0, 4.0), pos2(20.0, 20.0));
    let layer = active(&canvas);

    let mask = canvas.fill_mask_at(pos2(6.0, 6.0), 16).expect("fillable");
    canvas.apply_fill(layer, &mask, Color32::GREEN).expect("applied");

    let surface = canvas.layer_surface(layer).unwrap();
    assert!(surface.pixel(6, 6).a() > 0);
    assert_eq!(
        surface.pixel(30, 30).a(),
        0,
        "the fill may not escape the selection"
    );
}

#[test]
fn fill_is_one_undo_step_through_the_app() {
    let mut app = small_app(64, 64);
    let layer = app.canvas.active_layer_id().expect("layer");

    app.fill_at(pos2(2.0, 2.0));
    assert!(app.history.can_undo());
    assert!(app.canvas.layer_surface(layer).unwrap().pixel(2, 2).a() > 0);

    app.undo();
    assert_eq!(
        app.canvas.layer_surface(layer).unwrap().pixel(2, 2).a(),
        0,
        "undo removes the fill"
    );
}

#[test]
fn filling_the_selection_uses_the_current_colour() {
    let mut app = small_app(64, 64);
    app.color = Color32::from_rgb(10, 200, 30);
    app.select_all();
    let selection = app.canvas.selection().expect("selection");
    let inner = Selection::from_corners(
        pos2(8.0, 8.0),
        pos2(24.0, 24.0),
        app.canvas.width(),
        app.canvas.height(),
    )
    .expect("inner selection");
    assert!(selection.contains(20, 20));

    app.canvas.set_selection(Some(inner));
    app.fill_selection();

    let layer = app.canvas.active_layer_id().expect("layer");
    let surface = app.canvas.layer_surface(layer).unwrap();
    assert_eq!(surface.pixel(10, 10).g(), 200);
    assert_eq!(surface.pixel(40, 40).a(), 0);
    assert!(app.history.can_undo());
}

// ---- shapes --------------------------------------------------------------

#[test]
fn a_line_shape_paints_along_its_path_only() {
    let mut canvas = canvas(64, 64);
    let dab = brush(4.0).dab(Color32::BLACK, false);
    canvas
        .paint_shape(ShapeKind::Line, pos2(10.0, 10.0), pos2(50.0, 10.0), &dab)
        .expect("drew the line");

    let surface = canvas.layer_surface(active(&canvas)).unwrap();
    assert!(surface.pixel(30, 10).a() > 0, "the middle of the line is painted");
    assert_eq!(surface.pixel(30, 40).a(), 0, "nothing is painted off the line");
}

#[test]
fn a_rectangle_is_an_outline_not_a_filled_box() {
    let mut canvas = canvas(64, 64);
    let dab = brush(4.0).dab(Color32::BLACK, false);
    canvas
        .paint_shape(
            ShapeKind::Rectangle,
            pos2(10.0, 10.0),
            pos2(50.0, 40.0),
            &dab,
        )
        .expect("drew the rectangle");

    let surface = canvas.layer_surface(active(&canvas)).unwrap();
    assert!(surface.pixel(30, 10).a() > 0, "top edge painted");
    assert!(surface.pixel(10, 25).a() > 0, "left edge painted");
    assert_eq!(surface.pixel(30, 25).a(), 0, "the inside stays empty");
}

#[test]
fn an_ellipse_closes_back_on_itself() {
    let points = shape_path(ShapeKind::Ellipse, pos2(0.0, 0.0), pos2(100.0, 50.0));
    assert!(points.len() > 8);
    let first = points[0];
    let last = points[points.len() - 1];
    assert!((first - last).length() < 0.01, "the outline must close");

    let mut canvas = canvas(64, 64);
    let dab = brush(4.0).dab(Color32::BLACK, false);
    canvas
        .paint_shape(
            ShapeKind::Ellipse,
            pos2(12.0, 12.0),
            pos2(52.0, 52.0),
            &dab,
        )
        .expect("drew the ellipse");

    let surface = canvas.layer_surface(active(&canvas)).unwrap();
    assert!(surface.pixel(32, 12).a() > 0, "the top of the ellipse is painted");
    assert_eq!(surface.pixel(32, 32).a(), 0, "the centre stays empty");
}

#[test]
fn shapes_are_clipped_to_the_selection() {
    let mut canvas = canvas(64, 64);
    select(&mut canvas, pos2(0.0, 0.0), pos2(32.0, 64.0));
    let dab = brush(4.0).dab(Color32::BLACK, false);
    canvas.paint_shape(ShapeKind::Line, pos2(5.0, 32.0), pos2(60.0, 32.0), &dab);

    let surface = canvas.layer_surface(active(&canvas)).unwrap();
    assert!(surface.pixel(20, 32).a() > 0);
    assert_eq!(surface.pixel(45, 32).a(), 0, "past the selection edge");
}

// ---- soft edge on the canvas ---------------------------------------------

#[test]
fn a_feathered_selection_shows_its_soft_edge() {
    let selection = Selection::rectangle(pos2(16.0, 16.0), pos2(48.0, 48.0), 64, 64, 8.0)
        .expect("feathered rectangle");
    let spans = selection.soft_spans();

    assert!(!spans.is_empty(), "a feathered edge is tinted on the canvas");
    assert!(
        spans.iter().all(|span| span.alpha > 0 && span.alpha <= 70),
        "spans stay subtle"
    );
    assert!(
        spans.iter().all(|span| {
            let [x, y, w, h] = span.rect;
            x >= 16.0 && y >= 16.0 && x + w <= 48.0 && y + h <= 48.0
        }),
        "the tint never leaves the selection"
    );
    assert!(
        !spans.iter().any(|span| {
            let [x, y, w, h] = span.rect;
            x <= 32.0 && x + w >= 33.0 && y <= 32.0 && y + h >= 33.0
        }),
        "the fully selected middle is not tinted"
    );
}

#[test]
fn hard_selections_have_no_soft_edge_tint() {
    let rect = Selection::from_corners(pos2(16.0, 16.0), pos2(48.0, 48.0), 64, 64)
        .expect("hard rectangle");
    assert!(rect.soft_spans().is_empty());
    assert!(Selection::whole(64, 64).soft_spans().is_empty());
}

#[test]
fn growing_or_shrinking_a_feathered_selection_keeps_its_soft_edge() {
    let selection = Selection::rectangle(pos2(16.0, 16.0), pos2(48.0, 48.0), 64, 64, 8.0)
        .expect("feathered rectangle");

    let grown = selection.resized(4, 64, 64).expect("grown");
    assert!(!grown.soft_spans().is_empty(), "the band moves out with the edge");
    let shrunk = selection.resized(-4, 64, 64).expect("shrunk");
    assert!(!shrunk.soft_spans().is_empty(), "the band moves in with the edge");
}

// ---- quick mask -----------------------------------------------------------

#[test]
fn quick_mask_covers_the_whole_selection() {
    let selection = Selection::from_corners(pos2(16.0, 16.0), pos2(48.0, 48.0), 64, 64)
        .expect("rectangle");
    let spans = selection.preview_spans();

    assert_eq!(spans.len(), 1, "a hard selection is one solid wash");
    assert_eq!(spans[0].rect, [16.0, 16.0, 32.0, 32.0]);
    assert_eq!(spans[0].alpha, 88);
}

#[test]
fn quick_mask_fades_with_a_feathered_edge() {
    let selection = Selection::rectangle(pos2(16.0, 16.0), pos2(48.0, 48.0), 64, 64, 8.0)
        .expect("feathered rectangle");
    let spans = selection.preview_spans();

    assert!(spans.len() > 1, "a feathered mask is drawn in steps");
    assert!(spans.iter().all(|span| span.alpha <= 88));

    let centre = spans
        .iter()
        .find(|span| {
            let [x, y, w, h] = span.rect;
            x <= 32.0 && x + w >= 33.0 && y <= 32.0 && y + h >= 33.0
        })
        .expect("a span covers the middle");
    assert_eq!(centre.alpha, 88, "the middle is fully selected");
    assert!(
        spans.iter().any(|span| span.alpha > 0 && span.alpha < 88),
        "the soft edge is weaker than the middle"
    );
}

#[test]
fn quick_mask_spans_are_built_once() {
    let selection = Selection::rectangle(pos2(16.0, 16.0), pos2(48.0, 48.0), 64, 64, 8.0)
        .expect("feathered rectangle");
    let first = selection.preview_spans().as_ptr();
    let second = selection.preview_spans().as_ptr();
    assert_eq!(first, second, "the wash is cached, not rebuilt every frame");
}

#[test]
fn the_quick_mask_toggle_reports_its_state() {
    let mut app = small_app(64, 64);

    app.toggle_quick_mask();
    assert!(app.quick_mask);
    assert!(app.status().contains("nothing is selected"));

    app.canvas.set_selection(Selection::from_corners(
        pos2(16.0, 16.0),
        pos2(48.0, 48.0),
        64,
        64,
    ));
    app.set_quick_mask(false);
    assert!(!app.quick_mask);
    assert!(app.status().contains("Quick mask off"));

    app.toggle_quick_mask();
    assert!(app.quick_mask);
    assert!(app.status().contains("the brush adds"));
}

// ---- grow and shrink ------------------------------------------------------

#[test]
fn growing_and_shrinking_move_the_selection_edge() {
    let selection = Selection::from_corners(pos2(16.0, 16.0), pos2(32.0, 32.0), 64, 64)
        .expect("rectangle");

    let grown = selection.resized(4, 64, 64).expect("grown selection");
    let rect = grown.rect();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (12, 12, 24, 24),
        "growing by 4 adds 4 px on every side"
    );
    assert_eq!(grown.coverage(12, 20), 1.0, "the new edge is fully selected");

    let shrunk = selection.resized(-4, 64, 64).expect("shrunk selection");
    let rect = shrunk.rect();
    assert_eq!((rect.x, rect.width), (20, 8), "shrinking pulls every side in");
    assert_eq!(shrunk.coverage(19, 20), 0.0);

    let back = grown.resized(-4, 64, 64).expect("grown then shrunk");
    let rect = back.rect();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (16, 16, 16, 16),
        "the two steps cancel each other out"
    );
}

#[test]
fn shrinking_a_selection_to_nothing_removes_it() {
    let selection = Selection::from_corners(pos2(16.0, 16.0), pos2(19.0, 19.0), 64, 64)
        .expect("small rectangle");
    assert!(
        selection.resized(-4, 64, 64).is_none(),
        "a 3 × 3 selection cannot survive a 4 px shrink"
    );
}

#[test]
fn growing_stops_at_the_canvas_edge() {
    let selection = Selection::from_corners(pos2(0.0, 0.0), pos2(8.0, 8.0), 32, 32)
        .expect("corner rectangle");
    let grown = selection.resized(8, 32, 32).expect("grown selection");
    let rect = grown.rect();
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (0, 0, 16, 16));
}

// ---- invert ---------------------------------------------------------------

#[test]
fn inverting_a_rectangle_selects_everything_around_it() {
    let selection = Selection::from_corners(pos2(16.0, 16.0), pos2(32.0, 32.0), 64, 64)
        .expect("rectangle");
    let inverted = selection.inverted(64, 64).expect("inverted selection");

    let rect = inverted.rect();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (0, 0, 64, 64),
        "the complement covers the canvas"
    );
    assert_eq!(inverted.coverage(4, 4), 1.0);
    assert_eq!(inverted.coverage(20, 20), 0.0, "the old selection is a hole");
}

#[test]
fn inverting_a_feathered_selection_keeps_the_soft_edge() {
    let selection = Selection::lasso(&triangle(), 64, 64, 8.0).expect("lasso");
    let inverted = selection.inverted(64, 64).expect("inverted selection");

    for (x, y) in [(2, 2), (16, 32), (32, 32), (40, 50), (60, 60)] {
        let combined = selection.coverage(x, y) + inverted.coverage(x, y);
        assert!(
            (combined - 1.0).abs() < 0.01,
            "coverage at {x},{y} should complement: {combined}"
        );
    }
}

#[test]
fn inverting_twice_returns_the_original_selection() {
    let selection = Selection::from_corners(pos2(16.0, 16.0), pos2(32.0, 32.0), 64, 64)
        .expect("rectangle");
    let twice = selection
        .inverted(64, 64)
        .expect("inverted")
        .inverted(64, 64)
        .expect("inverted again");

    assert_eq!(twice.coverage(20, 20), 1.0);
    assert_eq!(twice.coverage(4, 4), 0.0);
}

#[test]
fn inverting_a_full_canvas_selection_leaves_nothing() {
    let selection = Selection::from_corners(pos2(0.0, 0.0), pos2(64.0, 64.0), 64, 64)
        .expect("full canvas");
    assert!(selection.inverted(64, 64).is_none());
}

// ---- magic wand ----------------------------------------------------------

/// Paint a solid rectangle on the active layer, so the wand has colour to find.
fn fill_rect(canvas: &mut Canvas, a: Pos2, b: Pos2, colour: Color32) {
    select(canvas, a, b);
    let layer = active(canvas);
    canvas.fill_selection(layer, colour);
    canvas.clear_selection();
}

#[test]
fn the_magic_wand_selects_a_connected_region_only() {
    let mut canvas = canvas(64, 64);
    fill_rect(&mut canvas, pos2(8.0, 8.0), pos2(24.0, 24.0), Color32::RED);
    fill_rect(&mut canvas, pos2(40.0, 40.0), pos2(56.0, 56.0), Color32::RED);

    let selection = canvas
        .wand_selection_at(pos2(12.0, 12.0), 0, 0.0)
        .expect("the wand should select the red block");

    let rect = selection.rect();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (8, 8, 16, 16),
        "the selection follows the clicked block"
    );
    assert_eq!(selection.coverage(12, 12), 1.0);
    assert_eq!(selection.coverage(30, 12), 0.0, "the gap is a different colour");
    assert_eq!(
        selection.coverage(45, 45),
        0.0,
        "a matching but disconnected block must stay out"
    );
    assert!(selection.label().contains("magic wand"));
    assert!(
        selection
            .outline_edges()
            .is_some_and(|edges| !edges.is_empty()),
        "the region should carry a boundary outline"
    );
}

#[test]
fn the_magic_wand_tolerance_widens_the_region() {
    let mut canvas = canvas(64, 64);
    fill_rect(
        &mut canvas,
        pos2(0.0, 0.0),
        pos2(32.0, 64.0),
        Color32::from_rgb(100, 100, 100),
    );
    fill_rect(
        &mut canvas,
        pos2(32.0, 0.0),
        pos2(64.0, 64.0),
        Color32::from_rgb(112, 112, 112),
    );

    let tight = canvas
        .wand_selection_at(pos2(10.0, 10.0), 0, 0.0)
        .expect("tight selection");
    assert_eq!(tight.rect().width, 32, "0 tolerance stops at the shade change");
    assert_eq!(tight.coverage(40, 10), 0.0);

    let loose = canvas
        .wand_selection_at(pos2(10.0, 10.0), 24, 0.0)
        .expect("loose selection");
    assert_eq!(
        loose.rect().width,
        64,
        "a tolerance of 24 swallows the neighbouring shade"
    );
    assert_eq!(loose.coverage(40, 10), 1.0);
}

#[test]
fn a_wand_selection_clips_painting_to_the_region_shape() {
    let mut canvas = canvas(64, 64);
    // Two arms of one colour form an L: its bounding box has an empty corner.
    fill_rect(&mut canvas, pos2(8.0, 8.0), pos2(32.0, 16.0), Color32::RED);
    fill_rect(&mut canvas, pos2(8.0, 8.0), pos2(16.0, 32.0), Color32::RED);

    let selection = canvas
        .wand_selection_at(pos2(10.0, 10.0), 0, 0.0)
        .expect("wand selection");
    let rect = selection.rect();
    assert_eq!((rect.width, rect.height), (24, 24));
    assert_eq!(selection.coverage(10, 10), 1.0);
    assert_eq!(
        selection.coverage(28, 28),
        0.0,
        "the empty corner is inside the bounds, not the region"
    );

    canvas.set_selection(Some(selection));
    let layer = active(&canvas);
    let dab = brush(40.0).dab(Color32::BLACK, false);
    canvas.paint_layer(layer, pos2(20.0, 20.0), pos2(20.0, 20.0), &dab);

    let surface = canvas.layer_surface(layer).unwrap();
    assert!(
        surface.pixel(28, 28).a() == 0,
        "painting must follow the region, not its bounding box"
    );
    assert!(surface.pixel(12, 12).a() > 0);
}

#[test]
fn a_feathered_wand_selection_softens_its_edge() {
    let mut canvas = canvas(64, 64);
    fill_rect(&mut canvas, pos2(8.0, 8.0), pos2(40.0, 40.0), Color32::RED);

    let selection = canvas
        .wand_selection_at(pos2(20.0, 20.0), 0, 4.0)
        .expect("wand selection");
    assert!(selection.is_feathered());
    assert_eq!(selection.coverage(20, 20), 1.0, "the centre stays solid");
    let edge = selection.coverage(8, 20);
    assert!(
        edge > 0.0 && edge < 0.95,
        "the boundary should fade, got {edge}"
    );
}

// ---- app wiring ----------------------------------------------------------

#[test]
fn dragging_with_the_selection_tool_creates_a_selection() {
    let mut app = small_app(128, 128);
    app.select_tool(Tool::Selection);

    app.begin_selection_drag(pos2(10.0, 10.0));
    app.update_selection_drag(pos2(60.0, 50.0));
    app.end_selection_drag();

    let rect = app.canvas.selection().expect("selection").rect();
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (10, 10, 50, 40));
    assert!(app.status().contains("Selection"));

    // A click (no drag) drops it again.
    app.begin_selection_drag(pos2(20.0, 20.0));
    app.end_selection_drag();
    assert!(app.canvas.selection().is_none());
}

#[test]
fn the_wand_mode_selects_through_the_app() {
    let mut app = small_app(64, 64);
    app.select_tool(Tool::Selection);
    app.set_selection_mode(inkforge::engine::SelectionMode::MagicWand);
    app.set_wand_tolerance(10);

    app.wand_at(pos2(32.0, 32.0));

    let rect = app.canvas.selection().expect("selection").rect();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (0, 0, 64, 64),
        "an untouched canvas is one connected colour"
    );
    assert!(app.status().contains("Selection"));
}

#[test]
fn dragging_the_grow_slider_is_relative_to_the_start_of_the_drag() {
    let mut app = small_app(64, 64);
    app.select_all();

    app.begin_grow();
    app.preview_grow(-4.0);
    let rect = app.canvas.selection().expect("selection").rect();
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (4, 4, 56, 56));

    // Dragging further shrinks from the captured selection, not cumulatively.
    app.preview_grow(-12.0);
    let rect = app.canvas.selection().expect("selection").rect();
    assert_eq!((rect.x, rect.width), (12, 40));

    // Back to the middle restores the original size exactly.
    app.preview_grow(0.0);
    let rect = app.canvas.selection().expect("selection").rect();
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (0, 0, 64, 64));

    app.preview_grow(6.0);
    app.finish_grow();
    assert_eq!(app.grow_offset, 0.0, "the slider springs back to the middle");
    assert!(app.status().contains("Grew the selection by 6 px"));
    let rect = app.canvas.selection().expect("selection").rect();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (0, 0, 64, 64),
        "growing past the canvas is clipped to it"
    );
}

#[test]
fn the_grow_slider_reports_an_empty_selection() {
    let mut app = small_app(64, 64);
    app.begin_grow();
    app.preview_grow(4.0);
    app.finish_grow();
    assert!(app.canvas.selection().is_none());
    assert!(app.status().contains("Nothing is selected"));

    app.canvas.set_selection(Selection::from_corners(
        pos2(16.0, 16.0),
        pos2(19.0, 19.0),
        64,
        64,
    ));
    app.begin_grow();
    app.preview_grow(-4.0);
    assert!(app.canvas.selection().is_none());
    app.finish_grow();
    assert!(app.status().contains("shrank away"));
}

#[test]
fn the_invert_button_complements_the_selection() {
    let mut app = small_app(64, 64);

    app.select_all();
    app.invert_selection();
    assert!(app.canvas.selection().is_none());
    assert!(app.status().contains("selection cleared"));

    app.canvas.set_selection(Selection::from_corners(
        pos2(16.0, 16.0),
        pos2(32.0, 32.0),
        64,
        64,
    ));
    app.invert_selection();
    let selection = app.canvas.selection().expect("selection");
    assert_eq!(selection.coverage(4, 4), 1.0);
    assert_eq!(selection.coverage(20, 20), 0.0);
    assert!(app.status().contains("Inverted the selection"));

    app.deselect();
    app.invert_selection();
    assert!(app.status().contains("Nothing is selected"));
}

#[test]
fn committing_a_shape_is_one_undo_step() {
    let mut app = small_app(128, 128);
    app.select_tool(Tool::Shape);
    app.set_shape(ShapeKind::Rectangle);
    let layer = app.canvas.active_layer_id().expect("layer");
    let depth = app.history.undo_depth();

    app.begin_shape_drag(pos2(20.0, 20.0));
    app.update_shape_drag(pos2(80.0, 60.0));
    app.commit_shape();

    assert_eq!(app.history.undo_depth(), depth + 1);
    assert!(
        app.canvas.layer_surface(layer).unwrap().pixel(50, 20).a() > 0,
        "the rectangle's top edge is painted"
    );

    app.undo();
    assert_eq!(
        app.canvas.layer_surface(layer).unwrap().pixel(50, 20).a(),
        0
    );
}

#[test]
fn painting_is_refused_when_no_layer_is_selected() {
    let mut app = small_app(64, 64);
    app.add_group();
    let depth = app.history.undo_depth();
    app.begin_stroke(pos2(10.0, 10.0));
    app.end_stroke();

    assert_eq!(app.history.undo_depth(), depth);
    assert!(app.stroke.is_none());
}

// ---- editable quick mask -------------------------------------------------

#[test]
fn quick_mask_painting_adds_to_the_selection() {
    let mut app = small_app(64, 64);
    app.canvas.set_selection(Selection::from_corners(
        pos2(10.0, 10.0),
        pos2(40.0, 40.0),
        64,
        64,
    ));
    app.set_quick_mask(true);

    app.dab_at(pos2(32.0, 32.0));
    app.dab_at(pos2(52.0, 52.0));

    let selection = app.canvas.selection().expect("selection");
    assert!(
        selection.is_feathered(),
        "painting has to be able to punch a hole, so the mask becomes dense"
    );
    assert_eq!(selection.coverage(32, 32), 1.0);
    assert!(
        selection.coverage(52, 52) > 0.9,
        "painting outside the old bounds grows the selection"
    );
    assert_eq!(selection.coverage(5, 5), 0.0);
    assert!(
        selection.rect().width > 30,
        "the bounds grew to hold the new paint"
    );
}

#[test]
fn quick_mask_erasing_removes_from_the_selection() {
    let mut app = small_app(64, 64);
    app.select_all();
    app.set_quick_mask(true);
    app.select_tool(Tool::Eraser);
    app.brush = brush(20.0);

    app.dab_at(pos2(32.0, 32.0));

    let selection = app.canvas.selection().expect("the rest is still selected");
    assert!(
        selection.coverage(32, 32) < 0.05,
        "the eraser takes coverage away"
    );
    assert_eq!(selection.coverage(4, 4), 1.0, "far from the eraser");
}

#[test]
fn quick_mask_painting_leaves_the_layer_alone() {
    let mut app = small_app(64, 64);
    app.select_all();
    app.set_quick_mask(true);
    let layer = app.canvas.active_layer_id().expect("layer");
    let depth = app.history.undo_depth();

    app.begin_stroke(pos2(20.0, 20.0));
    app.extend_stroke(pos2(40.0, 40.0));
    app.end_stroke();

    assert_eq!(
        app.canvas.layer_surface(layer).unwrap().pixel(20, 20).a(),
        0,
        "a Quick Mask stroke must not touch pixels"
    );
    assert_eq!(
        app.history.undo_depth(),
        depth + 1,
        "one step, and it is the selection — not a pixel edit"
    );
    let selection = app.canvas.selection().expect("selection");
    assert!(selection.coverage(20, 20) > 0.0);
    assert!(!app.is_painting_mask());

    app.undo();
    let restored = app.canvas.selection().expect("select-all is back");
    assert_eq!(
        restored.coverage(20, 20),
        1.0,
        "undo restores the selection from before the stroke"
    );
}

#[test]
fn quick_mask_painting_with_nothing_selected_creates_a_selection() {
    let mut app = small_app(64, 64);
    app.set_quick_mask(true);
    assert!(app.canvas.selection().is_none());

    app.dab_at(pos2(30.0, 30.0));

    let selection = app.canvas.selection().expect("painting made a selection");
    assert!(selection.coverage(30, 30) > 0.9);
    assert_eq!(selection.coverage(60, 60), 0.0);
}

#[test]
fn erasing_the_last_of_the_quick_mask_clears_the_selection() {
    let mut app = small_app(64, 64);
    app.canvas.set_selection(Selection::from_corners(
        pos2(8.0, 8.0),
        pos2(20.0, 20.0),
        64,
        64,
    ));
    app.set_quick_mask(true);
    app.select_tool(Tool::Eraser);
    app.brush = brush(64.0);

    app.dab_at(pos2(14.0, 14.0));

    assert!(app.canvas.selection().is_none());
    assert!(app.status().contains("selection cleared"));
}

#[test]
fn a_painted_mask_gets_its_outline_back_when_the_stroke_ends() {
    let mut app = small_app(64, 64);
    app.select_all();
    app.set_quick_mask(true);
    app.select_tool(Tool::Eraser);
    app.brush = brush(16.0);

    app.begin_stroke(pos2(32.0, 32.0));
    assert!(app.is_painting_mask(), "the drag is editing the selection");
    app.end_stroke();

    let selection = app.canvas.selection().expect("a ring is left");
    assert!(
        selection.outline_edges().is_some(),
        "the hole is outlined again once the stroke ends"
    );
}

#[test]
fn with_the_quick_mask_off_the_brush_paints_the_layer() {
    let mut app = small_app(64, 64);
    let layer = app.canvas.active_layer_id().expect("layer");
    app.set_quick_mask(false);

    app.dab_at(pos2(30.0, 30.0));

    assert!(app.canvas.selection().is_none());
    assert!(app.canvas.layer_surface(layer).unwrap().pixel(30, 30).a() > 0);
}

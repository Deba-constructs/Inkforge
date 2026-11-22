//! Central canvas view: page, transparency checkerboard, painting, zoom, pan
//! and canvas rotation.
//!
//! Pointer positions are converted to canvas pixels through [`Viewport`], so
//! painting, zooming, panning and turning the sheet all share one transform and
//! a stroke lands in the right place at any zoom level and any angle.

use inkforge::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Painter, PointerButton, Pos2, Rect,
    Sense, Shape, Stroke, StrokeKind, Vec2,
};

use super::theme;
use crate::app::{InkforgeApp, Tool};
use crate::engine::{Selection, SoftSpan, Viewport};
use crate::io::Unit;

/// Quick Mask tint: a translucent red wash over the selected area.
const QUICK_MASK_COLOUR: Color32 = Color32::from_rgb(0xD2, 0x3B, 0x3B);

/// Size of one transparency-checker square, in screen pixels.
const CHECKER_CELL: f32 = 16.0;
const CHECKER_LIGHT: Color32 = Color32::from_rgb(0xE8, 0xE8, 0xE8);
const CHECKER_DARK: Color32 = Color32::from_rgb(0xC6, 0xC6, 0xC6);

/// Radians the view turns per screen pixel of a Shift-drag with the Hand tool.
/// 0.006 rad is about a third of a degree per pixel: a 200 px drag is 70°.
const ROTATE_PER_PIXEL: f32 = 0.006;
/// Width of the ruler strips, in screen pixels.
const RULER: f32 = 18.0;
/// Smallest distance between labelled ruler ticks, in screen pixels.
const RULER_LABEL_GAP: f32 = 62.0;

pub fn show(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::CANVAS_SURROUND))
        .show(ui, |ui| {
            let available = ui.available_size();
            if available.x < 8.0 || available.y < 8.0 {
                return;
            }

            // Allocate the whole area, then take the ruler strips out of it so
            // the artwork is never hidden under the rulers.
            let (outer, _) = ui.allocate_exact_size(available, Sense::hover());
            let rulers = app.show_rulers && !app.view.rotated();
            let view = if rulers {
                Rect::from_min_max(outer.min + Vec2::splat(RULER), outer.max)
            } else {
                outer
            };
            let response = ui.interact(
                view,
                ui.auto_id_with("canvas_surface"),
                Sense::click_and_drag(),
            );
            let painter = ui.painter_at(view);
            let doc = app.canvas.size_vec();

            if app.view.needs_fit() {
                app.view.fit(view, doc);
            }

            // `page` is the upright page rectangle: its width and height are
            // the page size at the current zoom, and the drawing code turns its
            // corners through `page_view` when the view is rotated.
            let page = app.view.page_rect(view, doc);
            let page_view = PageView::new(view, &app.view, doc);

            // Soft drop shadow so the page reads as a sheet on a desk.
            let shadow = page.translate(Vec2::new(0.0, 8.0)).expand(7.0);
            if page_view.turned() {
                quad(&painter, page_view.corners(shadow), Color32::from_black_alpha(60));
            } else {
                painter.rect_filled(shadow, CornerRadius::same(6), Color32::from_black_alpha(60));
            }

            if let Some(colour) = app.canvas.background().fill() {
                if page_view.turned() {
                    quad(&painter, page_view.corners(page), colour);
                } else {
                    painter.rect_filled(page, CornerRadius::ZERO, colour);
                }
            } else {
                checkerboard(&painter, &page_view, page, view);
            }

            // Painted pixels on top of the background. The composite is drawn
            // from a tile pyramid, so only the tiles that are actually on
            // screen are uploaded and drawn — panning a very large document
            // costs the newly exposed tiles, not the whole canvas.
            app.tiles.draw_view(
                ui.ctx(),
                &painter,
                page,
                app.canvas.composite_mut(),
                app.view.zoom(),
                app.view.rotation(),
            );

            if page_view.turned() {
                let corners = page_view.corners(page);
                for index in 0..4 {
                    painter.line_segment(
                        [corners[index], corners[(index + 1) % 4]],
                        Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 40)),
                    );
                }
            } else {
                painter.rect_stroke(
                    page,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 40)),
                    StrokeKind::Outside,
                );
            }

            draw_guides(app, &painter, &page_view, page);
            empty_state(app, &painter, page, view);

            if !page.intersects(view) {
                painter.text(
                    view.center(),
                    Align2::CENTER_CENTER,
                    "Canvas is off-screen — press 0 to fit",
                    FontId::proportional(13.0),
                    theme::TEXT_MUTED,
                );
            }

            draw_overlays(app, &painter, &page_view);
            if rulers {
                draw_rulers(app, ui, outer, view, page);
            }
            handle_input(app, ui, &response, view);
            brush_cursor(app, ui, &response);
        });
}

/// Grid, trim, bleed and safe-area guides, plus print marks.
///
/// Everything is painted over the page and clipped to the canvas area, so the
/// guides never affect painting: they are measurement aids, not content.
fn draw_guides(app: &InkforgeApp, painter: &Painter, page_view: &PageView, page: Rect) {
    if page.width() <= 0.0 || page.height() <= 0.0 {
        return;
    }

    if app.show_grid {
        let spacing = app.grid_spacing.max(1.0) as f32 * page_view.zoom;
        if spacing >= 4.0 {
            let stroke = Stroke::new(1.0, Color32::from_white_alpha(26));
            let mut x = 0.0;
            while x <= page_view.doc.x + 0.5 {
                let from = page_view.point(Pos2::new(x, 0.0));
                let to = page_view.point(Pos2::new(x, page_view.doc.y));
                if page.contains(from) || page.contains(to) {
                    painter.line_segment([from, to], stroke);
                }
                x += app.grid_spacing.max(1.0) as f32;
            }
            let mut y = 0.0;
            while y <= page_view.doc.y + 0.5 {
                let from = page_view.point(Pos2::new(0.0, y));
                let to = page_view.point(Pos2::new(page_view.doc.x, y));
                if page.contains(from) || page.contains(to) {
                    painter.line_segment([from, to], stroke);
                }
                y += app.grid_spacing.max(1.0) as f32;
            }
        }
    }

    if !app.show_guides {
        return;
    }

    // The canvas includes its bleed, so the trim line sits inwards by the bleed
    // and the safe area sits further in still.
    let bleed = app.canvas.bleed_px().round();
    let safe = app.canvas.safe_px();
    let trim = Rect::from_min_max(
        Pos2::new(bleed, bleed),
        Pos2::new(page_view.doc.x - bleed, page_view.doc.y - bleed),
    );
    if trim.width() > 2.0 && trim.height() > 2.0 && bleed > 0.0 {
        outline_quad(painter, page_view.corners(trim), theme::ACCENT, 1.2, 6.0, 5.0);
    }

    if safe > 0.0 {
        let safe_rect = Rect::from_min_max(
            trim.min + Vec2::splat(safe),
            trim.max - Vec2::splat(safe),
        );
        if safe_rect.width() > 2.0 && safe_rect.height() > 2.0 {
            outline_quad(
                painter,
                page_view.corners(safe_rect),
                Color32::from_rgb(0x6F, 0xB5, 0xE8),
                1.0,
                3.0,
                4.0,
            );
        }
    }

    // Trim marks: short ticks at the trim corners, reaching into the bleed so a
    // printer can see where the cut falls.
    if app.show_print_marks && bleed > 0.0 {
        let marks = (bleed * page_view.zoom).clamp(3.0, 28.0);
        let corners = page_view.corners(trim);
        let stroke = Stroke::new(1.0, theme::ACCENT);
        for corner in corners {
            // Each mark straddles its corner along the page axes, which stay
            // axis-aligned on screen even when the view is turned.
            let along_x = Vec2::new(marks, 0.0);
            let along_y = Vec2::new(0.0, marks);
            painter.line_segment([corner - along_x, corner + along_x], stroke);
            painter.line_segment([corner - along_y, corner + along_y], stroke);
        }
    }
}

/// Dashed outline of a screen-space quad.
fn outline_quad(
    painter: &Painter,
    corners: [Pos2; 4],
    colour: Color32,
    width: f32,
    dash: f32,
    gap: f32,
) {
    let stroke = Stroke::new(width, colour);
    for index in 0..4 {
        let from = corners[index];
        let to = corners[(index + 1) % 4];
        let delta = to - from;
        let length = delta.length();
        if length <= 1.0 {
            continue;
        }
        let direction = delta / length;
        let mut offset = 0.0;
        while offset < length {
            let end = (offset + dash).min(length);
            painter.line_segment([from + direction * offset, from + direction * end], stroke);
            offset += dash + gap;
        }
    }
}

/// Top and left rulers, measured in the current unit.
///
/// They read the same `Viewport` mapping as the pointer, so a tick always sits
/// over the pixel it names. A turned canvas has no upright axis to measure
/// along, so the rulers are simply left out while it is turned.
fn draw_rulers(app: &InkforgeApp, ui: &egui::Ui, outer: Rect, view: Rect, page: Rect) {
    let unit = app.units;
    let dpi = app.canvas.dpi();
    let zoom = app.view.zoom();
    let pixels_per_unit = match unit {
        Unit::Pixels => 1.0,
        Unit::Millimeters => dpi / 25.4,
        Unit::Centimeters => dpi / 2.54,
        Unit::Inches => dpi,
    };
    let per_unit_on_screen = pixels_per_unit * zoom;
    if per_unit_on_screen <= 0.0 {
        return;
    }

    // A tick every 1, 2, 5, 10, 20, 25, 50, … units, whichever first leaves
    // enough room for its label.
    let mut step = 1.0f32;
    for candidate in [
        1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0,
        5000.0,
    ] {
        step = candidate;
        if candidate * per_unit_on_screen >= RULER_LABEL_GAP {
            break;
        }
    }

    let painter = ui.painter_at(outer);
    let top = Rect::from_min_max(Pos2::new(view.left(), outer.top()), Pos2::new(view.right(), view.top()));
    let left = Rect::from_min_max(Pos2::new(outer.left(), view.top()), Pos2::new(view.left(), view.bottom()));
    let corner = Rect::from_min_max(outer.min, Pos2::new(view.left(), view.top()));

    for strip in [top, left, corner] {
        painter.rect_filled(strip, CornerRadius::ZERO, theme::BAR);
    }
    painter.hline(
        top.x_range(),
        top.max.y - 0.5,
        Stroke::new(1.0, theme::LINE),
    );
    painter.vline(
        left.max.x - 0.5,
        left.y_range(),
        Stroke::new(1.0, theme::LINE),
    );

    // The page's span is marked on both rulers so the sheet is easy to find.
    let page_colour = theme::with_alpha(theme::ACCENT, 160);
    painter.hline(
        (page.left().max(view.left()))..=(page.right().min(view.right())),
        top.max.y - 1.0,
        Stroke::new(2.0, page_colour),
    );
    painter.vline(
        left.max.x - 1.0,
        (page.top().max(view.top()))..=(page.bottom().min(view.bottom())),
        Stroke::new(2.0, page_colour),
    );

    let top_painter = painter.with_clip_rect(top);
    let left_painter = painter.with_clip_rect(left);
    let tick = Stroke::new(1.0, theme::TEXT_MUTED);

    // Horizontal ruler.
    let start_value = ((view.left() - page.left()) / zoom).max(0.0) / pixels_per_unit;
    let end_value = ((view.right() - page.left()) / zoom).min(app.canvas.width() as f32)
        / pixels_per_unit;
    let first = (start_value / step).floor() * step;
    let mut value = first;
    let mut guard = 0;
    while value <= end_value && guard < 400 {
        let x = page.left() + value * per_unit_on_screen;
        top_painter.line_segment(
            [Pos2::new(x, top.max.y - 6.0), Pos2::new(x, top.max.y)],
            tick,
        );
        top_painter.text(
            Pos2::new(x + 3.0, top.top() + 1.0),
            Align2::LEFT_TOP,
            tick_label(value, step),
            FontId::proportional(9.0),
            theme::TEXT_MUTED,
        );
        value += step;
        guard += 1;
    }

    // Vertical ruler.
    let start_value = ((view.top() - page.top()) / zoom).max(0.0) / pixels_per_unit;
    let end_value = ((view.bottom() - page.top()) / zoom).min(app.canvas.height() as f32)
        / pixels_per_unit;
    let first = (start_value / step).floor() * step;
    let mut value = first;
    let mut guard = 0;
    while value <= end_value && guard < 400 {
        let y = page.top() + value * per_unit_on_screen;
        left_painter.line_segment(
            [Pos2::new(left.max.x - 6.0, y), Pos2::new(left.max.x, y)],
            tick,
        );
        left_painter.text(
            Pos2::new(left.left() + 1.0, y + 1.0),
            Align2::LEFT_TOP,
            tick_label(value, step),
            FontId::proportional(9.0),
            theme::TEXT_MUTED,
        );
        value += step;
        guard += 1;
    }
}

/// A tick label: whole numbers unless the step is fractional.
fn tick_label(value: f32, step: f32) -> String {
    let label = if value.fract().abs() < 1e-4 {
        format!("{value:.0}")
    } else if step < 0.1 {
        format!("{value:.2}")
    } else {
        format!("{value:.1}")
    };
    label
}

/// Canvas-to-screen mapping for one frame, including the view rotation.
///
/// Every piece of page geometry — tiles, checkerboard, outlines, previews — is
/// turned through this, so nothing can drift out of step with the pointer
/// mapping in [`Viewport`].
#[derive(Clone, Copy)]
struct PageView {
    /// Screen position of the page centre.
    centre: Pos2,
    doc: Vec2,
    zoom: f32,
    rotation: f32,
}

impl PageView {
    fn new(view: Rect, viewport: &Viewport, doc: Vec2) -> Self {
        Self {
            centre: view.center() + viewport.pan(),
            doc,
            zoom: viewport.zoom(),
            rotation: viewport.rotation(),
        }
    }

    fn turned(&self) -> bool {
        self.rotation.abs() > 1.0e-4
    }

    /// Canvas pixel coordinate → screen point.
    fn point(&self, canvas: Pos2) -> Pos2 {
        let offset = (canvas.to_vec2() - self.doc * 0.5) * self.zoom;
        self.centre + rotate(offset, self.rotation)
    }

    /// Screen point → upright page-space coordinate, the space the checkerboard
    /// and the tile rectangle are laid out in.
    fn unpoint(&self, screen: Pos2) -> Pos2 {
        let offset = rotate(screen - self.centre, -self.rotation) / self.zoom;
        Pos2::new(
            offset.x + self.doc.x * 0.5,
            offset.y + self.doc.y * 0.5,
        )
    }

    /// The four screen corners of an axis-aligned canvas rectangle, clockwise
    /// from the top-left.
    fn corners(&self, rect: Rect) -> [Pos2; 4] {
        [
            self.point(rect.min),
            self.point(Pos2::new(rect.max.x, rect.min.y)),
            self.point(rect.max),
            self.point(Pos2::new(rect.min.x, rect.max.y)),
        ]
    }
}

fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}

/// Fill a screen-space quad. Used for every page rectangle while the view is
/// rotated, where `rect_filled` would draw it upright.
fn quad(painter: &Painter, corners: [Pos2; 4], colour: Color32) {
    painter.add(Shape::convex_polygon(
        corners.to_vec(),
        colour,
        Stroke::NONE,
    ));
}

/// Start hint drawn on an untouched page: what to do first, in one card.
///
/// It is painted, not a widget, so it never swallows a click or a stroke; the
/// first painted pixel removes it (see [`InkforgeApp::document_is_empty`]).
fn empty_state(app: &mut InkforgeApp, painter: &Painter, page: Rect, view: Rect) {
    if !app.document_is_empty() {
        return;
    }

    let area = page.intersect(view);
    if area.width() < 180.0 || area.height() < 90.0 {
        return;
    }

    let width = 360.0_f32.min(area.width() - 24.0);
    let height = 124.0;
    let card = Rect::from_center_size(area.center(), Vec2::new(width, height));

    painter.rect_filled(
        card.translate(Vec2::new(0.0, 3.0)),
        CornerRadius::same(10),
        Color32::from_black_alpha(70),
    );
    painter.rect_filled(
        card,
        CornerRadius::same(10),
        theme::with_alpha(theme::BAR, 235),
    );
    painter.rect_stroke(
        card,
        CornerRadius::same(10),
        Stroke::new(1.0, theme::LINE),
        StrokeKind::Inside,
    );

    let left = card.left() + 18.0;
    painter.text(
        Pos2::new(left, card.top() + 22.0),
        Align2::LEFT_CENTER,
        "Start painting",
        FontId::proportional(15.0),
        theme::TEXT,
    );

    let mut y = card.top() + 48.0;
    for line in [
        "Drag with the brush (B), or press E to erase.",
        "Scroll to zoom · middle-drag to pan.",
        "Shift+drag the Pan tool (H) to turn the canvas.",
        "File ▸ Open image places a photo on a new layer.",
    ] {
        painter.text(
            Pos2::new(left, y),
            Align2::LEFT_CENTER,
            line,
            FontId::proportional(11.0),
            theme::TEXT_MUTED,
        );
        y += 19.0;
    }
}

fn checkerboard(painter: &Painter, page_view: &PageView, page: Rect, clip: Rect) {
    if page.width() <= 0.0 || page.height() <= 0.0 {
        return;
    }

    if page_view.turned() {
        quad(painter, page_view.corners(page), CHECKER_DARK);
    } else {
        painter.rect_filled(page, CornerRadius::ZERO, CHECKER_DARK);
    }

    // The cells are laid out in page space, so the visible region is the clip
    // rect turned back into that space; on an upright view that is the clip
    // rect itself.
    let visible = if page_view.turned() {
        let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
        let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for corner in [clip.min, clip.right_top(), clip.max, clip.left_bottom()] {
            let point = page_view.unpoint(corner);
            min = min.min(point);
            max = max.max(point);
        }
        Rect::from_min_max(page.min + min.to_vec2() * page_view.zoom, page.min + max.to_vec2() * page_view.zoom)
            .intersect(page)
    } else {
        page.intersect(clip)
    };
    if visible.width() <= 1.0 || visible.height() <= 1.0 {
        return;
    }

    let cols = (page.width() / CHECKER_CELL).ceil().max(1.0) as i32;
    let rows = (page.height() / CHECKER_CELL).ceil().max(1.0) as i32;
    let first_col = ((visible.min.x - page.min.x) / CHECKER_CELL).floor() as i32;
    let first_row = ((visible.min.y - page.min.y) / CHECKER_CELL).floor() as i32;
    let last_col = (((visible.max.x - page.min.x) / CHECKER_CELL).ceil() as i32).min(cols);
    let last_row = (((visible.max.y - page.min.y) / CHECKER_CELL).ceil() as i32).min(rows);

    for row in first_row.max(0)..last_row {
        for col in first_col.max(0)..last_col {
            if (row + col) % 2 == 0 {
                continue;
            }
            let min = page.min + Vec2::new(col as f32, row as f32) * CHECKER_CELL;
            let tile = Rect::from_min_size(min, Vec2::splat(CHECKER_CELL)).intersect(page);
            if tile.width() <= 0.0 || tile.height() <= 0.0 {
                continue;
            }
            if page_view.turned() {
                quad(painter, page_view.corners(tile), CHECKER_LIGHT);
            } else {
                painter.rect_filled(tile, CornerRadius::ZERO, CHECKER_LIGHT);
            }
        }
    }
}

fn handle_input(app: &mut InkforgeApp, ui: &egui::Ui, response: &egui::Response, view: Rect) {
    // Turning the sheet: Shift-drag with the Hand tool, the gesture Clip Studio
    // Paint and Photoshop both use for "rotate the view and keep painting".
    let shift = ui.input(|i| i.modifiers.shift);
    let turning =
        app.tool == Tool::Hand && shift && response.dragged_by(PointerButton::Primary);

    let panning = !turning
        && (response.dragged_by(PointerButton::Middle)
            || (app.tool == Tool::Hand && response.dragged_by(PointerButton::Primary)));

    if turning {
        app.view.rotate_by(response.drag_delta().x * ROTATE_PER_PIXEL);
    }

    if panning {
        app.view.pan_by(response.drag_delta());
    }

    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.01 {
            if let Some(cursor) = ui.input(|i| i.pointer.hover_pos()) {
                // Exponential so zooming feels even at every scale.
                let factor = (scroll * 0.0018).exp();
                app.view.zoom_at(factor, cursor, view);
            }
        }

        let cursor_icon = if turning {
            CursorIcon::Grabbing
        } else if panning {
            CursorIcon::Grabbing
        } else if app.tool == Tool::Hand {
            CursorIcon::Grab
        } else {
            CursorIcon::Crosshair
        };
        ui.ctx().set_cursor_icon(cursor_icon);
    }

    paint_input(app, ui, response, view);
}

/// Pointer handling for every canvas tool.
fn paint_input(app: &mut InkforgeApp, ui: &egui::Ui, response: &egui::Response, view: Rect) {
    let doc = app.canvas.size_vec();

    // A polygon lasso is keyboard-finishable: Enter closes it, Escape drops it.
    if app.polygon_in_progress() {
        let (enter, escape) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Escape),
            )
        });
        if escape {
            app.cancel_polygon();
        } else if enter {
            app.finish_polygon();
        }
    }

    // A click without a drag stamps, picks or fills.
    if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let canvas_pos = app.view.screen_to_canvas(view, doc, pointer);
            match app.tool {
                Tool::Eyedropper => app.pick_colour_at(canvas_pos),
                Tool::Fill => app.fill_at(canvas_pos),
                Tool::Brush | Tool::Eraser => app.dab_at(canvas_pos),
                Tool::Selection => match app.selection_mode {
                    crate::engine::SelectionMode::Rectangle => {
                        // A bare click drops the selection instead of creating one.
                        app.begin_selection_drag(canvas_pos);
                        app.end_selection_drag();
                    }
                    crate::engine::SelectionMode::Lasso => app.deselect(),
                    crate::engine::SelectionMode::Polygon => {
                        // The second click of a double-click closes the shape
                        // instead of adding a stray corner.
                        if response.double_clicked() {
                            app.finish_polygon();
                        } else {
                            app.add_polygon_point(canvas_pos);
                        }
                    }
                    crate::engine::SelectionMode::MagicWand => app.wand_at(canvas_pos),
                },
                Tool::Shape | Tool::Hand => {}
            }
        }
    }

    if response.drag_started_by(PointerButton::Primary) {
        // egui only reports a drag once the pointer has moved past its
        // threshold, so start the stroke at the press position, not where the
        // pointer is now. Otherwise a short drag would lose its beginning (and
        // a selection would collapse to nothing).
        let origin = ui
            .input(|i| i.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos());
        if let Some(pointer) = origin {
            let canvas_pos = app.view.screen_to_canvas(view, doc, pointer);
            match app.tool {
                Tool::Brush | Tool::Eraser => app.begin_stroke(canvas_pos),
                Tool::Selection => match app.selection_mode {
                    crate::engine::SelectionMode::Rectangle => app.begin_selection_drag(canvas_pos),
                    crate::engine::SelectionMode::Lasso => app.begin_lasso(canvas_pos),
                    // The polygon lasso and the wand are click-only: a drag
                    // across the canvas is not a region, so they neither
                    // select nor clear.
                    crate::engine::SelectionMode::Polygon
                    | crate::engine::SelectionMode::MagicWand => {}
                },
                Tool::Shape => app.begin_shape_drag(canvas_pos),
                Tool::Eyedropper | Tool::Fill | Tool::Hand => {}
            }
        }
    }

    if response.dragged_by(PointerButton::Primary) {
        if let Some(pointer) = response.interact_pointer_pos() {
            let canvas_pos = app.view.screen_to_canvas(view, doc, pointer);
            match app.tool {
                Tool::Brush | Tool::Eraser => app.extend_stroke(canvas_pos),
                Tool::Selection => match app.selection_mode {
                    crate::engine::SelectionMode::Rectangle => {
                        app.update_selection_drag(canvas_pos)
                    }
                    crate::engine::SelectionMode::Lasso => app.extend_lasso(canvas_pos),
                    crate::engine::SelectionMode::Polygon
                    | crate::engine::SelectionMode::MagicWand => {}
                },
                Tool::Shape => app.update_shape_drag(canvas_pos),
                Tool::Eyedropper | Tool::Fill | Tool::Hand => {}
            }
        }
    }

    if response.drag_stopped_by(PointerButton::Primary) {
        app.end_stroke();
        match app.tool {
            Tool::Selection => match app.selection_mode {
                crate::engine::SelectionMode::Rectangle => app.end_selection_drag(),
                crate::engine::SelectionMode::Lasso => app.end_lasso(),
                crate::engine::SelectionMode::Polygon
                | crate::engine::SelectionMode::MagicWand => {}
            },
            Tool::Shape => app.commit_shape(),
            _ => {}
        }
    }

    // Safety net: the button came up over another widget, so the drag never
    // reported its stop.
    if !ui.input(|i| i.pointer.primary_down()) {
        if app.stroke.is_some() {
            app.end_stroke();
        }
        if app.selection_drag.is_some() {
            app.end_selection_drag();
        }
        if app.lasso_points.is_some() {
            app.end_lasso();
        }
        // A polygon lasso deliberately survives the mouse button coming up:
        // it is drawn click by click, not by dragging.
        if app.shape_drag.is_some() {
            app.commit_shape();
        }
    }
}

/// Draw the shape preview and the selection outline.
fn draw_overlays(app: &InkforgeApp, painter: &Painter, page_view: &PageView) {
    if let Some((start, end)) = app.shape_drag {
        let points: Vec<Pos2> = crate::engine::shape::shape_path(app.shape, start, end)
            .into_iter()
            .map(|point| page_view.point(point))
            .collect();
        if points.len() > 1 {
            painter.add(Shape::line(
                points.clone(),
                Stroke::new(3.0, Color32::from_black_alpha(110)),
            ));
            painter.add(Shape::line(points, Stroke::new(1.5, theme::ACCENT)));
        }
    }

    if app.quick_mask {
        if let Some(selection) = app.canvas.selection() {
            draw_quick_mask(painter, page_view, selection);
        }
    }

    // Selection: an active lasso, an active marquee, or the committed outline.
    // A magic-wand region is a blob, so its ants come from its pixel edges.
    // While a Quick Mask stroke is running the outline is one segment behind
    // the wash, so its ants are left off until the stroke ends.
    let painting_mask = app.is_painting_mask();
    if app.lasso_points.is_none() && app.selection_drag.is_none() && !painting_mask {
        // The Quick Mask wash already shows the soft edge, so the band would
        // only double the tint.
        if !app.quick_mask {
            if let Some(selection) = app.canvas.selection() {
                draw_soft_band(painter, page_view, selection);
            }
        }
        if let Some(edges) = app
            .canvas
            .selection()
            .and_then(|selection| selection.outline_edges())
        {
            for edge in edges {
                let from = page_view.point(edge[0]);
                let to = page_view.point(edge[1]);
                painter.line_segment([from, to], Stroke::new(1.6, Color32::from_black_alpha(150)));
                painter.line_segment([from, to], Stroke::new(0.9, Color32::WHITE));
            }
        }
    }

    let points = if let Some(points) = &app.lasso_points {
        Some(points.clone())
    } else if let Some(points) = &app.selection_polygon {
        // A rubber band to the pointer is what makes the steps clickable,
        // otherwise only the corners drawn so far would be visible.
        let mut path = points.clone();
        if let Some(hover) = painter.ctx().pointer_hover_pos() {
            path.push(page_view.unpoint(hover));
        }
        Some(path)
    } else if let Some((start, end)) = app.selection_drag {
        Some(vec![
            start,
            Pos2::new(end.x, start.y),
            end,
            Pos2::new(start.x, end.y),
        ])
    } else if painting_mask {
        None
    } else {
        app.canvas
            .selection()
            .filter(|selection| selection.outline_edges().is_none())
            .map(|selection| selection.outline())
    };

    if let Some(points) = points {
        let screen: Vec<Pos2> = points.iter().map(|point| page_view.point(*point)).collect();
        // Marching-ants look: a black dashed line with a white phase offset.
        dashed_path(painter, &screen, Color32::BLACK, 0.0);
        dashed_path(painter, &screen, Color32::WHITE, 5.0);
    }
}

/// Tint the part of the selection that is only partly applied, so a feathered
/// edge stays visible while painting instead of only existing in the data.
fn draw_soft_band(painter: &Painter, page_view: &PageView, selection: &Selection) {
    draw_spans(painter, page_view, selection.soft_spans(), theme::ACCENT);
}

/// Quick Mask: the whole selection as a translucent wash, so it reads like a
/// painted mask instead of an outline.
fn draw_quick_mask(painter: &Painter, page_view: &PageView, selection: &Selection) {
    draw_spans(painter, page_view, selection.preview_spans(), QUICK_MASK_COLOUR);
}

/// Draw coverage spans as one mesh: separate translucent rectangles would
/// feather their own edges and show seams between neighbouring rows.
fn draw_spans(painter: &Painter, page_view: &PageView, spans: &[SoftSpan], colour: Color32) {
    if spans.is_empty() {
        return;
    }

    let mut mesh = egui::epaint::Mesh::default();
    for span in spans {
        let [x, y, width, height] = span.rect;
        let min = page_view.point(Pos2::new(x, y));
        let max = page_view.point(Pos2::new(x + width, y + height));
        let to_right = page_view.point(Pos2::new(x + width, y)) - min;
        let to_bottom = page_view.point(Pos2::new(x, y + height)) - min;
        if to_right.length() < 0.5 || to_bottom.length() < 0.5 {
            continue;
        }

        let fill = Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), span.alpha);
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(min, fill);
        mesh.colored_vertex(min + to_right, fill);
        mesh.colored_vertex(max, fill);
        mesh.colored_vertex(min + to_bottom, fill);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }

    if !mesh.vertices.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
}

fn dashed_path(painter: &Painter, points: &[Pos2], colour: Color32, phase: f32) {
    if points.len() < 2 {
        return;
    }

    let stroke = Stroke::new(1.0, colour);
    for pair in points.windows(2) {
        dashed_line(painter, pair[0], pair[1], phase, stroke);
    }
    // Selections are closed shapes: join the last point back to the first.
    dashed_line(
        painter,
        points[points.len() - 1],
        points[0],
        phase,
        stroke,
    );
}

fn dashed_line(painter: &Painter, from: Pos2, to: Pos2, phase: f32, stroke: Stroke) {
    const DASH: f32 = 5.0;
    const GAP: f32 = 5.0;

    let delta = to - from;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    let direction = delta / length;
    let mut offset = -(phase % (DASH + GAP));

    while offset < length {
        let start = offset.max(0.0);
        let end = (offset + DASH).min(length);
        if end > start {
            painter.line_segment([from + direction * start, from + direction * end], stroke);
        }
        offset += DASH + GAP;
    }
}

/// Outline showing the brush diameter on the canvas.
fn brush_cursor(app: &InkforgeApp, ui: &egui::Ui, response: &egui::Response) {
    if app.tool == Tool::Hand || !response.hovered() {
        return;
    }
    let Some(cursor) = ui.input(|i| i.pointer.hover_pos()) else {
        return;
    };

    // The ring shows the dab this reading will actually paint, so a leaned pen
    // widens the outline before the first stroke lands.
    let radius = (app.current_dab(app.tool == Tool::Eraser).radius * app.view.zoom())
        .clamp(1.5, 600.0);
    let painter = ui.painter();

    painter.circle_stroke(
        cursor,
        radius,
        Stroke::new(1.0, Color32::from_white_alpha(200)),
    );
    painter.circle_stroke(
        cursor,
        radius + 1.0,
        Stroke::new(1.0, Color32::from_black_alpha(90)),
    );
    painter.circle_filled(cursor, 1.2, Color32::from_white_alpha(220));
}

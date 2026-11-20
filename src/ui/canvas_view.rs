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

/// Quick Mask tint: a translucent red wash over the selected area.
const QUICK_MASK_COLOUR: Color32 = Color32::from_rgb(0xD2, 0x3B, 0x3B);

/// Size of one transparency-checker square, in screen pixels.
const CHECKER_CELL: f32 = 16.0;
const CHECKER_LIGHT: Color32 = Color32::from_rgb(0xE8, 0xE8, 0xE8);
const CHECKER_DARK: Color32 = Color32::from_rgb(0xC6, 0xC6, 0xC6);

/// Radians the view turns per screen pixel of a Shift-drag with the Hand tool.
/// 0.006 rad is about a third of a degree per pixel: a 200 px drag is 70°.
const ROTATE_PER_PIXEL: f32 = 0.006;

pub fn show(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::CANVAS_SURROUND))
        .show(ui, |ui| {
            let available = ui.available_size();
            if available.x < 8.0 || available.y < 8.0 {
                return;
            }

            let (view, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
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
            handle_input(app, ui, &response, view);
            brush_cursor(app, ui, &response);
        });
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
                    // The wand is click-only: a drag across the canvas is not a
                    // region, so it neither selects nor clears.
                    crate::engine::SelectionMode::MagicWand => {}
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
                    crate::engine::SelectionMode::MagicWand => {}
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
                crate::engine::SelectionMode::MagicWand => {}
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

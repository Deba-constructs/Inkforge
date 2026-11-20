//! Procedurally painted tool icons.
//!
//! Icons are vector art drawn with the egui painter, so they stay crisp at any
//! DPI and the release build ships no bitmap assets to lose.

use inkforge::egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};

use crate::app::Tool;

/// Every icon the workspace can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Brush,
    Eraser,
    Eyedropper,
    Fill,
    Selection,
    Shape,
    Hand,
    Undo,
    Redo,
    ZoomIn,
    ZoomOut,
    Eye,
    EyeOff,
    Lock,
    Plus,
    Trash,
    ArrowUp,
    ArrowDown,
    Duplicate,
    MergeDown,
    Folder,
    FolderPlus,
    ChevronDown,
    ChevronRight,
}

impl Icon {
    /// Map a tool to its rail icon.
    pub fn for_tool(tool: Tool) -> Self {
        match tool {
            Tool::Brush => Self::Brush,
            Tool::Eraser => Self::Eraser,
            Tool::Eyedropper => Self::Eyedropper,
            Tool::Fill => Self::Fill,
            Tool::Selection => Self::Selection,
            Tool::Shape => Self::Shape,
            Tool::Hand => Self::Hand,
        }
    }
}

/// Paint `icon` centred inside `rect`.
pub fn paint(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let side = rect.width().min(rect.height());
    if side < 4.0 {
        return;
    }
    let origin = rect.center() - Vec2::splat(side * 0.5);
    let u = |x: f32, y: f32| Pos2::new(origin.x + x * side, origin.y + y * side);

    let stroke = Stroke::new((side * 0.075).clamp(1.0, 2.2), color);
    let thick = Stroke::new((side * 0.13).clamp(1.4, 3.2), color);

    match icon {
        Icon::Brush => {
            // Handle plus a tapered bristle head.
            painter.line_segment([u(0.82, 0.16), u(0.58, 0.40)], thick);
            painter.add(Shape::convex_polygon(
                vec![u(0.44, 0.36), u(0.64, 0.56), u(0.18, 0.88)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Eraser => {
            painter.add(Shape::closed_line(
                vec![u(0.24, 0.60), u(0.52, 0.24), u(0.78, 0.44), u(0.50, 0.80)],
                stroke,
            ));
            painter.line_segment([u(0.36, 0.42), u(0.66, 0.64)], stroke);
        }
        Icon::Eyedropper => {
            painter.line_segment([u(0.18, 0.86), u(0.58, 0.46)], thick);
            painter.circle_stroke(u(0.68, 0.36), side * 0.14, stroke);
            painter.line_segment([u(0.60, 0.28), u(0.76, 0.44)], Stroke::new(stroke.width * 0.8, color));
        }
        Icon::Fill => {
            painter.add(Shape::closed_line(
                vec![u(0.16, 0.48), u(0.42, 0.22), u(0.68, 0.48), u(0.42, 0.74)],
                stroke,
            ));
            // Falling droplet.
            painter.add(Shape::convex_polygon(
                vec![u(0.80, 0.38), u(0.89, 0.58), u(0.71, 0.58)],
                color,
                Stroke::NONE,
            ));
            painter.circle_filled(u(0.80, 0.62), side * 0.09, color);
        }
        Icon::Selection => {
            let dash = side * 0.12;
            let gap = side * 0.085;
            let (a, b) = (u(0.16, 0.18), u(0.84, 0.82));
            dashed(painter, a, Pos2::new(b.x, a.y), dash, gap, stroke);
            dashed(painter, Pos2::new(b.x, a.y), b, dash, gap, stroke);
            dashed(painter, b, Pos2::new(a.x, b.y), dash, gap, stroke);
            dashed(painter, Pos2::new(a.x, b.y), a, dash, gap, stroke);
        }
        Icon::Lock => {
            // Padlock: a rounded body under a shackle.
            painter.rect_stroke(
                Rect::from_min_max(u(0.26, 0.46), u(0.74, 0.86)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Middle,
            );
            painter.add(Shape::closed_line(
                vec![u(0.36, 0.48), u(0.36, 0.30), u(0.64, 0.30), u(0.64, 0.48)],
                stroke,
            ));
        }
        Icon::Shape => {
            painter.rect_stroke(
                Rect::from_min_max(u(0.14, 0.14), u(0.56, 0.56)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Middle,
            );
            painter.circle_stroke(u(0.68, 0.68), side * 0.20, stroke);
        }
        Icon::Hand => {
            painter.rect_stroke(
                Rect::from_min_max(u(0.30, 0.42), u(0.72, 0.86)),
                CornerRadius::same(7),
                stroke,
                StrokeKind::Middle,
            );
            for (x, top) in [(0.38, 0.20), (0.51, 0.14), (0.64, 0.22)] {
                painter.line_segment([u(x, top), u(x, 0.46)], stroke);
            }
            painter.line_segment([u(0.30, 0.58), u(0.15, 0.46)], stroke);
        }
        Icon::Undo => undo_arrow(painter, &u, stroke, thick, false),
        Icon::Redo => undo_arrow(painter, &u, stroke, thick, true),
        Icon::ZoomIn => {
            magnifier(painter, &u, stroke, side);
            painter.line_segment([u(0.30, 0.44), u(0.54, 0.44)], stroke);
            painter.line_segment([u(0.42, 0.32), u(0.42, 0.56)], stroke);
        }
        Icon::ZoomOut => {
            magnifier(painter, &u, stroke, side);
            painter.line_segment([u(0.30, 0.44), u(0.54, 0.44)], stroke);
        }
        Icon::Eye | Icon::EyeOff => {
            // Lens outline: two arcs meeting at the corners.
            painter.add(Shape::line(
                vec![u(0.10, 0.50), u(0.30, 0.28), u(0.70, 0.28), u(0.90, 0.50)],
                stroke,
            ));
            painter.add(Shape::line(
                vec![u(0.90, 0.50), u(0.70, 0.72), u(0.30, 0.72), u(0.10, 0.50)],
                stroke,
            ));
            painter.circle_filled(u(0.50, 0.50), side * 0.13, color);
            if icon == Icon::EyeOff {
                painter.line_segment([u(0.16, 0.84), u(0.84, 0.16)], thick);
            }
        }
        Icon::Plus => {
            painter.line_segment([u(0.50, 0.18), u(0.50, 0.82)], thick);
            painter.line_segment([u(0.18, 0.50), u(0.82, 0.50)], thick);
        }
        Icon::Trash => {
            painter.line_segment([u(0.16, 0.28), u(0.84, 0.28)], thick);
            painter.line_segment([u(0.38, 0.20), u(0.62, 0.20)], stroke);
            painter.add(Shape::closed_line(
                vec![u(0.26, 0.32), u(0.34, 0.86), u(0.66, 0.86), u(0.74, 0.32)],
                stroke,
            ));
        }
        Icon::ArrowUp => arrow(painter, &u, stroke, true),
        Icon::ArrowDown => arrow(painter, &u, stroke, false),
        Icon::Duplicate => {
            // Two overlapping sheets: the back one behind, the copy in front.
            painter.rect_stroke(
                Rect::from_min_max(u(0.12, 0.12), u(0.62, 0.62)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Middle,
            );
            painter.rect_stroke(
                Rect::from_min_max(u(0.38, 0.38), u(0.88, 0.88)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Middle,
            );
        }
        Icon::Folder | Icon::FolderPlus => {
            painter.add(Shape::closed_line(
                vec![
                    u(0.12, 0.30),
                    u(0.42, 0.30),
                    u(0.50, 0.40),
                    u(0.88, 0.40),
                    u(0.88, 0.82),
                    u(0.12, 0.82),
                ],
                stroke,
            ));
            if icon == Icon::FolderPlus {
                painter.line_segment([u(0.50, 0.50), u(0.50, 0.72)], stroke);
                painter.line_segment([u(0.39, 0.61), u(0.61, 0.61)], stroke);
            }
        }
        Icon::ChevronDown => {
            painter.line_segment([u(0.30, 0.40), u(0.50, 0.62)], stroke);
            painter.line_segment([u(0.50, 0.62), u(0.70, 0.40)], stroke);
        }
        Icon::ChevronRight => {
            painter.line_segment([u(0.40, 0.30), u(0.62, 0.50)], stroke);
            painter.line_segment([u(0.62, 0.50), u(0.40, 0.70)], stroke);
        }
        Icon::MergeDown => {
            painter.line_segment([u(0.50, 0.14), u(0.50, 0.58)], thick);
            painter.line_segment([u(0.50, 0.58), u(0.28, 0.36)], stroke);
            painter.line_segment([u(0.50, 0.58), u(0.72, 0.36)], stroke);
            painter.line_segment([u(0.16, 0.84), u(0.84, 0.84)], stroke);
        }
    }
}

fn arrow(painter: &Painter, u: &impl Fn(f32, f32) -> Pos2, stroke: Stroke, up: bool) {
    let y = |v: f32| if up { v } else { 1.0 - v };
    painter.line_segment([u(0.50, y(0.80)), u(0.50, y(0.24))], stroke);
    painter.line_segment([u(0.50, y(0.24)), u(0.26, y(0.48))], stroke);
    painter.line_segment([u(0.50, y(0.24)), u(0.74, y(0.48))], stroke);
}

fn magnifier(painter: &Painter, u: &impl Fn(f32, f32) -> Pos2, stroke: Stroke, side: f32) {
    painter.circle_stroke(u(0.44, 0.44), side * 0.24, stroke);
    painter.line_segment([u(0.62, 0.62), u(0.86, 0.86)], stroke);
}

fn undo_arrow(
    painter: &Painter,
    u: &impl Fn(f32, f32) -> Pos2,
    stroke: Stroke,
    thick: Stroke,
    mirrored: bool,
) {
    let x = |v: f32| if mirrored { 1.0 - v } else { v };
    let hook = vec![
        u(x(0.22), 0.40),
        u(x(0.50), 0.28),
        u(x(0.72), 0.40),
        u(x(0.76), 0.62),
        u(x(0.58), 0.76),
    ];
    painter.add(Shape::line(hook, thick));
    // Arrow head at the start of the hook.
    painter.line_segment([u(x(0.22), 0.40), u(x(0.38), 0.28)], stroke);
    painter.line_segment([u(x(0.22), 0.40), u(x(0.36), 0.52)], stroke);
}

fn dashed(painter: &Painter, a: Pos2, b: Pos2, dash: f32, gap: f32, stroke: Stroke) {
    let delta = b - a;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    let dir = delta / length;
    let mut t = 0.0;
    while t < length {
        let end = (t + dash).min(length);
        painter.line_segment([a + dir * t, a + dir * end], stroke);
        t = end + gap;
    }
}

/// Paint the INKFORGE brand mark (an ink drop) plus the wordmark.
pub fn paint_logo(painter: &Painter, rect: Rect, accent: Color32, ink: Color32, text: Color32) {
    let mark = Rect::from_min_size(rect.left_center() - Vec2::new(0.0, 11.0), Vec2::splat(22.0));
    let u = |x: f32, y: f32| Pos2::new(mark.min.x + x * mark.width(), mark.min.y + y * mark.height());

    painter.add(Shape::convex_polygon(
        vec![u(0.50, 0.02), u(0.20, 0.54), u(0.80, 0.54)],
        accent,
        Stroke::NONE,
    ));
    painter.circle_filled(u(0.50, 0.60), mark.width() * 0.32, accent);
    // Gloss highlight, cut with the bar colour.
    painter.circle_filled(u(0.40, 0.68), mark.width() * 0.09, ink);

    painter.text(
        Pos2::new(mark.right() + 10.0, mark.center().y),
        inkforge::egui::Align2::LEFT_CENTER,
        "INKFORGE",
        inkforge::egui::FontId::new(16.0, inkforge::egui::FontFamily::Proportional),
        text,
    );
}

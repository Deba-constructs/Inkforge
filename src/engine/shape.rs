//! Shape outlines drawn with the current brush: line, rectangle and ellipse.
//!
//! A shape is just a polyline: the brush is stamped along it exactly as it would
//! be for a freehand stroke, so shapes share the brush tip, grain and opacity.

use crate::egui::Pos2;

use super::surface::segment_bounds;
use super::PixelRegion;

/// The shapes the Shape tool can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Line,
    Rectangle,
    Ellipse,
}

impl ShapeKind {
    pub const ALL: [Self; 3] = [Self::Line, Self::Rectangle, Self::Ellipse];

    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
        }
    }
}

impl Default for ShapeKind {
    fn default() -> Self {
        Self::Line
    }
}

/// Bounding region of a shape outline drawn with a brush of `radius`, clipped
/// to `clip` when a selection is active.
pub fn path_bounds(
    points: &[Pos2],
    radius: f32,
    width: u32,
    height: u32,
    clip: Option<PixelRegion>,
) -> Option<PixelRegion> {
    let mut bounds: Option<PixelRegion> = None;

    for pair in points.windows(2) {
        let Some(segment) = segment_bounds(pair[0], pair[1], radius, width, height) else {
            continue;
        };
        bounds = Some(match bounds {
            Some(existing) => union(existing, segment),
            None => segment,
        });
    }

    let bounds = bounds?;
    match clip {
        Some(clip) => {
            let x = bounds.x.max(clip.x);
            let y = bounds.y.max(clip.y);
            let right = (bounds.x + bounds.width).min(clip.x + clip.width);
            let bottom = (bounds.y + bounds.height).min(clip.y + clip.height);
            let width = right.saturating_sub(x);
            let height = bottom.saturating_sub(y);
            if width == 0 || height == 0 {
                None
            } else {
                Some(PixelRegion {
                    x,
                    y,
                    width,
                    height,
                })
            }
        }
        None => Some(bounds),
    }
}

fn union(a: PixelRegion, b: PixelRegion) -> PixelRegion {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    PixelRegion {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

/// Number of segments used to approximate an ellipse.
const ELLIPSE_SEGMENTS: usize = 72;

/// Points from `from` to `to`. Closed shapes repeat the first point at the end.
pub fn shape_path(kind: ShapeKind, from: Pos2, to: Pos2) -> Vec<Pos2> {
    match kind {
        ShapeKind::Line => vec![from, to],
        ShapeKind::Rectangle => vec![
            from,
            Pos2::new(to.x, from.y),
            to,
            Pos2::new(from.x, to.y),
            from,
        ],
        ShapeKind::Ellipse => {
            let centre = Pos2::new((from.x + to.x) * 0.5, (from.y + to.y) * 0.5);
            let rx = (to.x - from.x).abs() * 0.5;
            let ry = (to.y - from.y).abs() * 0.5;

            let mut points = Vec::with_capacity(ELLIPSE_SEGMENTS + 1);
            for step in 0..=ELLIPSE_SEGMENTS {
                let angle = std::f32::consts::TAU * step as f32 / ELLIPSE_SEGMENTS as f32;
                points.push(Pos2::new(
                    centre.x + angle.cos() * rx,
                    centre.y + angle.sin() * ry,
                ));
            }
            points
        }
    }
}

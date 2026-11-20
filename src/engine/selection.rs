//! Selections: rectangle marquee, freehand lasso and magic wand, with optional
//! feathering.
//!
//! A selection is stored as a bounding box plus an optional coverage mask
//! (`0..=255` per pixel). A rectangle with no feather has no mask at all, which
//! keeps the common case cheap; lassos, wand regions and feathered selections
//! carry a mask and painting multiplies each dab's alpha by that coverage, so
//! soft edges come out genuinely soft instead of clipped.

use std::cell::OnceCell;

use crate::egui::Pos2;

use super::brush::BrushDab;
use super::surface::segment_bounds;
use super::{FillMask, PixelRegion};

/// How the selection tool turns a drag into a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    Rectangle,
    Lasso,
    MagicWand,
}

impl SelectionMode {
    pub const ALL: [Self; 3] = [Self::Rectangle, Self::Lasso, Self::MagicWand];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Lasso => "Lasso",
            Self::MagicWand => "Magic wand",
        }
    }
}

impl Default for SelectionMode {
    fn default() -> Self {
        Self::Rectangle
    }
}

/// Largest feather radius offered, in pixels.
pub const MAX_FEATHER: f32 = 40.0;

/// Largest grow or shrink step offered, in pixels.
pub const MAX_GROW: u8 = 32;

/// Shape of the selection, for drawing its outline.
#[derive(Clone, Debug)]
enum Outline {
    Rect,
    Polygon(Vec<Pos2>),
    /// Boundary edges of a pixel region, for the magic wand.
    Edges(Vec<[Pos2; 2]>),
}

/// Beyond this many boundary segments a wand selection outlines its bounding
/// box instead: thousands of ants cost more than they explain.
const MAX_OUTLINE_EDGES: usize = 4000;

/// One tinted rectangle of a selection's soft edge, in canvas pixels.
#[derive(Clone, Copy, Debug)]
pub struct SoftSpan {
    /// `[x, y, width, height]` in canvas pixels.
    pub rect: [f32; 4],
    /// Opacity, strongest where the selection is least complete.
    pub alpha: u8,
}

/// A selection region with optional soft edges.
#[derive(Clone, Debug)]
pub struct Selection {
    bounds: PixelRegion,
    /// `None` for a hard rectangle; otherwise row-major coverage over `bounds`.
    mask: Option<Vec<u8>>,
    outline: Outline,
    /// Where the edge is only partly selected, so the UI can show how soft it is.
    soft: Vec<SoftSpan>,
    /// Quick Mask spans, built on first use and kept for the life of the selection.
    preview: OnceCell<Vec<SoftSpan>>,
}

impl Selection {
    /// Rectangle from two drag corners with no feathering.
    pub fn from_corners(a: Pos2, b: Pos2, width: u32, height: u32) -> Option<Self> {
        Self::rectangle(a, b, width, height, 0.0)
    }

    /// Rectangle from two drag corners, clamped to the canvas.
    pub fn rectangle(
        a: Pos2,
        b: Pos2,
        width: u32,
        height: u32,
        feather: f32,
    ) -> Option<Self> {
        if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
            return None;
        }

        let left = a.x.min(b.x).clamp(0.0, width as f32);
        let right = a.x.max(b.x).clamp(0.0, width as f32);
        let top = a.y.min(b.y).clamp(0.0, height as f32);
        let bottom = a.y.max(b.y).clamp(0.0, height as f32);

        let x = left.floor() as u32;
        let y = top.floor() as u32;
        let bounds = PixelRegion {
            x,
            y,
            width: (right.ceil() as u32).min(width).saturating_sub(x),
            height: (bottom.ceil() as u32).min(height).saturating_sub(y),
        };

        if bounds.width < 2 || bounds.height < 2 {
            return None;
        }

        let radius = feather_radius(feather);
        if radius == 0 {
            return Some(Self {
                bounds,
                mask: None,
                outline: Outline::Rect,
                soft: Vec::new(),
                preview: OnceCell::new(),
            });
        }

        let mut mask = vec![255u8; bounds.pixel_count()];
        box_blur(&mut mask, bounds.width, bounds.height, radius);
        let soft = soft_spans(&mask, bounds);
        Some(Self {
            bounds,
            mask: Some(mask),
            outline: Outline::Rect,
            soft,
            preview: OnceCell::new(),
        })
    }

    /// Freehand lasso from a dragged polygon, feathered by `feather` pixels.
    ///
    /// The polygon is closed automatically. Returns `None` when it encloses no
    /// area (for example a straight scribble).
    pub fn lasso(points: &[Pos2], width: u32, height: u32, feather: f32) -> Option<Self> {
        if points.len() < 3 {
            return None;
        }
        let finite: Vec<Pos2> = points
            .iter()
            .copied()
            .filter(|point| point.x.is_finite() && point.y.is_finite())
            .collect();
        if finite.len() < 3 {
            return None;
        }

        let min_x = finite.iter().map(|p| p.x).fold(f32::MAX, f32::min);
        let max_x = finite.iter().map(|p| p.x).fold(f32::MIN, f32::max);
        let min_y = finite.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        let max_y = finite.iter().map(|p| p.y).fold(f32::MIN, f32::max);

        let x = min_x.floor().clamp(0.0, width as f32) as u32;
        let y = min_y.floor().clamp(0.0, height as f32) as u32;
        let right = max_x.ceil().clamp(0.0, width as f32) as u32;
        let bottom = max_y.ceil().clamp(0.0, height as f32) as u32;

        let bounds = PixelRegion {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        };
        if bounds.width < 2 || bounds.height < 2 {
            return None;
        }

        // 4×4 supersampling gives an antialiased silhouette before feathering.
        const SAMPLES: u32 = 4;
        let mut mask = vec![0u8; bounds.pixel_count()];
        for row in 0..bounds.height {
            for col in 0..bounds.width {
                let mut hits = 0u32;
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let sample_x = (bounds.x + col) as f32
                            + (sx as f32 + 0.5) / SAMPLES as f32;
                        let sample_y = (bounds.y + row) as f32
                            + (sy as f32 + 0.5) / SAMPLES as f32;
                        if point_in_polygon(&finite, sample_x, sample_y) {
                            hits += 1;
                        }
                    }
                }
                let coverage = (hits * 255) / (SAMPLES * SAMPLES);
                mask[row as usize * bounds.width as usize + col as usize] = coverage as u8;
            }
        }

        if mask.iter().all(|value| *value == 0) {
            return None;
        }

        let radius = feather_radius(feather);
        if radius > 0 {
            box_blur(&mut mask, bounds.width, bounds.height, radius);
        }

        let soft = soft_spans(&mask, bounds);
        Some(Self {
            bounds,
            mask: Some(mask),
            outline: Outline::Polygon(finite),
            soft,
            preview: OnceCell::new(),
        })
    }

    /// Magic wand: turn a flood-fill region into a selection.
    ///
    /// The mask is exact per pixel, so clipping follows the region itself
    /// rather than its bounding box. Feathering blurs the coverage inward and
    /// outward from the region's edge, like the other selection modes.
    pub fn magic_wand(mask: &FillMask, feather: f32) -> Option<Self> {
        let bounds = mask.bounds;
        if bounds.width == 0 || bounds.height == 0 || mask.count() == 0 {
            return None;
        }

        let mut coverage: Vec<u8> = mask
            .pixels
            .iter()
            .map(|filled| if *filled { 255 } else { 0 })
            .collect();
        let radius = feather_radius(feather);
        if radius > 0 {
            box_blur(&mut coverage, bounds.width, bounds.height, radius);
        }

        let outline = coverage_outline(&coverage, bounds);
        let soft = soft_spans(&coverage, bounds);
        Some(Self {
            bounds,
            mask: Some(coverage),
            outline,
            soft,
            preview: OnceCell::new(),
        })
    }

    /// Grow (positive) or shrink (negative) the selection by `pixels`.
    ///
    /// Coverage is dilated or eroded with a square window, so a hard rectangle
    /// grows by exactly `pixels` on each side and a soft edge moves with its
    /// midpoint. The result always carries a mask, and is trimmed to what is
    /// left: shrinking a selection out of existence returns `None`.
    pub fn resized(&self, pixels: i32, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        if pixels == 0 {
            return Some(self.clone());
        }

        let radius = pixels.unsigned_abs().min(MAX_GROW as u32);
        // Pad by the step so growing can leave the old bounds, then clamp.
        let x0 = self.bounds.x.saturating_sub(radius);
        let y0 = self.bounds.y.saturating_sub(radius);
        let x1 = (self.bounds.x + self.bounds.width + radius).min(width);
        let y1 = (self.bounds.y + self.bounds.height + radius).min(height);
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        if w == 0 || h == 0 {
            return None;
        }

        // The working buffer is padded with zeros, so shrinking pulls the edge
        // in even where it runs along the canvas border, and growing can spill
        // into the pad before being trimmed back to the canvas.
        let pad = radius as usize;
        let (bw, bh) = (w as usize + pad * 2, h as usize + pad * 2);
        let mut coverage = vec![0u8; bw * bh];
        for row in 0..h as usize {
            for col in 0..w as usize {
                let value = (self.coverage(x0 + col as u32, y0 + row as u32) * 255.0).round();
                coverage[(row + pad) * bw + (col + pad)] = value as u8;
            }
        }

        morph(&mut coverage, bw as u32, bh as u32, radius, pixels > 0);

        // Trim to what survived inside the canvas, so painting only touches the
        // affected tiles.
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0, 0);
        let mut any = false;
        for row in 0..h {
            for col in 0..w {
                if coverage[(row as usize + pad) * bw + (col as usize + pad)] == 0 {
                    continue;
                }
                any = true;
                min_x = min_x.min(col);
                min_y = min_y.min(row);
                max_x = max_x.max(col);
                max_y = max_y.max(row);
            }
        }
        if !any {
            return None;
        }

        let bounds = PixelRegion {
            x: x0 + min_x,
            y: y0 + min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        };
        let mut mask = Vec::with_capacity(bounds.pixel_count());
        for row in min_y..=max_y {
            for col in min_x..=max_x {
                mask.push(coverage[(row as usize + pad) * bw + (col as usize + pad)]);
            }
        }

        let outline = coverage_outline(&mask, bounds);
        let soft = soft_spans(&mask, bounds);
        Some(Self {
            bounds,
            outline,
            mask: Some(mask),
            soft,
            preview: OnceCell::new(),
        })
    }

    /// Everything outside this selection, clipped to the canvas.
    ///
    /// The result always spans the canvas and carries a mask, so a selection
    /// becomes a hole in it. Feathering is kept as the complement of the
    /// coverage. Inverting a full-canvas selection leaves nothing, hence `None`.
    pub fn inverted(&self, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }

        let bounds = PixelRegion {
            x: 0,
            y: 0,
            width,
            height,
        };
        let mut mask = Vec::with_capacity(bounds.pixel_count());
        let mut any = false;
        for y in 0..height {
            for x in 0..width {
                let inside = (self.coverage(x, y) * 255.0).round();
                let value = 255u8.saturating_sub(inside as u8);
                any |= value > 0;
                mask.push(value);
            }
        }
        if !any {
            return None;
        }

        let outline = coverage_outline(&mask, bounds);
        let soft = soft_spans(&mask, bounds);
        Some(Self {
            bounds,
            outline,
            mask: Some(mask),
            soft,
            preview: OnceCell::new(),
        })
    }

    /// A selection that covers nothing, spanning the canvas.
    ///
    /// Quick Mask painting starts from this when nothing is selected yet: the
    /// brush then adds coverage to an empty field instead of there being no
    /// selection to paint into.
    pub fn empty(width: u32, height: u32) -> Self {
        let bounds = PixelRegion {
            x: 0,
            y: 0,
            width,
            height,
        };
        Self {
            bounds,
            mask: Some(vec![0u8; bounds.pixel_count()]),
            outline: Outline::Rect,
            soft: Vec::new(),
            preview: OnceCell::new(),
        }
    }

    /// True when no pixel is selected at all.
    pub fn is_empty(&self) -> bool {
        match &self.mask {
            None => self.bounds.is_empty(),
            Some(mask) => mask.iter().all(|value| *value == 0),
        }
    }

    /// Paint brush dabs into the selection itself: the brush adds coverage and
    /// the eraser removes it, which is what makes the Quick Mask editable.
    ///
    /// The mask grows to hold anything painted outside the old bounds. Only the
    /// coverage is updated; [`Self::finish_paint`] rebuilds the outline and soft
    /// edge once the stroke ends, so dragging stays responsive on a selection
    /// that spans a large canvas.
    pub fn paint_segment(
        &mut self,
        from: Pos2,
        to: Pos2,
        dab: &BrushDab,
        width: u32,
        height: u32,
    ) -> bool {
        if width == 0 || height == 0 || dab.opacity <= 0.0 {
            return false;
        }
        let Some(region) = segment_bounds(from, to, dab.radius, width, height) else {
            return false;
        };

        self.include(region);
        // A hard rectangle carries no mask; painting needs one to add to and
        // subtract from, so it becomes fully covered first.
        if self.mask.is_none() {
            self.mask = Some(vec![255u8; self.bounds.pixel_count()]);
        }

        // Same dab spacing as the rasteriser, so a mask stroke feels like a
        // paint stroke at the same brush size.
        let radius = dab.radius.max(0.35);
        let spacing = (radius * 2.0 * dab.spacing.clamp(0.02, 1.0)).max(0.75);
        let delta = to - from;
        let steps = (delta.length() / spacing).ceil().max(0.0) as u32;

        let bounds = self.bounds;
        let Some(mask) = self.mask.as_mut() else {
            return false;
        };
        for step in 0..=steps {
            let t = if steps == 0 {
                0.0
            } else {
                step as f32 / steps as f32
            };
            stamp_coverage(mask, bounds, from + delta * t, dab);
        }

        // The wash the Quick Mask view draws comes from this cache.
        self.preview = OnceCell::new();
        true
    }

    /// Rebuild the outline and soft edge after painting the coverage.
    ///
    /// Returns `false` when nothing is selected any more, so the caller can
    /// clear the selection instead of keeping an empty one.
    pub fn finish_paint(&mut self) -> bool {
        let (outline, soft) = {
            let Some(mask) = self.mask.as_deref() else {
                return !self.bounds.is_empty();
            };
            if mask.iter().all(|value| *value == 0) {
                return false;
            }
            (
                coverage_outline(mask, self.bounds),
                soft_spans(mask, self.bounds),
            )
        };

        self.outline = outline;
        self.soft = soft;
        self.preview = OnceCell::new();
        true
    }

    /// Grow the bounds to hold `region`, keeping the coverage already there.
    ///
    /// A hard rectangle becomes a dense mask of 255s, because painting has to
    /// be able to punch a hole in it somewhere.
    fn include(&mut self, region: PixelRegion) {
        let old = self.bounds;
        let x = old.x.min(region.x);
        let y = old.y.min(region.y);
        let right = (old.x + old.width).max(region.x + region.width);
        let bottom = (old.y + old.height).max(region.y + region.height);
        let new = PixelRegion {
            x,
            y,
            width: right - x,
            height: bottom - y,
        };
        if new == old {
            return;
        }

        let old_mask = self.mask.take();
        let old_width = old.width as usize;
        let new_width = new.width as usize;
        let (dx, dy) = ((old.x - x) as usize, (old.y - y) as usize);
        let mut mask = vec![0u8; new.pixel_count()];
        for row in 0..old.height as usize {
            for col in 0..old_width {
                let value = match &old_mask {
                    Some(mask) => mask[row * old_width + col],
                    None => 255,
                };
                if value == 0 {
                    continue;
                }
                mask[(row + dy) * new_width + col + dx] = value;
            }
        }

        self.mask = Some(mask);
        self.bounds = new;
    }

    /// The whole canvas as a hard rectangle.
    pub fn whole(width: u32, height: u32) -> Self {
        Self {
            bounds: PixelRegion {
                x: 0,
                y: 0,
                width,
                height,
            },
            mask: None,
            outline: Outline::Rect,
            soft: Vec::new(),
            preview: OnceCell::new(),
        }
    }

    /// The whole selection as translucent spans, for the Quick Mask view.
    ///
    /// Built on first request and cached, because the caller draws it every
    /// frame while the view is on.
    pub fn preview_spans(&self) -> &[SoftSpan] {
        self.preview.get_or_init(|| build_preview_spans(self))
    }

    /// The spans that show where the edge is soft; empty for a hard selection.
    pub fn soft_spans(&self) -> &[SoftSpan] {
        &self.soft
    }

    pub fn rect(&self) -> PixelRegion {
        self.bounds
    }

    pub fn bounds(&self) -> PixelRegion {
        self.bounds
    }

    pub fn is_feathered(&self) -> bool {
        self.mask.is_some()
    }

    pub fn contains(&self, x: u32, y: u32) -> bool {
        self.coverage(x, y) > 0.0
    }

    /// Selection coverage at a pixel, in `0.0..=1.0`.
    pub fn coverage(&self, x: u32, y: u32) -> f32 {
        let bounds = self.bounds;
        if x < bounds.x
            || y < bounds.y
            || x >= bounds.x + bounds.width
            || y >= bounds.y + bounds.height
        {
            return 0.0;
        }

        match &self.mask {
            None => 1.0,
            Some(mask) => {
                let index = (y - bounds.y) as usize * bounds.width as usize
                    + (x - bounds.x) as usize;
                mask.get(index).copied().unwrap_or(0) as f32 / 255.0
            }
        }
    }

    /// Borrowed view for the rasteriser.
    pub fn clip(&self) -> Clip<'_> {
        Clip {
            bounds: self.bounds,
            mask: self.mask.as_deref(),
        }
    }

    /// Points describing the outline, for the on-screen marquee.
    pub fn outline(&self) -> Vec<Pos2> {
        match &self.outline {
            Outline::Rect => vec![
                Pos2::new(self.bounds.x as f32, self.bounds.y as f32),
                Pos2::new(
                    (self.bounds.x + self.bounds.width) as f32,
                    self.bounds.y as f32,
                ),
                Pos2::new(
                    (self.bounds.x + self.bounds.width) as f32,
                    (self.bounds.y + self.bounds.height) as f32,
                ),
                Pos2::new(
                    self.bounds.x as f32,
                    (self.bounds.y + self.bounds.height) as f32,
                ),
            ],
            Outline::Polygon(points) => points.clone(),
            // A wand region has no single closed path; its bounding box is the
            // honest stand-in for any caller that wants a closed outline.
            Outline::Edges(_) => vec![
                Pos2::new(self.bounds.x as f32, self.bounds.y as f32),
                Pos2::new(
                    (self.bounds.x + self.bounds.width) as f32,
                    self.bounds.y as f32,
                ),
                Pos2::new(
                    (self.bounds.x + self.bounds.width) as f32,
                    (self.bounds.y + self.bounds.height) as f32,
                ),
                Pos2::new(
                    self.bounds.x as f32,
                    (self.bounds.y + self.bounds.height) as f32,
                ),
            ],
        }
    }

    /// Boundary segments of a wand region, for drawing its marching ants.
    pub fn outline_edges(&self) -> Option<&[[Pos2; 2]]> {
        match &self.outline {
            Outline::Edges(edges) => Some(edges),
            _ => None,
        }
    }

    pub fn is_polygon(&self) -> bool {
        matches!(self.outline, Outline::Polygon(_))
    }

    /// e.g. `"320 × 180 px at 40, 60"`.
    pub fn label(&self) -> String {
        let kind = match &self.outline {
            Outline::Rect => "rectangle",
            Outline::Polygon(_) => "lasso",
            Outline::Edges(_) => "magic wand",
        };
        format!(
            "{kind} {} × {} px at {}, {}",
            self.bounds.width, self.bounds.height, self.bounds.x, self.bounds.y
        )
    }
}

/// Stamp one dab into a coverage mask, adding with the brush and multiplying
/// the coverage down with the eraser.
///
/// The falloff matches the rasteriser's, so the painted edge of a mask stroke
/// is as soft as the same brush painting pixels; grain and colour are ignored,
/// since coverage is a single channel.
fn stamp_coverage(mask: &mut [u8], bounds: PixelRegion, centre: Pos2, dab: &BrushDab) {
    if !(centre.x.is_finite() && centre.y.is_finite()) || bounds.is_empty() {
        return;
    }

    let radius = dab.radius.max(0.35);
    let opacity = dab.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let hardness = dab.hardness.clamp(0.0, 1.0);
    let inner = (radius * hardness).min(radius - 0.5).max(0.0);
    let outer = radius + 0.5;

    let right_edge = (bounds.x + bounds.width) as f32;
    let bottom_edge = (bounds.y + bounds.height) as f32;
    let (left, right) = (centre.x - outer, centre.x + outer);
    let (top, bottom) = (centre.y - outer, centre.y + outer);
    if right < bounds.x as f32
        || bottom < bounds.y as f32
        || left >= right_edge
        || top >= bottom_edge
    {
        return;
    }

    let x0 = left.floor().max(bounds.x as f32) as u32;
    let x1 = (right.ceil() as u32).min(bounds.x + bounds.width - 1);
    let y0 = top.floor().max(bounds.y as f32) as u32;
    let y1 = (bottom.ceil() as u32).min(bounds.y + bounds.height - 1);
    let stride = bounds.width as usize;

    for y in y0..=y1 {
        let dy = y as f32 + 0.5 - centre.y;
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - centre.x;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > outer {
                continue;
            }

            let coverage = if distance <= inner {
                1.0
            } else {
                let t = ((distance - inner) / (outer - inner)).clamp(0.0, 1.0);
                1.0 - t * t * (3.0 - 2.0 * t)
            };
            let alpha = coverage * opacity;
            if alpha <= 0.002 {
                continue;
            }

            let slot = &mut mask[(y - bounds.y) as usize * stride + (x - bounds.x) as usize];
            let value = if dab.erase {
                *slot as f32 * (1.0 - alpha)
            } else {
                // Additive with a max, so overlapping dabs of one stroke settle
                // at the brush's coverage instead of building past it.
                (*slot as f32).max(alpha * 255.0)
            };
            *slot = value.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Coverage view passed to painting: a rectangle plus an optional mask.
#[derive(Clone, Copy, Debug)]
pub struct Clip<'a> {
    pub bounds: PixelRegion,
    pub mask: Option<&'a [u8]>,
}

impl Clip<'_> {
    /// Coverage at a canvas pixel, `0.0..=1.0`.
    pub fn coverage(&self, x: u32, y: u32) -> f32 {
        let bounds = self.bounds;
        if x < bounds.x
            || y < bounds.y
            || x >= bounds.x + bounds.width
            || y >= bounds.y + bounds.height
        {
            return 0.0;
        }

        match self.mask {
            None => 1.0,
            Some(mask) => {
                let index = (y - bounds.y) as usize * bounds.width as usize
                    + (x - bounds.x) as usize;
                mask.get(index).copied().unwrap_or(0) as f32 / 255.0
            }
        }
    }

    /// True when the clip fully covers a rectangle (no mask, same bounds).
    pub fn is_full(&self) -> bool {
        self.mask.is_none()
    }
}

/// Buckets the soft edge is split into, from nearly complete to barely there.
const SOFT_BUCKETS: u32 = 4;
/// Above this many spans the band is dropped: it is a hint, not worth tens of
/// thousands of quads on every frame.
const MAX_SOFT_SPANS: usize = 8000;

/// Row spans covering the pixels that are only partly selected.
///
/// This is what makes a feathered edge visible on the canvas: coverage is
/// split into a few buckets on each row and merged into runs, so a feathered
/// blob costs a couple of spans per row rather than one per pixel.
fn soft_spans(mask: &[u8], bounds: PixelRegion) -> Vec<SoftSpan> {
    let width = bounds.width as usize;
    let mut spans = Vec::new();
    for row in 0..bounds.height as usize {
        let line = &mask[row * width..(row + 1) * width];
        let mut col = 0;
        while col < width {
            let Some(bucket) = soft_bucket(line[col]) else {
                col += 1;
                continue;
            };
            let start = col;
            col += 1;
            while col < width && soft_bucket(line[col]) == Some(bucket) {
                col += 1;
            }
            spans.push(SoftSpan {
                rect: [
                    bounds.x as f32 + start as f32,
                    bounds.y as f32 + row as f32,
                    (col - start) as f32,
                    1.0,
                ],
                alpha: (18 + bucket * 16) as u8,
            });
            if spans.len() >= MAX_SOFT_SPANS {
                return spans;
            }
        }
    }
    spans
}

/// Opacity of a fully selected pixel in the Quick Mask view.
const QUICK_MASK_PEAK: u32 = 88;
/// Steps the Quick Mask wash is split into.
const QUICK_MASK_BUCKETS: u32 = 5;
/// Above this many spans the Quick Mask falls back to its bounding box.
const MAX_PREVIEW_SPANS: usize = 20_000;

/// Every covered pixel of the selection as a row run, tinted by coverage, so
/// the Quick Mask view reads like a painted mask rather than an outline.
fn build_preview_spans(selection: &Selection) -> Vec<SoftSpan> {
    let bounds = selection.bounds;
    let whole = SoftSpan {
        rect: [
            bounds.x as f32,
            bounds.y as f32,
            bounds.width as f32,
            bounds.height as f32,
        ],
        alpha: QUICK_MASK_PEAK as u8,
    };

    let Some(mask) = selection.mask.as_deref() else {
        return vec![whole];
    };

    let width = bounds.width as usize;
    let mut spans = Vec::new();
    for row in 0..bounds.height as usize {
        let line = &mask[row * width..(row + 1) * width];
        let mut col = 0;
        while col < width {
            let Some(bucket) = quick_bucket(line[col]) else {
                col += 1;
                continue;
            };
            let start = col;
            col += 1;
            while col < width && quick_bucket(line[col]) == Some(bucket) {
                col += 1;
            }
            spans.push(SoftSpan {
                rect: [
                    bounds.x as f32 + start as f32,
                    bounds.y as f32 + row as f32,
                    (col - start) as f32,
                    1.0,
                ],
                alpha: (bucket * QUICK_MASK_PEAK / (QUICK_MASK_BUCKETS - 1)) as u8,
            });
            if spans.len() >= MAX_PREVIEW_SPANS {
                return vec![whole];
            }
        }
    }
    spans
}

/// `None` for unselected pixels, otherwise a bucket 0..=4 by coverage.
fn quick_bucket(value: u8) -> Option<u32> {
    if value == 0 {
        return None;
    }
    let bucket = (value as u32 * (QUICK_MASK_BUCKETS - 1) + 127) / 255;
    (bucket > 0).then_some(bucket)
}

/// `None` for fully selected or unselected pixels, otherwise a bucket 0..=3
/// where 0 is nearly complete and 3 is barely there.
fn soft_bucket(value: u8) -> Option<u32> {
    if value == 0 || value == 255 {
        return None;
    }
    Some((255 - value as u32) * SOFT_BUCKETS / 255)
}

/// Edge segments tracing the boundary of a coverage mask, in canvas coordinates.
///
/// A wand or grown region is a blob rather than a rectangle or a polygon, so its
/// ants are drawn from the region's own pixel edges, where coverage passes 50%.
/// A very ragged boundary falls back to the bounding box.
fn coverage_outline(coverage: &[u8], bounds: PixelRegion) -> Outline {
    let filled = |x: u32, y: u32| -> bool {
        if x < bounds.x
            || y < bounds.y
            || x >= bounds.x + bounds.width
            || y >= bounds.y + bounds.height
        {
            return false;
        }
        coverage
            .get((y - bounds.y) as usize * bounds.width as usize + (x - bounds.x) as usize)
            .is_some_and(|value| *value >= 128)
    };

    let mut edges: Vec<[Pos2; 2]> = Vec::new();
    for row in 0..bounds.height {
        for col in 0..bounds.width {
            if !filled(bounds.x + col, bounds.y + row) {
                continue;
            }
            let x = bounds.x + col;
            let y = bounds.y + row;
            let (left, right) = (x as f32, (x + 1) as f32);
            let (top, bottom) = (y as f32, (y + 1) as f32);

            if x == 0 || !filled(x - 1, y) {
                edges.push([Pos2::new(left, top), Pos2::new(left, bottom)]);
            }
            if !filled(x + 1, y) {
                edges.push([Pos2::new(right, top), Pos2::new(right, bottom)]);
            }
            if y == 0 || !filled(x, y - 1) {
                edges.push([Pos2::new(left, top), Pos2::new(right, top)]);
            }
            if !filled(x, y + 1) {
                edges.push([Pos2::new(left, bottom), Pos2::new(right, bottom)]);
            }

            if edges.len() > MAX_OUTLINE_EDGES {
                return Outline::Rect;
            }
        }
    }

    Outline::Edges(edges)
}

/// Grow (`keep_max`) or shrink a coverage mask by `radius` pixels.
///
/// A square window is used both ways, so the two cancel each other out exactly
/// and grey (feathered) edges move along with their midpoint.
fn morph(coverage: &mut [u8], width: u32, height: u32, radius: u32, keep_max: bool) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }
    let (w, h) = (width as usize, height as usize);
    let r = radius as usize;
    let mut temp = vec![0u8; coverage.len()];
    let mut line_in = vec![0u8; w.max(h)];
    let mut line_out = vec![0u8; w.max(h)];

    for y in 0..h {
        line_in[..w].copy_from_slice(&coverage[y * w..(y + 1) * w]);
        sliding_line(&line_in[..w], &mut line_out[..w], r, keep_max);
        temp[y * w..(y + 1) * w].copy_from_slice(&line_out[..w]);
    }

    for x in 0..w {
        for y in 0..h {
            line_in[y] = temp[y * w + x];
        }
        sliding_line(&line_in[..h], &mut line_out[..h], r, keep_max);
        for y in 0..h {
            coverage[y * w + x] = line_out[y];
        }
    }
}

/// Sliding-window maximum (`keep_max`) or minimum over one line, O(length).
fn sliding_line(src: &[u8], dst: &mut [u8], radius: usize, keep_max: bool) {
    let len = src.len();
    if len == 0 {
        return;
    }

    let mut deque: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for i in 0..len + radius {
        if i < len {
            while let Some(&back) = deque.back() {
                let dominated = if keep_max {
                    src[back] <= src[i]
                } else {
                    src[back] >= src[i]
                };
                if dominated {
                    deque.pop_back();
                } else {
                    break;
                }
            }
            deque.push_back(i);
        }

        if i >= radius {
            let out = i - radius;
            let left = out.saturating_sub(radius);
            while deque.front().is_some_and(|front| *front < left) {
                deque.pop_front();
            }
            if let Some(&front) = deque.front() {
                dst[out] = src[front];
            }
        }
    }
}

/// Feather radius in whole pixels, clamped to what the UI offers.
fn feather_radius(feather: f32) -> u32 {
    if !feather.is_finite() || feather <= 0.0 {
        return 0;
    }
    (feather.round() as u32).min(MAX_FEATHER as u32)
}

/// Even-odd point-in-polygon test.
fn point_in_polygon(points: &[Pos2], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut previous = points.len() - 1;

    for current in 0..points.len() {
        let (xi, yi) = (points[current].x, points[current].y);
        let (xj, yj) = (points[previous].x, points[previous].y);

        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        previous = current;
    }

    inside
}

/// Separable box blur over a coverage mask, treating outside as zero.
///
/// O(width × height) using prefix sums, so a 40 px feather on a 1080p canvas is
/// still cheap.
fn box_blur(mask: &mut [u8], width: u32, height: u32, radius: u32) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }
    let window = (radius * 2 + 1) as u32;
    let mut temp = vec![0u8; mask.len()];

    // Horizontal pass with a sliding window.
    for row in 0..height {
        let base = row as usize * width as usize;
        let mut sum: u32 = (0..=radius.min(width - 1))
            .map(|x| mask[base + x as usize] as u32)
            .sum();

        for column in 0..width {
            temp[base + column as usize] = (sum / window) as u8;

            let entering = column as i64 + radius as i64 + 1;
            let leaving = column as i64 - radius as i64;
            if entering < width as i64 {
                sum += mask[base + entering as usize] as u32;
            }
            if leaving >= 0 {
                sum -= mask[base + leaving as usize] as u32;
            }
        }
    }

    // Vertical pass with a sliding window.
    for column in 0..width {
        let mut sum: u32 = (0..=radius.min(height - 1))
            .map(|y| temp[y as usize * width as usize + column as usize] as u32)
            .sum();

        for row in 0..height {
            mask[row as usize * width as usize + column as usize] = (sum / window) as u8;

            let entering = row as i64 + radius as i64 + 1;
            let leaving = row as i64 - radius as i64;
            if entering < height as i64 {
                sum += temp[entering as usize * width as usize + column as usize] as u32;
            }
            if leaving >= 0 {
                sum -= temp[leaving as usize * width as usize + column as usize] as u32;
            }
        }
    }
}

//! Raster paint surface.
//!
//! Pixels are stored premultiplied (the same layout [`Color32`] uses), which
//! makes brush compositing a plain `src * a + dst * (1 - a)` on all four
//! channels and gives correct results for soft edges and overlapping stamps.
//!
//! Each layer owns one of these buffers, and the visible composite is another,
//! so the rasteriser is identical whether it paints into a layer or into an
//! offscreen buffer.

use crate::egui::{Color32, Pos2};

use super::brush::BrushDab;
use super::pool::{Task, TaskPool};
use super::selection::Clip;

/// An integer pixel rectangle inside a [`Surface`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRegion {
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// An RGBA raster buffer.
#[derive(Clone, Debug)]
pub struct Surface {
    width: u32,
    height: u32,
    pixels: Vec<Color32>,
    revision: u64,
    /// Union of the pixels changed since the dirty flag was cleared. The
    /// renderer uses it to upload a rectangle instead of the whole canvas.
    dirty: Option<PixelRegion>,
}

impl Surface {
    /// Allocate a fully transparent surface.
    pub fn new(width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            pixels: vec![Color32::TRANSPARENT; width as usize * height as usize],
            revision: 0,
            dirty: None,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Bumped on every change; the renderer uses it to skip uploads.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Rectangle changed since [`Self::clear_dirty`], or `None` when nothing
    /// has changed.
    pub fn dirty(&self) -> Option<PixelRegion> {
        self.dirty
    }

    /// Forget the accumulated dirty rectangle, after it has been uploaded.
    pub fn clear_dirty(&mut self) {
        self.dirty = None;
    }

    /// Grow the dirty rectangle to include `region`.
    fn mark_dirty(&mut self, region: PixelRegion) {
        if region.is_empty() {
            return;
        }
        self.dirty = Some(match self.dirty {
            Some(existing) => union_region(existing, region),
            None => region,
        });
    }

    /// Premultiplied RGBA pixels, row major.
    pub fn pixels(&self) -> &[Color32] {
        &self.pixels
    }

    /// Pixel at a canvas coordinate (transparent when out of bounds).
    pub fn pixel(&self, x: u32, y: u32) -> Color32 {
        if x >= self.width || y >= self.height {
            Color32::TRANSPARENT
        } else {
            self.pixels[y as usize * self.width as usize + x as usize]
        }
    }

    /// Flip or rotate the whole surface, in place.
    ///
    /// The pixels are permuted, never resampled, so the result is lossless and
    /// the inverse transform returns exactly the original bytes.
    pub fn transform(&mut self, transform: super::Transform) {
        let (width, height) = (self.width, self.height);
        let (out_width, out_height) = transform.output_size(width, height);
        let mut pixels = vec![Color32::TRANSPARENT; out_width as usize * out_height as usize];

        for y in 0..out_height {
            for x in 0..out_width {
                let (sx, sy) = transform.source_pixel(x, y, width, height);
                pixels[y as usize * out_width as usize + x as usize] =
                    self.pixels[sy as usize * width as usize + sx as usize];
            }
        }

        self.width = out_width;
        self.height = out_height;
        self.pixels = pixels;
        self.dirty = Some(PixelRegion {
            x: 0,
            y: 0,
            width: out_width,
            height: out_height,
        });
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn clear(&mut self) {
        self.pixels.fill(Color32::TRANSPARENT);
        self.mark_dirty(PixelRegion {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        });
        self.revision = self.revision.wrapping_add(1);
    }

    /// True when nothing has been painted.
    pub fn is_empty(&self) -> bool {
        self.pixels.iter().all(|pixel| pixel.a() == 0)
    }

    /// Smallest rectangle containing every pixel with non-zero alpha, or
    /// `None` when the surface is fully transparent.
    ///
    /// Layer merges use this so history stores the pixels the merge actually
    /// changed instead of a whole-canvas snapshot.
    pub fn opaque_bounds(&self) -> Option<PixelRegion> {
        let width = self.width as usize;
        let mut min_x = self.width;
        let mut min_y = self.height;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut found = false;

        for (index, pixel) in self.pixels.iter().enumerate() {
            if pixel.a() == 0 {
                continue;
            }
            found = true;
            let x = (index % width) as u32;
            let y = (index / width) as u32;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        found.then(|| PixelRegion {
            x: min_x,
            y: min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        })
    }

    /// Copy a rectangular region out, row major. Used by undo/redo.
    pub fn copy_region(&self, region: PixelRegion) -> Vec<Color32> {
        let mut pixels = Vec::with_capacity(region.pixel_count());
        for y in region.y..region.y + region.height {
            for x in region.x..region.x + region.width {
                pixels.push(self.pixel(x, y));
            }
        }
        pixels
    }

    /// Write a region back, ignoring out-of-range pixels and length mismatches.
    pub fn write_region(&mut self, region: PixelRegion, pixels: &[Color32]) {
        if pixels.len() != region.pixel_count() {
            return;
        }

        let mut index = 0;
        for y in region.y..region.y + region.height {
            for x in region.x..region.x + region.width {
                if x < self.width && y < self.height {
                    self.pixels[y as usize * self.width as usize + x as usize] = pixels[index];
                }
                index += 1;
            }
        }

        // Clamp: a region from history should already fit, but the renderer
        // must never be handed a rectangle outside the texture.
        let x = region.x.min(self.width);
        let y = region.y.min(self.height);
        self.mark_dirty(PixelRegion {
            x,
            y,
            width: region.width.min(self.width - x),
            height: region.height.min(self.height - y),
        });
        self.revision = self.revision.wrapping_add(1);
    }

    /// Straight-alpha colour at a canvas coordinate, for the eyedropper.
    ///
    /// Returns `None` when the point is outside the surface or fully
    /// transparent, so the caller can fall back to the canvas background.
    pub fn sample(&self, at: Pos2) -> Option<Color32> {
        if !(at.x.is_finite() && at.y.is_finite()) || at.x < 0.0 || at.y < 0.0 {
            return None;
        }
        let (x, y) = (at.x as u32, at.y as u32);
        if x >= self.width || y >= self.height {
            return None;
        }

        let pixel = self.pixel(x, y);
        let alpha = pixel.a();
        if alpha == 0 {
            return None;
        }

        // Un-premultiply so the picked colour matches what the artist sees.
        let straight = |channel: u8| {
            ((channel as u32 * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8
        };
        Some(Color32::from_rgba_unmultiplied(
            straight(pixel.r()),
            straight(pixel.g()),
            straight(pixel.b()),
            alpha,
        ))
    }

    /// Stamp a line of dabs from `from` to `to` (both in canvas pixels).
    ///
    /// Spacing scales with the brush radius so fast pointer movements leave a
    /// continuous stroke instead of separate blobs, and huge brushes do not
    /// stamp thousands of times per segment.
    pub fn paint_segment(&mut self, from: Pos2, to: Pos2, dab: &BrushDab) {
        self.paint_segment_clipped(from, to, dab, None);
    }

    /// Same as [`Self::paint_segment`], clipped to an optional pixel rectangle
    /// (the selection), so a stroke can never paint outside it.
    pub fn paint_segment_clipped(
        &mut self,
        from: Pos2,
        to: Pos2,
        dab: &BrushDab,
        clip: Option<Clip<'_>>,
    ) {
        self.paint_segment_mode(from, to, dab, clip, false);
    }

    /// Paint a segment into a layer whose alpha channel is locked.
    ///
    /// Pixels that already hold paint are recoloured and keep their alpha,
    /// fully transparent pixels stay empty, and erasing cannot lower alpha, so
    /// on a locked layer the eraser does nothing. That is what "Lock
    /// transparent pixels" means: the shape of the paint cannot change, only
    /// its colour.
    pub fn paint_segment_locked(
        &mut self,
        from: Pos2,
        to: Pos2,
        dab: &BrushDab,
        clip: Option<Clip<'_>>,
    ) {
        self.paint_segment_mode(from, to, dab, clip, true);
    }

    fn paint_segment_mode(
        &mut self,
        from: Pos2,
        to: Pos2,
        dab: &BrushDab,
        clip: Option<Clip<'_>>,
        lock_alpha: bool,
    ) {
        if !(from.x.is_finite() && from.y.is_finite() && to.x.is_finite() && to.y.is_finite()) {
            return;
        }

        let radius = dab.radius.max(0.35);
        // Spacing is a fraction of the diameter; the floor keeps very fine
        // brushes continuous instead of leaving pixel gaps.
        let spacing = (radius * 2.0 * dab.spacing.clamp(0.02, 1.0)).max(0.75);
        let delta = to - from;
        let steps = (delta.length() / spacing).ceil().max(0.0) as u32;

        for step in 0..=steps {
            let t = if steps == 0 {
                0.0
            } else {
                step as f32 / steps as f32
            };
            self.stamp_mode(from + delta * t, dab, clip, lock_alpha);
        }

        self.revision = self.revision.wrapping_add(1);
    }

    /// Stamp a single dab.
    pub fn stamp(&mut self, centre: Pos2, dab: &BrushDab) {
        self.stamp_clipped(centre, dab, None);
    }

    /// Stamp a single dab, choosing the worker count automatically.
    fn stamp_clipped(&mut self, centre: Pos2, dab: &BrushDab, clip: Option<Clip<'_>>) {
        self.stamp_mode(centre, dab, clip, false);
    }

    /// Stamp a single dab with an explicit alpha-lock choice.
    fn stamp_mode(
        &mut self,
        centre: Pos2,
        dab: &BrushDab,
        clip: Option<Clip<'_>>,
        lock_alpha: bool,
    ) {
        self.stamp_clipped_with_workers(centre, dab, clip, None, lock_alpha);
    }

    /// Stamp a single dab with an explicit number of workers.
    ///
    /// Production painting picks the count from the machine. This entry point
    /// exists so tests can prove the single-worker and many-worker paths
    /// produce exactly the same pixels, which is what makes the split safe.
    pub fn stamp_with_workers(&mut self, centre: Pos2, dab: &BrushDab, workers: usize) {
        self.stamp_clipped_with_workers(centre, dab, None, Some(workers.max(1)), false);
    }

    /// Stamp a single dab, clipped to an optional selection.
    fn stamp_clipped_with_workers(
        &mut self,
        centre: Pos2,
        dab: &BrushDab,
        clip: Option<Clip<'_>>,
        workers: Option<usize>,
        lock_alpha: bool,
    ) {
        if !(centre.x.is_finite() && centre.y.is_finite()) {
            return;
        }

        let radius = dab.radius.max(0.35);
        let opacity = dab.opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return;
        }

        // A hard brush still keeps a one-pixel feathered edge, otherwise round
        // stamps look jagged at every zoom level.
        let outer = radius + 0.5;

        let left = centre.x - outer;
        let right = centre.x + outer;
        let top = centre.y - outer;
        let bottom = centre.y + outer;
        if right < 0.0 || bottom < 0.0 || left > self.width as f32 || top > self.height as f32 {
            return;
        }

        let x0 = left.floor().max(0.0) as u32;
        let x1 = (right.ceil() as u32).min(self.width.saturating_sub(1));
        let y0 = top.floor().max(0.0) as u32;
        let y1 = (bottom.ceil() as u32).min(self.height.saturating_sub(1));

        let bounds = clip.as_ref().map(|clip| clip.bounds);
        let (x0, x1, y0, y1) = match bounds {
            Some(bounds) => {
                let right = bounds.x + bounds.width;
                let bottom = bounds.y + bounds.height;
                if x1 < bounds.x || y1 < bounds.y || x0 >= right || y0 >= bottom {
                    return;
                }
                (
                    x0.max(bounds.x),
                    x1.min(right - 1),
                    y0.max(bounds.y),
                    y1.min(bottom - 1),
                )
            }
            None => (x0, x1, y0, y1),
        };

        // Every row of a dab is independent, so a large dab is split into
        // horizontal bands. Each band owns a disjoint slice of the pixel
        // buffer, which means no locks and no shared writes, and each pixel is
        // still computed with exactly the same maths as before, so the result
        // does not depend on how many workers ran.
        let width = self.width as usize;
        let rows = (y1 - y0 + 1) as usize;
        let band_start = y0 as usize * width;
        let band_end = (y1 as usize + 1) * width;
        let pixels = &mut self.pixels[band_start..band_end];

        // Spread only as wide as the dab has work for: starting a thread costs
        // more than a small dab saves.
        let area = rows * (x1 - x0 + 1) as usize;
        let worker_count = workers
            .unwrap_or_else(|| (area / PIXELS_PER_WORKER).clamp(1, stamp_workers()))
            .clamp(1, rows.max(1));

        if worker_count <= 1 {
            stamp_rows(pixels, width, y0, x0, x1, centre, dab, clip, lock_alpha);
        } else {
            let rows_per_worker = rows.div_ceil(worker_count);
            let raw_clip = clip.map(RawClip::from);
            let mut tasks: Vec<Task> = Vec::with_capacity(worker_count);
            for (index, band) in pixels.chunks_mut(rows_per_worker * width).enumerate() {
                let job = StampBand {
                    pixels: SendPtr(band.as_mut_ptr()),
                    len: band.len(),
                    stride: width,
                    first_row: y0 + (index * rows_per_worker) as u32,
                    x0,
                    x1,
                    centre,
                    dab: *dab,
                    clip: raw_clip,
                    lock_alpha,
                };
                tasks.push(Box::new(move || job.run()));
            }

            match TaskPool::global() {
                Some(pool) => pool.run(tasks),
                // A single-core machine still gets correct pixels, just
                // sequentially.
                None => tasks.into_iter().for_each(|task| task()),
            }
        }

        self.mark_dirty(PixelRegion {
            x: x0,
            y: y0,
            width: x1 - x0 + 1,
            height: y1 - y0 + 1,
        });
        self.revision = self.revision.wrapping_add(1);
    }
}

/// Work that justifies one worker, in pixels.
///
/// A dab is split into at most one band per this many pixels, so a 40 px brush
/// stays single-threaded while a 600 px brush uses every core. Measured on 8
/// cores: 600 px dab 7.0 ms with one worker, 1.4 ms with eight.
const PIXELS_PER_WORKER: usize = 96 * 96;

/// Worker count for large dabs: every available core, capped at eight because
/// painting is memory-bandwidth bound beyond that.
fn stamp_workers() -> usize {
    use std::sync::OnceLock;
    static WORKERS: OnceLock<usize> = OnceLock::new();
    *WORKERS.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .clamp(1, 8)
    })
}

/// A pointer carried to a pool worker.
///
/// SAFETY: sending one is only sound because [`TaskPool::run`] does not return
/// until every task using it has finished, and the caller keeps the buffer
/// borrowed and untouched for that whole time. Bands never overlap.
struct SendPtr<T>(*mut T);
unsafe impl<T> Send for SendPtr<T> {}

/// An owned copy of the selection bounds and mask pointer for one band.
///
/// SAFETY: the mask is read-only and outlives the stamping call, which is the
/// same guarantee as `Clip` itself; only the lifetime has been erased so the
/// band can travel to a worker thread.
#[derive(Clone, Copy)]
struct RawClip {
    bounds: PixelRegion,
    mask: Option<(*const u8, usize)>,
}
unsafe impl Send for RawClip {}

impl From<Clip<'_>> for RawClip {
    fn from(clip: Clip<'_>) -> Self {
        Self {
            bounds: clip.bounds,
            mask: clip.mask.map(|mask| (mask.as_ptr(), mask.len())),
        }
    }
}

/// One horizontal band of a dab, ready to run on a worker thread.
struct StampBand {
    pixels: SendPtr<Color32>,
    len: usize,
    stride: usize,
    first_row: u32,
    x0: u32,
    x1: u32,
    centre: Pos2,
    dab: BrushDab,
    clip: Option<RawClip>,
    lock_alpha: bool,
}

impl StampBand {
    fn run(self) {
        // SAFETY: see `SendPtr`. The slice covers exactly the rows this band
        // was split from, and no other task touches them.
        let band = unsafe { std::slice::from_raw_parts_mut(self.pixels.0, self.len) };
        let clip = self.clip.map(|raw| Clip {
            bounds: raw.bounds,
            // SAFETY: see `RawClip`; the selection mask is only read.
            mask: raw
                .mask
                .map(|(ptr, len)| unsafe { std::slice::from_raw_parts(ptr, len) }),
        });
        stamp_rows(
            band,
            self.stride,
            self.first_row,
            self.x0,
            self.x1,
            self.centre,
            &self.dab,
            clip,
            self.lock_alpha,
        );
    }
}

/// Stamp one horizontal band of a dab into a contiguous block of rows.
///
/// `first_row` is the canvas row the first row of `band` maps to; `stride` is
/// the surface width. Only columns `x0..=x1` are written.
#[allow(clippy::too_many_arguments)]
fn stamp_rows(
    band: &mut [Color32],
    stride: usize,
    first_row: u32,
    x0: u32,
    x1: u32,
    centre: Pos2,
    dab: &BrushDab,
    clip: Option<Clip<'_>>,
    lock_alpha: bool,
) {
    if band.is_empty() {
        return;
    }

    let radius = dab.radius.max(0.35);
    let opacity = dab.opacity.clamp(0.0, 1.0);
    let hardness = dab.hardness.clamp(0.0, 1.0);
    let inner = (radius * hardness).min(radius - 0.5).max(0.0);
    let outer = radius + 0.5;
    let grain = dab.grain.clamp(0.0, 1.0);
    let (sr, sg, sb, sa) = (
        dab.colour.r() as f32,
        dab.colour.g() as f32,
        dab.colour.b() as f32,
        dab.colour.a() as f32,
    );

    for (row, line) in band.chunks_mut(stride).enumerate() {
        let y = first_row + row as u32;
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
                // Smoothstep falloff: soft, artefact-free edge.
                1.0 - t * t * (3.0 - 2.0 * t)
            };
            // Selection coverage scales the dab, so feathered edges fade
            // instead of being cut off.
            let clip_coverage = match &clip {
                Some(clip) => clip.coverage(x, y),
                None => 1.0,
            };
            if clip_coverage <= 0.0 {
                continue;
            }

            let mut alpha = coverage * opacity * clip_coverage;
            if grain > 0.0 {
                // Position-based noise, so grain is a property of the canvas
                // (like paper tooth) rather than of a particular stroke.
                alpha *= 1.0 - grain * grain_noise(x, y);
            }
            if alpha <= 0.002 {
                continue;
            }

            let slot = &mut line[x as usize];
            let destination = *slot;

            if lock_alpha {
                // Alpha is frozen: a transparent pixel has nothing to recolour
                // and the eraser cannot lower alpha, so both are left alone.
                let kept = destination.a() as f32 / 255.0;
                if kept <= 0.0 || dab.erase {
                    continue;
                }
                let inverse = 1.0 - alpha;
                *slot = Color32::from_rgba_premultiplied(
                    to_u8(sr * alpha * kept + destination.r() as f32 * inverse),
                    to_u8(sg * alpha * kept + destination.g() as f32 * inverse),
                    to_u8(sb * alpha * kept + destination.b() as f32 * inverse),
                    destination.a(),
                );
                continue;
            }

            let inverse = 1.0 - alpha;

            let blended = if dab.erase {
                // Destination-out: erase alpha and colour together.
                Color32::from_rgba_premultiplied(
                    to_u8(destination.r() as f32 * inverse),
                    to_u8(destination.g() as f32 * inverse),
                    to_u8(destination.b() as f32 * inverse),
                    to_u8(destination.a() as f32 * inverse),
                )
            } else {
                // Source-over, both sides already premultiplied.
                Color32::from_rgba_premultiplied(
                    to_u8(sr * alpha + destination.r() as f32 * inverse),
                    to_u8(sg * alpha + destination.g() as f32 * inverse),
                    to_u8(sb * alpha + destination.b() as f32 * inverse),
                    to_u8(sa * alpha + destination.a() as f32 * inverse),
                )
            };

            *slot = blended;
        }
    }
}

/// Smallest rectangle containing both inputs.
fn union_region(a: PixelRegion, b: PixelRegion) -> PixelRegion {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    PixelRegion {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

/// Bounding region (clamped to a surface of `width` × `height`) that a brush
/// segment with `radius` can touch.
///
/// Shared by painting, which needs it to invalidate the layer composite, and by
/// undo, which needs it to know which tiles to snapshot.
pub fn segment_bounds(
    from: Pos2,
    to: Pos2,
    radius: f32,
    width: u32,
    height: u32,
) -> Option<PixelRegion> {
    if !(from.x.is_finite() && from.y.is_finite() && to.x.is_finite() && to.y.is_finite()) {
        return None;
    }

    let margin = radius.max(0.0) + 1.0;
    let min_x = from.x.min(to.x) - margin;
    let max_x = from.x.max(to.x) + margin;
    let min_y = from.y.min(to.y) - margin;
    let max_y = from.y.max(to.y) + margin;

    let width_f = width as f32;
    let height_f = height as f32;

    if max_x < 0.0 || max_y < 0.0 || min_x > width_f || min_y > height_f {
        return None;
    }

    let x = min_x.floor().max(0.0).min(width_f) as u32;
    let y = min_y.floor().max(0.0).min(height_f) as u32;
    let right = max_x.ceil().max(0.0).min(width_f) as u32;
    let bottom = max_y.ceil().max(0.0).min(height_f) as u32;

    let region = PixelRegion {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    };

    if region.is_empty() {
        None
    } else {
        Some(region)
    }
}

fn to_u8(value: f32) -> u8 {
    value.clamp(0.0, 255.0).round() as u8
}

/// Deterministic per-pixel noise in `0.0..=1.0`.
///
/// Cheap integer hash: stable across runs and platforms, which keeps brush
/// output reproducible in tests while still looking like grain.
fn grain_noise(x: u32, y: u32) -> f32 {
    let mut hash = x.wrapping_mul(0x1653_1F5D) ^ y.wrapping_mul(0x27D4_EB2D);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x85EB_CA6B);
    hash ^= hash >> 13;
    ((hash & 0xFFFF) as f32) / 65535.0
}

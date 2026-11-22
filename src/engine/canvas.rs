//! The canvas document: pixel dimensions, background and the layer stack.
//!
//! Painting goes into one layer at a time and the stack keeps a cached composite
//! for display. The background sits underneath every layer and is a document
//! setting rather than paint.

use crate::egui::{Color32, Pos2, Vec2};
use crate::layers::{
    composite_pixel, BlendMode, Entry, FlattenedStack, GroupProps, Layer, LayerId, LayerProps,
    LayerStack, MergedLayer,
};

use super::{
    fill::flood_mask, segment_bounds, shape::path_bounds, BrushDab, PixelRegion, Selection,
    ShapeKind, Surface, Transform,
};

/// Clip a region to a page, returning an empty region when it misses it.
fn clamp_region(region: PixelRegion, width: u32, height: u32) -> PixelRegion {
    let x = region.x.min(width);
    let y = region.y.min(height);
    PixelRegion {
        x,
        y,
        width: region.width.min(width - x),
        height: region.height.min(height - y),
    }
}

/// Recolour a pixel without changing its alpha, for a layer with locked
/// transparency.
///
/// The surface stores premultiplied colour, so the straight colour is recovered
/// first, mixed towards `colour` by `coverage`, and premultiplied again by the
/// alpha the pixel already had. A fully transparent pixel has no colour to
/// change and comes back untouched, which is exactly what the lock promises.
fn paint_locked_pixel(destination: Color32, colour: Color32, coverage: f32) -> Color32 {
    let alpha = destination.a() as f32 / 255.0;
    if alpha <= 0.0 {
        return destination;
    }
    let t = if coverage.is_finite() {
        coverage.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let mix = |channel: u8, was: u8| {
        let straight = was as f32 / alpha;
        let value = channel as f32 * t + straight * (1.0 - t);
        (value * alpha).clamp(0.0, 255.0).round() as u8
    };
    Color32::from_rgba_premultiplied(
        mix(colour.r(), destination.r()),
        mix(colour.g(), destination.g()),
        mix(colour.b(), destination.b()),
        destination.a(),
    )
}

/// Smallest allowed canvas edge, in pixels.
pub const MIN_CANVAS_DIM: u32 = 16;
/// Largest allowed canvas edge, in pixels.
///
/// Tall print and web-comic pages are legitimate (an A0 page at 300 dpi is
/// 9933 × 14043 px, a webtoon episode 800 × 12800 px), so the edge limit is
/// generous and the real guard is the area limit below.
pub const MAX_CANVAS_DIM: u32 = 20_000;
/// Largest allowed canvas area, in pixels.
///
/// Every layer is a full-canvas RGBA surface, so 200 MP is 800 MB per layer.
/// Anything past this is refused before a single buffer is allocated, which is
/// what turns "my machine froze" into a readable error message.
pub const MAX_CANVAS_PIXELS: u64 = 200_000_000;

/// Check a pixel size against both limits without allocating anything.
///
/// The "New canvas" window calls this before creating the document, so an
/// impossible size is reported inline instead of aborting on a failed
/// allocation.
pub fn validate_canvas_size(width: u32, height: u32) -> Result<(), String> {
    if !(MIN_CANVAS_DIM..=MAX_CANVAS_DIM).contains(&width)
        || !(MIN_CANVAS_DIM..=MAX_CANVAS_DIM).contains(&height)
    {
        return Err(format!(
            "Canvas size must be between {MIN_CANVAS_DIM} and {MAX_CANVAS_DIM} pixels \
             on each side (got {width} × {height})."
        ));
    }

    let pixels = width as u64 * height as u64;
    if pixels > MAX_CANVAS_PIXELS {
        let megapixels = pixels as f64 / 1_000_000.0;
        return Err(format!(
            "{width} × {height} px is {megapixels:.0} megapixels, over the \
             {:.0} megapixel limit (about 800 MB per layer). Reduce the size or \
             the resolution.",
            MAX_CANVAS_PIXELS as f64 / 1_000_000.0
        ));
    }

    Ok(())
}

/// What sits underneath the artwork.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    White,
    Transparent,
    Custom(Color32),
}

impl Background {
    /// Short label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Transparent => "Transparent",
            Self::Custom(_) => "Custom",
        }
    }

    /// Colour the canvas view paints the page with; `None` means the
    /// transparency checkerboard.
    pub fn fill(self) -> Option<Color32> {
        match self {
            Self::White => Some(Color32::WHITE),
            Self::Transparent => None,
            Self::Custom(color) => Some(color),
        }
    }
}

impl Default for Background {
    fn default() -> Self {
        Self::White
    }
}

/// A single artwork document.
pub struct Canvas {
    name: String,
    width: u32,
    height: u32,
    background: Background,
    stack: LayerStack,
    /// Editing selection; clips painting, erasing, filling and shapes.
    selection: Option<Selection>,
    modified: bool,
    /// Physical resolution. The engine works in pixels; this is the bridge to
    /// print sizes, the PNG `pHYs` chunk and physical ruler units.
    dpi: f32,
    /// Width of one pixel relative to its height. 1.0 is square pixels; 2.0
    /// means every pixel is twice as wide as it is tall (anamorphic video).
    pixel_aspect: f32,
    /// Bleed, in pixels, included on every side of the canvas for print work.
    bleed_px: f32,
    /// Safe-area margin, in pixels, measured inwards from the trim edge.
    safe_px: f32,
}

/// Default resolution for documents that do not carry one (screen artwork and
/// every project written before version 3 of the format).
pub const DEFAULT_DPI: f32 = 72.0;
/// Widest resolution the document model accepts, in dots per inch.
pub const MAX_DPI: f32 = 2_400.0;
/// Smallest accepted pixel aspect ratio.
pub const MIN_PIXEL_ASPECT: f32 = 0.1;
/// Largest accepted pixel aspect ratio.
pub const MAX_PIXEL_ASPECT: f32 = 10.0;

impl Canvas {
    /// The artwork opened on first launch.
    pub fn default_artwork() -> Self {
        Self::build(String::from("Untitled"), 1920, 1080, Background::White)
    }

    fn build(name: String, width: u32, height: u32, background: Background) -> Self {
        Self {
            name,
            width,
            height,
            background,
            stack: LayerStack::new(width, height),
            selection: None,
            modified: false,
            dpi: DEFAULT_DPI,
            pixel_aspect: 1.0,
            bleed_px: 0.0,
            safe_px: 0.0,
        }
    }

    /// Validate the requested size and build a canvas.
    ///
    /// Returns a human-readable error instead of panicking so the "New canvas"
    /// dialog can show it inline.
    pub fn try_new(
        name: impl Into<String>,
        width: u32,
        height: u32,
        background: Background,
    ) -> Result<Self, String> {
        validate_canvas_size(width, height)?;

        let name = name.into();
        let name = if name.trim().is_empty() {
            String::from("Untitled")
        } else {
            name.trim().to_owned()
        };

        Ok(Self::build(name, width, height, background))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rename the document (used when a project is saved under a new name).
    pub fn set_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !name.trim().is_empty() {
            self.name = name.trim().to_owned();
        }
    }

    /// Rebuild a document from a loaded layer stack.
    pub fn from_entries(
        name: impl Into<String>,
        width: u32,
        height: u32,
        background: Background,
        entries: Vec<Entry>,
        active: usize,
    ) -> Result<Self, String> {
        validate_canvas_size(width, height)
            .map_err(|error| format!("The document cannot be reopened. {error}"))?;
        if entries.is_empty() {
            return Err(String::from("The document contains no layers."));
        }

        let name = name.into();
        let name = if name.trim().is_empty() {
            String::from("Untitled")
        } else {
            name.trim().to_owned()
        };

        Ok(Self {
            name,
            width,
            height,
            background,
            stack: LayerStack::from_entries(width, height, entries, active),
            selection: None,
            modified: false,
            dpi: DEFAULT_DPI,
            pixel_aspect: 1.0,
            bleed_px: 0.0,
            safe_px: 0.0,
        })
    }

    // ---- print setup ----------------------------------------------------

    /// Resolution in dots (pixels) per inch.
    pub fn dpi(&self) -> f32 {
        self.dpi
    }

    /// Width of one pixel relative to its height (1.0 = square).
    pub fn pixel_aspect(&self) -> f32 {
        self.pixel_aspect
    }

    /// Bleed included around the trim edge, in pixels.
    pub fn bleed_px(&self) -> f32 {
        self.bleed_px
    }

    /// Safe-area margin measured inwards from the trim edge, in pixels.
    pub fn safe_px(&self) -> f32 {
        self.safe_px
    }

    /// Install the print setup loaded from a project or chosen at creation.
    ///
    /// Values are clamped rather than rejected: a project should still open if
    /// a hand-edited manifest carries a nonsense resolution. This is document
    /// metadata, not artwork, so it does not mark the document modified.
    pub fn set_print_setup(&mut self, dpi: f32, pixel_aspect: f32, bleed_px: f32, safe_px: f32) {
        self.dpi = if dpi.is_finite() {
            dpi.clamp(1.0, MAX_DPI)
        } else {
            DEFAULT_DPI
        };
        self.pixel_aspect = if pixel_aspect.is_finite() {
            pixel_aspect.clamp(MIN_PIXEL_ASPECT, MAX_PIXEL_ASPECT)
        } else {
            1.0
        };
        let limit = self.width.min(self.height) as f32 / 2.0;
        self.bleed_px = if bleed_px.is_finite() {
            bleed_px.clamp(0.0, limit)
        } else {
            0.0
        };
        self.safe_px = if safe_px.is_finite() {
            safe_px.clamp(0.0, limit)
        } else {
            0.0
        };
    }

    /// Physical size of the artwork at its resolution, in millimetres.
    ///
    /// A pixel is `pixel_aspect` times wider than it is tall, so the horizontal
    /// size uses the same density for both axes but a stretched pixel width.
    pub fn print_size_mm(&self) -> (f32, f32) {
        let dpi = self.dpi.max(1.0);
        let width = self.width as f32 * self.pixel_aspect / dpi * 25.4;
        let height = self.height as f32 / dpi * 25.4;
        (width, height)
    }

    /// Bleed in millimetres, for display.
    pub fn bleed_mm(&self) -> f32 {
        self.bleed_px / self.dpi.max(1.0) * 25.4
    }

    /// Safe-area margin in millimetres, for display.
    pub fn safe_mm(&self) -> f32 {
        self.safe_px / self.dpi.max(1.0) * 25.4
    }

    /// e.g. `"300 dpi"`.
    pub fn dpi_label(&self) -> String {
        format!("{:.0} dpi", self.dpi)
    }

    /// Forget unsaved changes, e.g. right after a successful save.
    pub fn mark_saved(&mut self) {
        self.modified = false;
    }

    /// Flag the document as having unsaved changes, e.g. after a recovery.
    pub fn mark_modified(&mut self) {
        self.modified = true;
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn background(&self) -> Background {
        self.background
    }

    pub fn set_background(&mut self, background: Background) {
        if self.background != background {
            self.background = background;
            self.modified = true;
        }
    }

    // ---- layers and groups --------------------------------------------

    /// Bottom to top; a group is followed by its children.
    pub fn entries(&self) -> &[Entry] {
        self.stack.entries()
    }

    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.stack.entry(index)
    }

    /// Number of paint layers (groups are not counted).
    pub fn layer_count(&self) -> usize {
        self.stack.layer_count()
    }

    /// Number of rows in the stack, including groups.
    pub fn entry_count(&self) -> usize {
        self.stack.len()
    }

    pub fn active_entry(&self) -> &Entry {
        self.stack.active_entry()
    }

    /// The active entry when it is a paint layer; painting needs one.
    pub fn active_layer(&self) -> Option<&Layer> {
        self.stack.active_layer()
    }

    pub fn active_layer_id(&self) -> Option<LayerId> {
        self.stack.active_layer_id()
    }

    /// Id of the active entry, which may be a group.
    pub fn active_id(&self) -> LayerId {
        self.stack.active_id()
    }

    pub fn active_layer_index(&self) -> usize {
        self.stack.active_index()
    }

    pub fn set_active_layer(&mut self, index: usize) {
        self.stack.set_active_index(index);
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.stack.layer(id)
    }

    pub fn group(&self, id: LayerId) -> Option<&crate::layers::Group> {
        self.stack.group(id)
    }

    pub fn layer_surface(&self, id: LayerId) -> Option<&Surface> {
        self.stack.surface(id)
    }

    /// Mutable layer pixels, used by undo/redo. Marks the document modified.
    pub fn layer_surface_mut(&mut self, id: LayerId) -> Option<&mut Surface> {
        self.modified = true;
        self.stack.surface_mut(id)
    }

    // ---- layer masks ---------------------------------------------------

    /// True when this layer has a mask.
    pub fn layer_has_mask(&self, id: LayerId) -> bool {
        self.stack.layer(id).is_some_and(Layer::has_mask)
    }

    /// A layer's mask, if it has one.
    pub fn layer_mask(&self, id: LayerId) -> Option<&Surface> {
        self.stack.layer(id).and_then(Layer::mask)
    }

    /// Mutable mask pixels, used by undo/redo. Marks the document modified.
    pub fn layer_mask_mut(&mut self, id: LayerId) -> Option<&mut Surface> {
        self.modified = true;
        self.stack.layer_mut(id).and_then(Layer::mask_mut)
    }

    /// Install or remove a mask, returning the one that was there before.
    pub fn set_layer_mask(&mut self, id: LayerId, mask: Option<Surface>) -> Option<Surface> {
        let previous = self
            .stack
            .layer_mut(id)
            .map(|layer| layer.set_mask(mask))?;
        self.modified = true;
        self.invalidate_composite(None);
        previous
    }

    /// Create a full-canvas mask (`reveal` shows all, otherwise hides all).
    pub fn add_layer_mask(&mut self, id: LayerId, reveal: bool) -> bool {
        let added = self
            .stack
            .layer_mut(id)
            .is_some_and(|layer| layer.add_mask(reveal));
        if added {
            self.modified = true;
            self.invalidate_composite(None);
        }
        added
    }

    /// Invert a mask's coverage; returns the whole-canvas region it changed.
    pub fn invert_layer_mask(&mut self, id: LayerId) -> Option<PixelRegion> {
        let mask = self.stack.layer_mut(id).and_then(Layer::mask_mut)?;
        mask.invert_coverage();
        self.modified = true;
        let region = PixelRegion {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        };
        self.invalidate_composite(Some(region));
        Some(region)
    }

    /// Paint a brush segment into one layer's mask and mark the changed
    /// rectangle for recompositing. The layer's pixels are untouched.
    pub fn paint_layer_mask(&mut self, id: LayerId, from: Pos2, to: Pos2, dab: &BrushDab) {
        let clip = self.selection.as_ref().map(Selection::clip);
        let Some(layer) = self.stack.layer_mut(id) else {
            return;
        };
        layer.paint_mask_segment(from, to, dab, clip);

        if let Some(region) = segment_bounds(from, to, dab.radius, self.width, self.height) {
            self.stack.invalidate(region);
        }
        self.modified = true;
    }

    // ---- selection -----------------------------------------------------

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    pub fn select_all(&mut self) {
        self.selection = Some(Selection::whole(self.width, self.height));
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// The rectangle every edit is clipped to, if any.
    pub fn clip(&self) -> Option<PixelRegion> {
        self.selection.as_ref().map(|selection| selection.rect())
    }

    /// Paint a brush segment into one layer and mark the changed rectangle for
    /// recompositing.
    pub fn paint_layer(&mut self, id: LayerId, from: Pos2, to: Pos2, dab: &BrushDab) {
        let clip = self.selection.as_ref().map(Selection::clip);
        let locked = self.stack.layer(id).is_some_and(Layer::alpha_locked);
        let Some(surface) = self.stack.surface_mut(id) else {
            return;
        };
        if locked {
            surface.paint_segment_locked(from, to, dab, clip);
        } else {
            surface.paint_segment_clipped(from, to, dab, clip);
        }

        if let Some(region) = segment_bounds(from, to, dab.radius, self.width, self.height) {
            self.stack.invalidate(region);
        }
        self.modified = true;
    }

    /// Paint the selection itself with a brush segment.
    ///
    /// This is what makes the Quick Mask editable: the brush adds coverage to
    /// the selection and the eraser takes it away. No layer is touched, so this
    /// works even when the active entry is a group.
    pub fn paint_selection(&mut self, from: Pos2, to: Pos2, dab: &BrushDab) -> bool {
        let (width, height) = (self.width, self.height);
        let Some(selection) = self.selection.as_mut() else {
            return false;
        };
        selection.paint_segment(from, to, dab, width, height)
    }

    /// Rebuild the selection after Quick Mask painting, clearing it when the
    /// stroke removed the last of the coverage.
    ///
    /// Returns `true` when something is still selected.
    pub fn finish_selection_paint(&mut self) -> bool {
        let Some(selection) = self.selection.as_mut() else {
            return false;
        };
        if selection.finish_paint() {
            true
        } else {
            self.selection = None;
            false
        }
    }

    /// Add an empty layer above the active entry (inside the active group, if
    /// any); returns its row index and a snapshot for the undo entry.
    pub fn layer_add_above_active(&mut self) -> (usize, Layer) {
        let result = self.stack.insert_layer_above_active();
        self.modified = true;
        result
    }

    /// Wrap the active layer in a new group.
    pub fn group_add_around_active(&mut self) -> Option<(usize, Entry)> {
        let result = self.stack.insert_group_around_active();
        if result.is_some() {
            self.modified = true;
        }
        result
    }

    /// Remove the entry at `index`; a group takes its children with it.
    pub fn remove_entries_at(&mut self, index: usize) -> Option<Vec<Entry>> {
        let removed = self.stack.remove_block(index)?;
        self.modified = true;
        Some(removed)
    }

    /// Insert entries back at `index` (undo of a delete).
    pub fn insert_entries_at(&mut self, index: usize, entries: Vec<Entry>) {
        self.stack.insert_block(index, entries);
        self.modified = true;
    }

    /// Replace the whole stack (undo/redo of a flatten).
    pub fn replace_entries(&mut self, entries: Vec<Entry>, active: usize) {
        self.stack.replace(entries, active);
        self.modified = true;
    }

    /// Copy the layer at `index` above itself and select the copy.
    pub fn layer_duplicate(&mut self, index: usize) -> Option<(usize, Layer)> {
        let result = self.stack.duplicate_layer(index)?;
        self.modified = true;
        Some(result)
    }

    /// Bake the layer at `index` into its sibling below.
    pub fn layer_merge_down(&mut self, index: usize) -> Option<MergedLayer> {
        let merged = self.stack.merge_down(index)?;
        self.modified = true;
        Some(merged)
    }

    /// Re-run a merge for redo; the merged-away layer is handed back to history.
    pub fn layer_merge_redo(&mut self, index: usize) -> Option<Layer> {
        let removed = self.stack.merge_redo(index)?;
        self.modified = true;
        Some(removed)
    }

    /// Place decoded image pixels on a new layer above the active entry.
    ///
    /// Images larger than the canvas are cropped to fit; smaller ones sit at
    /// the top-left. Returns the new row index and a snapshot for undo.
    pub fn add_image_layer(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        pixels: &[Color32],
    ) -> Option<(usize, Layer)> {
        if width == 0 || height == 0 || pixels.len() != width as usize * height as usize {
            return None;
        }

        let (index, inserted) = self.stack.insert_layer_above_active();
        let id = inserted.id();

        let region = PixelRegion {
            x: 0,
            y: 0,
            width: width.min(self.width),
            height: height.min(self.height),
        };
        let mut buffer = Vec::with_capacity(region.pixel_count());
        for row in 0..region.height {
            let start = row as usize * width as usize;
            buffer.extend_from_slice(&pixels[start..start + region.width as usize]);
        }
        self.stack.surface_mut(id)?.write_region(region, &buffer);

        let mut props = self.layer_props(id)?;
        props.name = format!("{name} (imported)");
        self.set_layer_props(id, props);

        Some((index, self.layer(id)?.clone()))
    }

    /// Bake every visible layer into a single layer.
    pub fn layer_flatten(&mut self) -> Option<FlattenedStack> {
        let flattened = self.stack.flatten()?;
        self.modified = true;
        Some(flattened)
    }

    /// Move the block at `from` so that it starts at `to` after removal.
    pub fn layer_move(&mut self, from: usize, to: usize) {
        self.stack.move_block(from, to);
        self.modified = true;
    }

    /// Group whose block contains insertion slot `slot` (drag and drop).
    pub fn group_at_slot(&self, slot: usize) -> Option<LayerId> {
        self.stack.group_at_slot(slot)
    }

    /// Move the block at `from` so it starts at `to` after removal, putting a
    /// single layer into `group` (`None` = top level).
    ///
    /// Returns the block's new row index; `None` when the move is refused.
    pub fn layer_move_into(
        &mut self,
        from: usize,
        to: usize,
        group: Option<LayerId>,
    ) -> Option<usize> {
        let moved = self.stack.move_block_into(from, to, group);
        if moved.is_some() {
            self.modified = true;
        }
        moved
    }

    /// Move the active entry among its siblings; false when it cannot move.
    pub fn move_active_by(&mut self, offset: i32) -> bool {
        let moved = self.stack.move_active_by(offset);
        if moved {
            self.modified = true;
        }
        moved
    }

    pub fn layer_props(&self, id: LayerId) -> Option<LayerProps> {
        self.stack.props(id)
    }

    pub fn set_layer_props(&mut self, id: LayerId, props: LayerProps) {
        self.stack.set_props(id, props);
        self.modified = true;
    }

    pub fn group_props(&self, id: LayerId) -> Option<GroupProps> {
        self.stack.group_props(id)
    }

    pub fn set_group_props(&mut self, id: LayerId, props: GroupProps) {
        self.stack.set_group_props(id, props);
        self.modified = true;
    }

    pub fn set_group_collapsed(&mut self, id: LayerId, collapsed: bool) {
        self.stack.set_group_collapsed(id, collapsed);
    }

    pub fn group_is_collapsed(&self, id: LayerId) -> bool {
        self.stack
            .group(id)
            .map(|group| group.collapsed())
            .unwrap_or(false)
    }

    /// Entries owned by the block starting at `index` (a group brings its
    /// children, a layer is one row).
    pub fn block_len(&self, index: usize) -> usize {
        self.stack.block_len(index)
    }

    /// Flip or rotate the whole document.
    ///
    /// Every layer is permuted in place, so the transform is lossless and the
    /// inverse restores the document exactly. A quarter turn swaps the canvas
    /// width and height.
    ///
    /// A selection travels with the artwork: it is re-described in the
    /// transformed coordinates, so flipping or turning a document keeps
    /// selecting the same pixels. Undo applies the inverse transform, which
    /// carries the selection back with it.
    pub fn transform(&mut self, transform: Transform) {
        let (old_width, old_height) = (self.width, self.height);
        self.stack.transform(transform);
        self.width = self.stack.width();
        self.height = self.stack.height();
        self.selection = self
            .selection
            .as_ref()
            .and_then(|selection| selection.transformed(transform, old_width, old_height));
        self.modified = true;
    }

    /// Resize, pad or crop the page without resampling.
    ///
    /// `offset` is where the old origin lands in the new page (see
    /// [`Anchor::offset`]). Growing adds transparent margin, shrinking discards
    /// the pixels outside the page. The selection is cleared because it is
    /// described in the old coordinates.
    pub fn resize_with_offset(&mut self, width: u32, height: u32, offset: (i32, i32)) {
        self.stack.resize(width, height, offset);
        self.width = width;
        self.height = height;
        self.selection = None;
        self.modified = true;
    }

    /// Crop the page to a pixel region, discarding everything outside it.
    pub fn crop_to(&mut self, region: PixelRegion) {
        let region = clamp_region(region, self.width, self.height);
        if region.is_empty() {
            return;
        }
        self.resize_with_offset(
            region.width,
            region.height,
            (-(region.x as i32), -(region.y as i32)),
        );
    }

    /// Bounding box of everything visible, or `None` when the layers are empty.
    ///
    /// Used by "trim transparent edges": the document background is ignored,
    /// so a white canvas still crops to its painted area.
    pub fn visible_bounds(&mut self) -> Option<PixelRegion> {
        self.composite();
        self.stack.cached_composite().opaque_bounds()
    }

    /// Every entry plus the active row, for a resize snapshot.
    pub fn snapshot_entries(&self) -> (Vec<Entry>, usize) {
        (self.stack.entries().to_vec(), self.stack.active_index())
    }

    /// Put a snapshot back, including the page size it was made on.
    pub fn restore_entries(
        &mut self,
        entries: Vec<Entry>,
        active: usize,
        width: u32,
        height: u32,
    ) {
        self.stack.replace_document(entries, active, width, height);
        self.width = width;
        self.height = height;
        self.selection = None;
        self.modified = true;
    }

    /// Mark part (or all) of the composite for recomputing.
    pub fn invalidate_composite(&mut self, region: Option<crate::engine::PixelRegion>) {
        match region {
            Some(region) => self.stack.invalidate(region),
            None => self.stack.invalidate_all(),
        }
    }

    /// Colour the artist sees at a canvas pixel: the composite over the
    /// background. Used by the fill tool, which fills what is on screen.
    pub fn visible_sample(&self, x: u32, y: u32) -> Color32 {
        let layer = self.stack.cached_composite().pixel(x, y);
        match self.background.fill() {
            Some(background) => composite_pixel(background, layer, BlendMode::Normal, 1.0),
            None => layer,
        }
    }

    /// Flood-fill mask from `at`, sampled on the visible image and clipped to
    /// the selection.
    ///
    /// The caller records the region for undo, then calls [`Self::apply_fill`].
    pub fn fill_mask_at(&mut self, at: Pos2, tolerance: u8) -> Option<crate::engine::FillMask> {
        if !(at.x.is_finite() && at.y.is_finite()) || at.x < 0.0 || at.y < 0.0 {
            return None;
        }
        let (x, y) = (at.x as u32, at.y as u32);
        if x >= self.width || y >= self.height {
            return None;
        }

        // Sampling the visible image needs the cached composite to be current.
        self.composite();

        let limit = self.clip().unwrap_or(PixelRegion {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        });

        let mask = {
            let composite = self.stack.cached_composite();
            let background = self.background;
            flood_mask(
                |sx, sy| {
                    let layer = composite.pixel(sx, sy);
                    match background.fill() {
                        Some(background) => {
                            composite_pixel(background, layer, BlendMode::Normal, 1.0)
                        }
                        None => layer,
                    }
                },
                (x, y),
                tolerance,
                self.width,
                self.height,
                Some(limit),
            )
        };

        let mut mask = mask?;
        if let Some(selection) = &self.selection {
            if selection.is_feathered() {
                for row in 0..mask.bounds.height {
                    for col in 0..mask.bounds.width {
                        let index = row as usize * mask.bounds.width as usize + col as usize;
                        if mask.pixels[index]
                            && selection.coverage(mask.bounds.x + col, mask.bounds.y + row) < 0.5
                        {
                            mask.pixels[index] = false;
                        }
                    }
                }
                if !mask.pixels.iter().any(|filled| *filled) {
                    return None;
                }
            }
        }

        Some(mask)
    }

    /// Magic wand selection from `at`: the connected area of similar colour on
    /// the visible image, inside the current selection.
    pub fn wand_selection_at(
        &mut self,
        at: Pos2,
        tolerance: u8,
        feather: f32,
    ) -> Option<Selection> {
        let mask = self.fill_mask_at(at, tolerance)?;
        Selection::magic_wand(&mask, feather)
    }

    /// Swap the current selection for everything outside it.
    pub fn invert_selection(&mut self) -> Option<Selection> {
        let selection = self.selection.clone()?;
        let inverted = selection.inverted(self.width, self.height);
        self.selection = inverted.clone();
        inverted
    }

    /// Write a fill mask into one layer with a solid colour.
    pub fn apply_fill(
        &mut self,
        id: LayerId,
        mask: &crate::engine::FillMask,
        colour: Color32,
    ) -> Option<PixelRegion> {
        let region = mask.bounds;
        let locked = self.stack.layer(id).is_some_and(Layer::alpha_locked);
        {
            let surface = self.stack.surface_mut(id)?;
            let mut pixels = Vec::with_capacity(region.pixel_count());
            for row in 0..region.height {
                for col in 0..region.width {
                    let index = row as usize * region.width as usize + col as usize;
                    let destination = surface.pixel(region.x + col, region.y + row);
                    if mask.pixels[index] {
                        pixels.push(if locked {
                            paint_locked_pixel(destination, colour, 1.0)
                        } else {
                            colour
                        });
                    } else {
                        pixels.push(destination);
                    }
                }
            }
            surface.write_region(region, &pixels);
        }

        self.stack.invalidate(region);
        self.modified = true;
        Some(region)
    }

    /// Fill the current selection with a solid colour, honouring feathering.
    pub fn fill_selection(&mut self, id: LayerId, colour: Color32) -> Option<PixelRegion> {
        let selection = self.selection.clone()?;
        let region = selection.rect();
        let locked = self.stack.layer(id).is_some_and(Layer::alpha_locked);

        {
            let surface = self.stack.surface_mut(id)?;
            let mut pixels = Vec::with_capacity(region.pixel_count());
            for row in 0..region.height {
                for col in 0..region.width {
                    let x = region.x + col;
                    let y = region.y + row;
                    let destination = surface.pixel(x, y);
                    let coverage = selection.coverage(x, y);
                    if coverage <= 0.0 {
                        pixels.push(destination);
                    } else if locked {
                        pixels.push(paint_locked_pixel(destination, colour, coverage));
                    } else {
                        pixels.push(composite_pixel(
                            destination,
                            colour,
                            BlendMode::Normal,
                            coverage,
                        ));
                    }
                }
            }
            surface.write_region(region, &pixels);
        }

        self.stack.invalidate(region);
        self.modified = true;
        Some(region)
    }

    /// Region a shape would touch, so the caller can record it before drawing.
    pub fn shape_region(
        &self,
        kind: ShapeKind,
        from: Pos2,
        to: Pos2,
        radius: f32,
    ) -> Option<PixelRegion> {
        let points = super::shape::shape_path(kind, from, to);
        path_bounds(&points, radius, self.width, self.height, self.clip())
    }

    /// Paint a shape outline into the active layer, clipped to the selection.
    ///
    /// Returns the region touched, so the caller can record it for undo.
    pub fn paint_shape(
        &mut self,
        kind: ShapeKind,
        from: Pos2,
        to: Pos2,
        dab: &BrushDab,
    ) -> Option<PixelRegion> {
        let id = self.active_layer_id()?;
        let clip = self.selection.as_ref().map(Selection::clip);
        let locked = self.stack.layer(id).is_some_and(Layer::alpha_locked);
        let points = super::shape::shape_path(kind, from, to);
        let region = path_bounds(&points, dab.radius, self.width, self.height, self.clip())?;

        {
            let surface = self.stack.surface_mut(id)?;
            for pair in points.windows(2) {
                if locked {
                    surface.paint_segment_locked(pair[0], pair[1], dab, clip);
                } else {
                    surface.paint_segment_clipped(pair[0], pair[1], dab, clip);
                }
            }
        }

        self.stack.invalidate(region);
        self.modified = true;
        Some(region)
    }

    /// The composited image for display (layers over the background's absence).
    pub fn composite(&mut self) -> &Surface {
        self.stack.composite()
    }

    /// Mutable composite, so the renderer can mark the uploaded region clean.
    pub fn composite_mut(&mut self) -> &mut Surface {
        self.stack.composite_mut()
    }

    /// Sample the visible composite, for the eyedropper.
    pub fn sample(&mut self, at: Pos2) -> Option<Color32> {
        self.stack.composite().sample(at)
    }

    /// True when there are unsaved changes.
    pub fn modified(&self) -> bool {
        self.modified
    }

    // ---- geometry and reporting ---------------------------------------

    /// Pixel size as a vector, handy for viewport maths.
    pub fn size_vec(&self) -> Vec2 {
        Vec2::new(self.width as f32, self.height as f32)
    }

    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }

    /// e.g. `"1920 × 1080 px"`.
    pub fn dimensions_label(&self) -> String {
        format!("{} × {} px", self.width, self.height)
    }

    /// e.g. `"1920 × 1080 px · 72 dpi"`, for the status bar and title.
    pub fn resolution_label(&self) -> String {
        format!("{} × {} px · {}", self.width, self.height, self.dpi_label())
    }

    /// An added layer named like a sketch layer, for documents created with
    /// "start with a sketch layer" on.
    pub fn add_sketch_layer(&mut self) {
        let (_, layer) = self.layer_add_above_active();
        let id = layer.id();
        if let Some(mut props) = self.layer_props(id) {
            props.name = String::from("Sketch");
            self.set_layer_props(id, props);
        }
    }

    /// Rough memory cost of one RGBA surface, in mebibytes.
    pub fn layer_memory_mb(&self) -> f32 {
        let bytes = self.width as f64 * self.height as f64 * 4.0;
        (bytes / (1024.0 * 1024.0)) as f32
    }

    /// All layer surfaces plus the composite.
    pub fn total_memory_mb(&self) -> f32 {
        (self.stack.memory_bytes() as f64 / (1024.0 * 1024.0)) as f32
    }
}

//! Undo/redo for painting, layer and group operations.
//!
//! Pixel edits store only the 64×64 tiles they touched, so memory tracks painted
//! area rather than canvas size. Structural edits store the smallest thing that
//! can restore them: the entries that were removed (a group brings its children),
//! or a before/after snapshot of the properties that changed.

use crate::egui::{Color32, Pos2};
use crate::engine::{segment_bounds, Canvas, PixelRegion, Surface};
use crate::layers::{
    Entry, FlattenedStack, GroupProps, Layer, LayerId, LayerProps, MergedLayer,
};

/// Edge length of one history tile, in pixels.
pub const TILE: u32 = 64;

/// Default cap for all retained edits (before + after pixels).
pub const DEFAULT_MEMORY_LIMIT: usize = 256 * 1024 * 1024;

/// The pixels of one tile on both sides of a stroke.
pub struct TilePatch {
    pub region: PixelRegion,
    pub before: Vec<Color32>,
    pub after: Vec<Color32>,
}

impl TilePatch {
    fn memory_bytes(&self) -> usize {
        (self.before.len() + self.after.len()) * std::mem::size_of::<Color32>()
    }
}

/// What an edit changed.
pub enum Change {
    /// Pixels inside one layer.
    Pixels {
        layer: LayerId,
        patches: Vec<TilePatch>,
    },
    /// A layer's name, visibility, opacity or blend mode.
    Props {
        layer: LayerId,
        before: LayerProps,
        after: LayerProps,
    },
    /// A group's name, visibility or opacity.
    GroupProps {
        group: LayerId,
        before: GroupProps,
        after: GroupProps,
    },
    /// Entries were inserted at `index`.
    Inserted { index: usize, entries: Vec<Entry> },
    /// Entries were removed from `index` (a group takes its children).
    Removed { index: usize, entries: Vec<Entry> },
    /// A block moved within the stack.
    ///
    /// `to` is the block's row index after the move. The group pair records a
    /// drag into or out of a folder; both are `None` for a plain reorder.
    Moved {
        from: usize,
        to: usize,
        group_before: Option<LayerId>,
        group_after: Option<LayerId>,
    },
    /// Every entry was replaced by a single flattened layer.
    ///
    /// The flattened layer is not stored: redo recomputes it from `entries`,
    /// which costs one canvas image instead of two.
    Flattened {
        entries: Vec<Entry>,
        active: usize,
    },
    /// A layer was baked into the one below and removed.
    ///
    /// `removed` is `None` while the merge is undone, because the layer then
    /// lives in the canvas. The layer is moved between the two, never cloned.
    Merged {
        index: usize,
        removed: Option<Layer>,
        target: LayerId,
        /// The part of the target the merge changed (empty for a blank layer).
        region: PixelRegion,
        target_before: Vec<Color32>,
    },
    /// The whole document was flipped or rotated.
    ///
    /// No pixels are stored: every transform is a lossless permutation, so the
    /// opposite transform restores the document exactly and undo costs nothing.
    Transformed { transform: crate::engine::Transform },
    /// Pixels inside one layer's mask.
    MaskPixels {
        layer: LayerId,
        patches: Vec<TilePatch>,
    },
    /// A mask was added to a layer. A fresh mask is uniform, so redo rebuilds it
    /// instead of storing a canvas-sized copy.
    MaskAdded { layer: LayerId, reveal: bool },
    /// A mask was removed from a layer. The mask itself moves between the
    /// canvas and this entry, never a copy, so undo costs no extra memory while
    /// it is undone.
    MaskRemoved {
        layer: LayerId,
        mask: Option<Surface>,
    },
}

impl Change {
    fn memory_bytes(&self) -> usize {
        let pixel = std::mem::size_of::<Color32>();
        match self {
            Self::Pixels { patches, .. } => patches.iter().map(TilePatch::memory_bytes).sum(),
            Self::Props { .. } => std::mem::size_of::<LayerProps>(),
            Self::GroupProps { .. } => std::mem::size_of::<GroupProps>(),
            Self::Inserted { entries, .. }
            | Self::Removed { entries, .. }
            | Self::Flattened { entries, .. } => entries.iter().map(Entry::memory_bytes).sum(),
            Self::Moved { .. } | Self::Transformed { .. } | Self::MaskAdded { .. } => 0,
            Self::MaskPixels { patches, .. } => {
                patches.iter().map(TilePatch::memory_bytes).sum()
            }
            Self::MaskRemoved { mask, .. } => {
                mask.as_ref().map_or(0, |mask| mask.pixels().len() * pixel)
            }
            // Only what is actually retained is counted: while an edit is
            // undone, the layer belongs to the canvas and history holds no
            // copy of it.
            Self::Merged {
                removed,
                target_before,
                ..
            } => {
                removed
                    .as_ref()
                    .map_or(0, |layer| layer.surface().pixels().len() * pixel)
                    + target_before.len() * pixel
            }
        }
    }
}

/// One reversible edit.
pub struct Edit {
    label: &'static str,
    change: Change,
}

impl Edit {
    pub fn pixels(layer: LayerId, patches: Vec<TilePatch>, label: &'static str) -> Self {
        Self {
            label,
            change: Change::Pixels { layer, patches },
        }
    }

    pub fn props(layer: LayerId, before: LayerProps, after: LayerProps, label: &'static str) -> Self {
        Self {
            label,
            change: Change::Props {
                layer,
                before,
                after,
            },
        }
    }

    pub fn group_props(
        group: LayerId,
        before: GroupProps,
        after: GroupProps,
        label: &'static str,
    ) -> Self {
        Self {
            label,
            change: Change::GroupProps {
                group,
                before,
                after,
            },
        }
    }

    /// One entry inserted at `index` (a layer, or a group wrapping a layer).
    pub fn inserted(index: usize, entry: Entry, label: &'static str) -> Self {
        Self {
            label,
            change: Change::Inserted {
                index,
                entries: vec![entry],
            },
        }
    }

    /// A block of entries removed from `index`.
    pub fn removed(index: usize, entries: Vec<Entry>, label: &'static str) -> Self {
        Self {
            label,
            change: Change::Removed { index, entries },
        }
    }

    pub fn moved(from: usize, to: usize, label: &'static str) -> Self {
        Self::moved_into(from, to, None, None, label)
    }

    /// A move that also changes which group the row belongs to.
    pub fn moved_into(
        from: usize,
        to: usize,
        group_before: Option<LayerId>,
        group_after: Option<LayerId>,
        label: &'static str,
    ) -> Self {
        Self {
            label,
            change: Change::Moved {
                from,
                to,
                group_before,
                group_after,
            },
        }
    }

    pub fn flattened(flattened: FlattenedStack, label: &'static str) -> Self {
        Self {
            label,
            change: Change::Flattened {
                entries: flattened.entries,
                active: flattened.active,
            },
        }
    }

    pub fn merged(merged: MergedLayer, label: &'static str) -> Self {
        Self {
            label,
            change: Change::Merged {
                index: merged.index,
                removed: Some(merged.removed),
                target: merged.target,
                region: merged.region,
                target_before: merged.target_before,
            },
        }
    }

    /// Pixels of one layer's mask changed.
    pub fn mask_pixels(layer: LayerId, patches: Vec<TilePatch>, label: &'static str) -> Self {
        Self {
            label,
            change: Change::MaskPixels { layer, patches },
        }
    }

    /// A mask was added to a layer.
    pub fn mask_added(layer: LayerId, reveal: bool) -> Self {
        Self {
            label: "add layer mask",
            change: Change::MaskAdded { layer, reveal },
        }
    }

    /// A mask was removed from a layer.
    pub fn mask_removed(layer: LayerId, mask: Surface) -> Self {
        Self {
            label: "delete layer mask",
            change: Change::MaskRemoved {
                layer,
                mask: Some(mask),
            },
        }
    }

    /// A flip or rotation of the whole document.
    pub fn transformed(transform: crate::engine::Transform) -> Self {
        Self {
            label: transform.undo_label(),
            change: Change::Transformed { transform },
        }
    }

    pub fn label(&self) -> &'static str {
        self.label
    }

    pub fn memory_bytes(&self) -> usize {
        self.change.memory_bytes()
    }

    /// Reverses (or re-applies) this edit.
    ///
    /// Structural edits move layers and entries between the canvas and the
    /// history entry instead of cloning them. A full canvas layer is megabytes,
    /// and undo/redo of a merge or flatten would otherwise copy every layer of
    /// the document twice.
    fn apply(&mut self, canvas: &mut Canvas, undo: bool) {
        match &mut self.change {
            Change::Pixels { layer, patches } => {
                for patch in patches.iter().rev() {
                    if let Some(surface) = canvas.layer_surface_mut(*layer) {
                        let pixels = if undo { &patch.before } else { &patch.after };
                        surface.write_region(patch.region, pixels);
                    }
                    canvas.invalidate_composite(Some(patch.region));
                }
            }
            Change::Props {
                layer,
                before,
                after,
            } => {
                let props = if undo { before } else { after };
                canvas.set_layer_props(*layer, props.clone());
            }
            Change::GroupProps {
                group,
                before,
                after,
            } => {
                let props = if undo { before } else { after };
                canvas.set_group_props(*group, props.clone());
            }
            Change::Inserted { index, entries } => {
                if undo {
                    if let Some(removed) = canvas.remove_entries_at(*index) {
                        *entries = removed;
                    }
                } else {
                    canvas.insert_entries_at(*index, std::mem::take(entries));
                }
            }
            Change::Removed { index, entries } => {
                if undo {
                    if !entries.is_empty() {
                        canvas.insert_entries_at(*index, std::mem::take(entries));
                    }
                } else if let Some(removed) = canvas.remove_entries_at(*index) {
                    *entries = removed;
                }
            }
            Change::Moved {
                from,
                to,
                group_before,
                group_after,
            } => {
                let (start, dest, group) = if undo {
                    (*to, *from, *group_before)
                } else {
                    (*from, *to, *group_after)
                };
                // A plain reorder keeps every layer in its group, which is what
                // the arrow buttons do. Only a drag records group changes.
                if group_before.is_none() && group_after.is_none() {
                    canvas.layer_move(start, dest);
                } else if canvas.layer_move_into(start, dest, group).is_none() {
                    canvas.layer_move(start, dest);
                }
            }
            Change::Transformed { transform } => {
                canvas.transform(if undo {
                    transform.inverse()
                } else {
                    *transform
                });
            }
            Change::MaskPixels { layer, patches } => {
                for patch in patches.iter().rev() {
                    if let Some(mask) = canvas.layer_mask_mut(*layer) {
                        let pixels = if undo { &patch.before } else { &patch.after };
                        mask.write_region(patch.region, pixels);
                    }
                    canvas.invalidate_composite(Some(patch.region));
                }
            }
            Change::MaskAdded { layer, reveal } => {
                if undo {
                    canvas.set_layer_mask(*layer, None);
                } else {
                    canvas.add_layer_mask(*layer, *reveal);
                }
            }
            Change::MaskRemoved { layer, mask } => {
                if undo {
                    if let Some(mask) = mask.take() {
                        canvas.set_layer_mask(*layer, Some(mask));
                    }
                } else {
                    *mask = canvas.set_layer_mask(*layer, None);
                }
            }
            Change::Flattened { entries, active } => {
                if undo {
                    if !entries.is_empty() {
                        canvas.replace_entries(std::mem::take(entries), *active);
                    }
                } else if let Some(flattened) = canvas.layer_flatten() {
                    // Recomputing the flat image from the restored entries is
                    // cheaper in memory than storing a second canvas image.
                    *entries = flattened.entries;
                    *active = flattened.active;
                }
            }
            Change::Merged {
                index,
                removed,
                target,
                region,
                target_before,
            } => {
                if undo {
                    if !region.is_empty() {
                        if let Some(surface) = canvas.layer_surface_mut(*target) {
                            surface.write_region(*region, target_before);
                        }
                        canvas.invalidate_composite(Some(*region));
                    }
                    if let Some(layer) = removed.take() {
                        canvas.insert_entries_at(*index, vec![Entry::Layer(layer)]);
                    }
                    canvas.invalidate_composite(None);
                } else if let Some(layer) = canvas.layer_merge_redo(*index) {
                    *removed = Some(layer);
                }
            }
        }
    }

    /// Merge a follow-up property change into this edit.
    ///
    /// Dragging the opacity slider or typing a name fires many small changes;
    /// they must collapse into one undo step instead of flooding the history.
    /// Only edits of the same field on the same target merge, and only into the
    /// newest edit, so unrelated work never gets folded together: a rename after
    /// an opacity drag is its own undo step, not a passenger on it.
    fn try_merge(&mut self, incoming: &Edit) -> bool {
        match (&mut self.change, &incoming.change) {
            (
                Change::Props {
                    layer: existing,
                    after,
                    ..
                },
                Change::Props {
                    layer: new_layer,
                    after: new_after,
                    ..
                },
            ) if existing == new_layer && self.label == incoming.label => {
                *after = new_after.clone();
                self.label = incoming.label;
                true
            }
            (
                Change::GroupProps {
                    group: existing,
                    after,
                    ..
                },
                Change::GroupProps {
                    group: new_group,
                    after: new_after,
                    ..
                },
            ) if existing == new_group && self.label == incoming.label => {
                *after = new_after.clone();
                self.label = incoming.label;
                true
            }
            _ => false,
        }
    }
}

/// Collects the "before" pixels of a stroke as it is painted.
///
/// Call [`Self::record_segment`] *before* painting each segment, then
/// [`Self::finish`] once the stroke ends.
pub struct Recorder {
    patches: Vec<TilePatch>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            patches: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    /// Snapshot any tiles not yet recorded that the segment can touch.
    pub fn record_segment(&mut self, surface: &Surface, from: Pos2, to: Pos2, radius: f32) {
        let Some(bounds) = segment_bounds(from, to, radius, surface.width(), surface.height())
        else {
            return;
        };

        let mut tile_y = bounds.y / TILE * TILE;
        while tile_y < bounds.y + bounds.height {
            let mut tile_x = bounds.x / TILE * TILE;
            while tile_x < bounds.x + bounds.width {
                let already_recorded = self
                    .patches
                    .iter()
                    .any(|patch| patch.region.x == tile_x && patch.region.y == tile_y);

                if !already_recorded {
                    let region = PixelRegion {
                        x: tile_x,
                        y: tile_y,
                        width: TILE.min(surface.width() - tile_x),
                        height: TILE.min(surface.height() - tile_y),
                    };
                    if !region.is_empty() {
                        let before = surface.copy_region(region);
                        self.patches.push(TilePatch {
                            region,
                            before,
                            after: Vec::new(),
                        });
                    }
                }

                tile_x += TILE;
            }
            tile_y += TILE;
        }
    }

    /// Snapshot every tile overlapping `region` (used by fill and shapes, which
    /// change an area rather than a stroke path).
    pub fn record_region(&mut self, surface: &Surface, region: PixelRegion) {
        let mut tile_y = region.y / TILE * TILE;
        let bottom = region.y + region.height;
        while tile_y < bottom {
            let mut tile_x = region.x / TILE * TILE;
            let right = region.x + region.width;
            while tile_x < right {
                let already_recorded = self
                    .patches
                    .iter()
                    .any(|patch| patch.region.x == tile_x && patch.region.y == tile_y);

                if !already_recorded {
                    let tile = PixelRegion {
                        x: tile_x,
                        y: tile_y,
                        width: TILE.min(surface.width() - tile_x),
                        height: TILE.min(surface.height() - tile_y),
                    };
                    if !tile.is_empty() {
                        let before = surface.copy_region(tile);
                        self.patches.push(TilePatch {
                            region: tile,
                            before,
                            after: Vec::new(),
                        });
                    }
                }

                tile_x += TILE;
            }
            tile_y += TILE;
        }
    }

    /// Capture the "after" pixels and return only the tiles that actually changed.
    ///
    /// An empty result means the edit did nothing (for example a click that
    /// landed entirely off-canvas), so no undo step is created.
    pub fn finish(mut self, surface: &Surface) -> Vec<TilePatch> {
        for patch in &mut self.patches {
            patch.after = surface.copy_region(patch.region);
        }
        self.patches.retain(|patch| patch.before != patch.after);
        self.patches
    }
}

/// Undo and redo stacks with a memory cap.
#[derive(Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    memory: usize,
    limit: usize,
}

impl History {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            memory: 0,
            limit: DEFAULT_MEMORY_LIMIT,
        }
    }

    /// Override the memory cap (used by tests to exercise trimming).
    pub fn with_memory_limit(limit: usize) -> Self {
        Self {
            limit,
            ..Self::new()
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    pub fn memory_bytes(&self) -> usize {
        self.memory
    }

    /// Record a completed edit, merging into the newest one when it is a
    /// continuation of the same property change.
    pub fn push(&mut self, edit: Edit) {
        self.redo.clear();

        if let Some(top) = self.undo.last_mut() {
            if top.try_merge(&edit) {
                self.recalculate();
                return;
            }
        }

        self.undo.push(edit);
        self.recalculate();
        self.trim();
    }

    /// Undo the newest edit; returns its label when something was undone.
    pub fn undo(&mut self, canvas: &mut Canvas) -> Option<&'static str> {
        let mut edit = self.undo.pop()?;
        edit.apply(canvas, true);
        let label = edit.label();
        self.redo.push(edit);
        self.recalculate();
        Some(label)
    }

    /// Redo the newest undone edit; returns its label when something was redone.
    pub fn redo(&mut self, canvas: &mut Canvas) -> Option<&'static str> {
        let mut edit = self.redo.pop()?;
        edit.apply(canvas, false);
        let label = edit.label();
        self.undo.push(edit);
        self.recalculate();
        Some(label)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.memory = 0;
    }

    fn recalculate(&mut self) {
        self.memory = self
            .undo
            .iter()
            .chain(self.redo.iter())
            .map(Edit::memory_bytes)
            .sum();
    }

    /// Drop the oldest edits until the memory cap is respected.
    fn trim(&mut self) {
        while self.memory > self.limit && self.undo.len() > 1 {
            self.undo.remove(0);
            self.recalculate();
        }
    }
}

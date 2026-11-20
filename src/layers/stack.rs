//! The layer stack, its groups and the cached composite.
//!
//! The stack is a flat, ordered list of [`Entry`] values: paint layers and layer
//! groups. A group's children sit directly after it (one level deep), which
//! keeps reordering, undo indices and the sidebar list simple while giving the
//! artist the usual folder behaviour.
//!
//! Compositing every visible layer for the whole canvas on every frame would
//! cost megabytes of work per pointer move, so the composite is cached and only
//! the region that changed since the last frame is recomputed.

use crate::egui::Color32;
use crate::engine::{PixelRegion, Surface};

use super::group::{Group, GroupProps};
use super::layer::{composite_pixel, Layer, LayerId, LayerProps};

/// One row of the stack: a paint layer or a group of layers.
#[derive(Clone, Debug)]
pub enum Entry {
    Layer(Layer),
    Group(Group),
}

impl Entry {
    pub fn id(&self) -> LayerId {
        match self {
            Self::Layer(layer) => layer.id(),
            Self::Group(group) => group.id(),
        }
    }

    pub fn as_layer(&self) -> Option<&Layer> {
        match self {
            Self::Layer(layer) => Some(layer),
            Self::Group(_) => None,
        }
    }

    pub fn as_group(&self) -> Option<&Group> {
        match self {
            Self::Group(group) => Some(group),
            Self::Layer(_) => None,
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group(_))
    }

    /// Parent group id; groups themselves live at the top level for now.
    pub fn parent(&self) -> Option<LayerId> {
        match self {
            Self::Layer(layer) => layer.group(),
            Self::Group(_) => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Layer(layer) => layer.name(),
            Self::Group(group) => group.name(),
        }
    }

    pub fn visible(&self) -> bool {
        match self {
            Self::Layer(layer) => layer.visible(),
            Self::Group(group) => group.visible(),
        }
    }

    pub fn opacity(&self) -> f32 {
        match self {
            Self::Layer(layer) => layer.opacity(),
            Self::Group(group) => group.opacity(),
        }
    }

    pub fn memory_bytes(&self) -> usize {
        match self {
            Self::Layer(layer) => layer.surface().pixels().len() * std::mem::size_of::<Color32>(),
            Self::Group(_) => 0,
        }
    }
}

/// Ordered set of layers and groups plus the composited image used for display.
pub struct LayerStack {
    width: u32,
    height: u32,
    /// Bottom (index 0) to top.
    entries: Vec<Entry>,
    active: usize,
    next_id: LayerId,
    composite: Surface,
    dirty: Option<PixelRegion>,
}

impl LayerStack {
    /// A stack with one empty layer.
    pub fn new(width: u32, height: u32) -> Self {
        let mut stack = Self {
            width,
            height,
            entries: Vec::new(),
            active: 0,
            next_id: 0,
            composite: Surface::new(width, height),
            dirty: None,
        };

        let layer = stack.create_layer();
        stack.entries.push(Entry::Layer(layer));
        stack.invalidate_all();
        stack
    }

    /// Rebuild a stack from loaded entries (project files, tests).
    pub fn from_entries(width: u32, height: u32, entries: Vec<Entry>, active: usize) -> Self {
        let mut stack = Self {
            width,
            height,
            active: 0,
            next_id: 0,
            composite: Surface::new(width, height),
            dirty: None,
            entries,
        };
        for entry in &stack.entries {
            stack.next_id = stack.next_id.max(entry.id() + 1);
        }
        stack.active = active.min(stack.entries.len().saturating_sub(1));
        stack.invalidate_all();
        stack
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Bottom to top, groups followed by their children.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of paint layers (groups are not counted).
    pub fn layer_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.as_layer().is_some())
            .count()
    }

    pub fn active_index(&self) -> usize {
        self.active.min(self.entries.len().saturating_sub(1))
    }

    pub fn set_active_index(&mut self, index: usize) {
        if !self.entries.is_empty() {
            self.active = index.min(self.entries.len() - 1);
        }
    }

    pub fn active_entry(&self) -> &Entry {
        &self.entries[self.active_index()]
    }

    pub fn active_id(&self) -> LayerId {
        self.active_entry().id()
    }

    /// The active entry when it is a paint layer; painting needs one.
    pub fn active_layer(&self) -> Option<&Layer> {
        self.active_entry().as_layer()
    }

    pub fn active_layer_id(&self) -> Option<LayerId> {
        self.active_layer().map(Layer::id)
    }

    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.entries.iter().find_map(|entry| match entry {
            Entry::Layer(layer) if layer.id() == id => Some(layer),
            _ => None,
        })
    }

    pub fn group(&self, id: LayerId) -> Option<&Group> {
        self.entries.iter().find_map(|entry| match entry {
            Entry::Group(group) if group.id() == id => Some(group),
            _ => None,
        })
    }

    pub fn index_of(&self, id: LayerId) -> Option<usize> {
        self.entries.iter().position(|entry| entry.id() == id)
    }

    /// Parent group id of the entry at `index` (`None` for top-level entries).
    pub fn parent_of(&self, index: usize) -> Option<LayerId> {
        self.entries.get(index).and_then(Entry::parent)
    }

    pub fn surface(&self, id: LayerId) -> Option<&Surface> {
        self.layer(id).map(Layer::surface)
    }

    pub fn surface_mut(&mut self, id: LayerId) -> Option<&mut Surface> {
        self.entries
            .iter_mut()
            .find_map(|entry| match entry {
                Entry::Layer(layer) if layer.id() == id => Some(layer.surface_mut()),
                _ => None,
            })
    }

    /// Allocate a new layer (unique id, transparent surface) without inserting it.
    pub fn create_layer(&mut self) -> Layer {
        let id = self.next_id;
        self.next_id += 1;
        Layer::new(id, format!("Layer {}", id + 1), self.width, self.height)
    }

    /// Add an empty layer above the active entry.
    ///
    /// When the active entry is a group (or a layer inside one) the new layer
    /// joins that group, so the "add layer" button keeps working inside folders.
    pub fn insert_layer_above_active(&mut self) -> (usize, Layer) {
        let index = self.active_index();
        let (parent, insert_at) = match self.entries.get(index) {
            Some(Entry::Layer(layer)) => (layer.group(), index + 1),
            Some(Entry::Group(group)) => (Some(group.id()), index + 1),
            None => (None, 0),
        };

        let mut layer = self.create_layer();
        layer.set_group(parent);
        let snapshot = layer.clone();
        self.insert_block(insert_at, vec![Entry::Layer(layer)]);
        (insert_at, snapshot)
    }

    /// Wrap the active layer in a new group. Nested groups are not supported, so
    /// a layer that is already inside a group is refused.
    pub fn insert_group_around_active(&mut self) -> Option<(usize, Entry)> {
        let index = self.active_index();
        match self.entries.get(index) {
            Some(Entry::Layer(layer)) if layer.group().is_none() => {}
            _ => return None,
        }

        let id = self.next_id;
        self.next_id += 1;
        let group = Group::new(id, format!("Group {}", id + 1));
        let snapshot = Entry::Group(group.clone());

        self.entries.insert(index, Entry::Group(group));
        if let Some(Entry::Layer(layer)) = self.entries.get_mut(index + 1) {
            layer.set_group(Some(id));
        }
        self.active = index;
        self.invalidate_all();
        Some((index, snapshot))
    }

    /// Insert whole entries at `index` (undo of a delete restores a block).
    pub fn insert_block(&mut self, index: usize, entries: Vec<Entry>) {
        if entries.is_empty() {
            return;
        }
        for entry in &entries {
            self.next_id = self.next_id.max(entry.id() + 1);
        }

        let index = index.min(self.entries.len());
        self.entries.splice(index..index, entries);
        self.active = index;
        self.invalidate_all();
    }

    /// Length of the block starting at `index`: a group plus its children, or
    /// one entry for a layer.
    pub fn block_len(&self, index: usize) -> usize {
        match self.entries.get(index) {
            Some(Entry::Group(group)) => 1 + self.child_count(group.id()),
            Some(Entry::Layer(_)) => 1,
            None => 0,
        }
    }

    fn child_count(&self, group: LayerId) -> usize {
        let Some(start) = self.index_of(group).map(|index| index + 1) else {
            return 0;
        };

        let mut count = 0;
        while let Some(Entry::Layer(layer)) = self.entries.get(start + count) {
            if layer.group() == Some(group) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Remove the entry at `index` (a group takes its children with it).
    ///
    /// Refuses to remove the last paint layer: a document always has somewhere
    /// to paint.
    pub fn remove_block(&mut self, index: usize) -> Option<Vec<Entry>> {
        let len = self.block_len(index);
        if len == 0 {
            return None;
        }

        let removed_layers = self.entries[index..index + len]
            .iter()
            .filter(|entry| entry.as_layer().is_some())
            .count();
        if self.layer_count() <= removed_layers {
            return None;
        }

        let end = index + len;
        let block: Vec<Entry> = self.entries.drain(index..end).collect();

        self.active = if self.active >= end {
            self.active - len
        } else if self.active >= index {
            index
        } else {
            self.active
        };
        self.active = self.active.min(self.entries.len().saturating_sub(1));
        self.invalidate_all();
        Some(block)
    }

    /// Move the block at `from` so that it starts at `to` *after* removal.
    pub fn move_block(&mut self, from: usize, to: usize) {
        let len = self.block_len(from);
        if len == 0 {
            return;
        }

        let block: Vec<Entry> = self.entries.drain(from..from + len).collect();
        let to = to.min(self.entries.len());
        self.entries.splice(to..to, block);
        self.active = to;
        self.invalidate_all();
    }

    /// Group whose block contains insertion slot `slot`.
    ///
    /// Slots sit between entries. Slot `g` is the top-level position just below
    /// group `g` and slot `g + block_len` the one just above it; the slots in
    /// between fall inside the group.
    pub fn group_at_slot(&self, slot: usize) -> Option<LayerId> {
        let mut index = 0;
        while index < self.entries.len() {
            let len = self.block_len(index);
            if self.entries[index].is_group() && slot > index && slot < index + len {
                return Some(self.entries[index].id());
            }
            index += len.max(1);
        }
        None
    }

    /// Nearest slot outside a group's block, so a top-level drop never lands
    /// between a folder and its contents.
    fn top_level_slot(&self, slot: usize) -> usize {
        let mut index = 0;
        while index < self.entries.len() {
            let len = self.block_len(index);
            if self.entries[index].is_group() && slot > index && slot <= index + len {
                let end = index + len;
                return if slot - index <= end - slot { index } else { end };
            }
            index += len.max(1);
        }
        slot
    }

    /// Move the block at `from` so it starts at `dest` once the block has been
    /// taken out, and put a single layer into `group` (`None` = top level).
    ///
    /// This is what dragging a row does: a layer can join a folder or leave it,
    /// while groups themselves stay at the top level (one level of nesting).
    /// Returns the block's new row index, or `None` when the move is refused.
    pub fn move_block_into(
        &mut self,
        from: usize,
        dest: usize,
        group: Option<LayerId>,
    ) -> Option<usize> {
        let len = self.block_len(from);
        if len == 0 {
            return None;
        }
        // Groups hold layers, never other groups: one level deep.
        if self.entries[from].is_group() && group.is_some() {
            return None;
        }
        if group.is_some_and(|id| self.group(id).is_none()) {
            return None;
        }

        let mut block: Vec<Entry> = self.entries.drain(from..from + len).collect();
        let mut insert = dest.min(self.entries.len());

        match group {
            Some(id) => match self.index_of(id) {
                Some(start) => {
                    // Between the folder row and its children.
                    let end = start + 1 + self.child_count(id);
                    insert = insert.clamp(start + 1, end);
                }
                None => {
                    // Cannot happen while the group is borrowed, but a lost
                    // block would be lost artwork: put it back instead.
                    let at = from.min(self.entries.len());
                    self.entries.splice(at..at, block);
                    return None;
                }
            },
            None => insert = self.top_level_slot(insert),
        }

        if let [Entry::Layer(layer)] = block.as_mut_slice() {
            layer.set_group(group);
        }
        self.entries.splice(insert..insert, block);
        self.active = insert;
        self.invalidate_all();
        Some(insert)
    }

    /// Move the active entry up (`1`) or down (`-1`) among its siblings.
    ///
    /// Siblings share a parent, so a layer never jumps into or out of a group
    /// with the arrow buttons.
    pub fn move_active_by(&mut self, offset: i32) -> bool {
        if offset == 0 {
            return false;
        }
        let index = self.active_index();
        let parent = self.parent_of(index);
        let siblings: Vec<usize> = (0..self.entries.len())
            .filter(|&i| self.parent_of(i) == parent)
            .collect();
        let Some(position) = siblings.iter().position(|&i| i == index) else {
            return false;
        };

        let len = self.block_len(index);
        let steps = offset.unsigned_abs() as usize;

        let to = if offset > 0 {
            let Some(&next) = siblings.get(position + steps) else {
                return false;
            };
            // After removing our block the sibling sits `len` slots lower, and
            // we want to land just above it.
            next - len + self.block_len(next)
        } else {
            let Some(&previous) = position
                .checked_sub(steps)
                .and_then(|p| siblings.get(p))
            else {
                return false;
            };
            previous
        };

        self.move_block(index, to);
        true
    }

    /// Copy a layer (pixels and properties) directly above the original.
    pub fn duplicate_layer(&mut self, index: usize) -> Option<(usize, Layer)> {
        let source = self.entries.get(index)?.as_layer()?;

        let mut props = source.props();
        props.name = format!("{} copy", props.name);
        let pixels = source.surface().pixels().to_vec();
        let group = source.group();

        let mut layer = Layer::new(self.next_id, props.name.clone(), self.width, self.height);
        self.next_id += 1;
        layer
            .surface_mut()
            .write_region(full_region(self.width, self.height), &pixels);
        layer.set_props(props);
        layer.set_group(group);

        let snapshot = layer.clone();
        self.insert_block(index + 1, vec![Entry::Layer(layer)]);
        Some((index + 1, snapshot))
    }

    /// Bake the layer at `index` into its sibling directly below.
    pub fn merge_down(&mut self, index: usize) -> Option<MergedLayer> {
        self.merge_into_below(index)
    }

    /// Re-run a merge for redo.
    ///
    /// The merged-away layer is handed back so the history entry can keep it
    /// for the next undo; nothing is cloned on the way.
    pub fn merge_redo(&mut self, index: usize) -> Option<Layer> {
        self.merge_into_below(index).map(|merged| merged.removed)
    }

    fn merge_into_below(&mut self, index: usize) -> Option<MergedLayer> {
        if index == 0 || index >= self.entries.len() {
            return None;
        }

        let target_index = index - 1;
        let target_layer = self.entries.get(target_index)?.as_layer()?;
        let target = target_layer.id();
        let target_group = target_layer.group();

        // A group is not something to merge into the layer below.
        self.entries.get(index)?.as_layer()?;

        // Take the layer out rather than cloning it: it is either merged away
        // or put straight back, and a full layer clone is the expensive part.
        let source = match self.entries.remove(index) {
            Entry::Layer(layer) => layer,
            Entry::Group(_) => unreachable!("checked above"),
        };
        if source.group() != target_group {
            self.entries.insert(index, Entry::Layer(source));
            return None;
        }

        // Only pixels the source actually covers can change, so both the undo
        // snapshot and the blending work are limited to that rectangle.
        let region = source.surface().opaque_bounds().unwrap_or(PixelRegion {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });

        let mut target_before = Vec::new();
        if !region.is_empty() {
            let target_surface = self.surface_mut(target)?;
            target_before = target_surface.copy_region(region);
            let mut merged = target_before.clone();
            for row in 0..region.height {
                for col in 0..region.width {
                    let src = source.surface().pixel(region.x + col, region.y + row);
                    if src.a() == 0 {
                        continue;
                    }
                    let slot = row as usize * region.width as usize + col as usize;
                    merged[slot] =
                        composite_pixel(merged[slot], src, source.blend(), source.opacity());
                }
            }
            target_surface.write_region(region, &merged);
        }

        self.active = target_index.min(self.entries.len().saturating_sub(1));
        self.invalidate_all();

        Some(MergedLayer {
            index,
            removed: source,
            target,
            region,
            target_before,
        })
    }

    /// Bake every visible layer into one layer and drop the rest.
    ///
    /// Hidden layers (and hidden groups) are discarded, exactly as flattening
    /// behaves in other editors.
    pub fn flatten(&mut self) -> Option<FlattenedStack> {
        if self.layer_count() <= 1 {
            return None;
        }

        // Make sure the cached composite is up to date before copying it.
        let pixels = self.composite().pixels().to_vec();

        let mut layer = Layer::new(self.next_id, "Flattened", self.width, self.height);
        self.next_id += 1;
        layer
            .surface_mut()
            .write_region(full_region(self.width, self.height), &pixels);

        // The flattened layer itself is not kept in history: redo recomputes it
        // from the saved entries, so a flatten costs one canvas image, not two.
        let previous = std::mem::replace(&mut self.entries, vec![Entry::Layer(layer)]);
        let active = self.active.min(previous.len().saturating_sub(1));
        self.active = 0;
        self.invalidate_all();

        Some(FlattenedStack {
            entries: previous,
            active,
        })
    }

    /// Replace the whole stack, used by undo/redo of a flatten.
    pub fn replace(&mut self, entries: Vec<Entry>, active: usize) {
        if entries.is_empty() {
            return;
        }
        for entry in &entries {
            self.next_id = self.next_id.max(entry.id() + 1);
        }
        self.entries = entries;
        self.active = active.min(self.entries.len() - 1);
        self.invalidate_all();
    }

    pub fn props(&self, id: LayerId) -> Option<LayerProps> {
        self.layer(id).map(Layer::props)
    }

    pub fn set_props(&mut self, id: LayerId, props: LayerProps) {
        if let Some(layer) = self
            .entries
            .iter_mut()
            .find_map(|entry| match entry {
                Entry::Layer(layer) if layer.id() == id => Some(layer),
                _ => None,
            })
        {
            layer.set_props(props);
            self.invalidate_all();
        }
    }

    pub fn group_props(&self, id: LayerId) -> Option<GroupProps> {
        self.group(id).map(Group::props)
    }

    pub fn set_group_props(&mut self, id: LayerId, props: GroupProps) {
        if let Some(Entry::Group(group)) = self
            .entries
            .iter_mut()
            .find(|entry| entry.id() == id && entry.is_group())
        {
            group.set_props(props);
            self.invalidate_all();
        }
    }

    pub fn set_group_collapsed(&mut self, id: LayerId, collapsed: bool) {
        if let Some(Entry::Group(group)) = self
            .entries
            .iter_mut()
            .find(|entry| entry.id() == id && entry.is_group())
        {
            group.set_collapsed(collapsed);
        }
    }

    /// Mark a rectangle for recompositing.
    pub fn invalidate(&mut self, region: PixelRegion) {
        self.dirty = Some(match self.dirty {
            Some(existing) => union(existing, region),
            None => region,
        });
    }

    /// Mark the whole canvas for recompositing.
    pub fn invalidate_all(&mut self) {
        self.dirty = Some(PixelRegion {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        });
    }

    /// Composite of all visible layers, refreshed for anything that changed.
    pub fn composite(&mut self) -> &Surface {
        self.refresh();
        &self.composite
    }

    /// Same composite, mutable. The renderer clears the dirty region once the
    /// changed pixels have been uploaded to the GPU.
    pub fn composite_mut(&mut self) -> &mut Surface {
        self.refresh();
        &mut self.composite
    }

    /// Read-only composite; may be stale if something changed since the last
    /// [`Self::composite`] call.
    pub fn cached_composite(&self) -> &Surface {
        &self.composite
    }

    fn refresh(&mut self) {
        let Some(region) = self.dirty.take() else {
            return;
        };
        if region.is_empty() {
            return;
        }

        let mut buffer = vec![Color32::TRANSPARENT; region.pixel_count()];

        let mut index = 0;
        while index < self.entries.len() {
            match &self.entries[index] {
                Entry::Layer(layer) => {
                    // Children are drawn through their group below.
                    if layer.group().is_none() {
                        composite_entry(layer, 1.0, region, &mut buffer);
                    }
                    index += 1;
                }
                Entry::Group(group) => {
                    let group_id = group.id();
                    let visible = group.visible();
                    let inherited = group.opacity();
                    index += 1;

                    while let Some(Entry::Layer(layer)) = self.entries.get(index) {
                        if layer.group() != Some(group_id) {
                            break;
                        }
                        if visible {
                            composite_entry(layer, inherited, region, &mut buffer);
                        }
                        index += 1;
                    }
                }
            }
        }

        self.composite.write_region(region, &buffer);
    }

    /// Bytes used by all layer surfaces plus the composite.
    pub fn memory_bytes(&self) -> usize {
        let bytes_per_surface = self.width as usize * self.height as usize * 4;
        (self.layer_count() + 1) * bytes_per_surface
    }
}

/// Composite one layer into a region buffer.
fn composite_entry(
    layer: &Layer,
    inherited_opacity: f32,
    region: PixelRegion,
    buffer: &mut [Color32],
) {
    if !layer.visible() || layer.opacity() <= 0.0 || inherited_opacity <= 0.0 {
        return;
    }

    let opacity = layer.opacity() * inherited_opacity;
    let mode = layer.blend();
    let surface = layer.surface();

    for row in 0..region.height {
        let y = region.y + row;
        for col in 0..region.width {
            let x = region.x + col;
            let src = surface.pixel(x, y);
            if src.a() == 0 {
                continue;
            }
            let index = row as usize * region.width as usize + col as usize;
            buffer[index] = composite_pixel(buffer[index], src, mode, opacity);
        }
    }
}

/// The result of baking one layer into the layer below.
pub struct MergedLayer {
    /// Where the merged-away layer sat.
    pub index: usize,
    /// The layer that was removed, kept whole so undo can bring it back.
    pub removed: Layer,
    /// The layer that received the pixels.
    pub target: LayerId,
    /// How much of the target the merge touched (empty for a blank layer).
    pub region: PixelRegion,
    /// The receiving layer's pixels inside `region` before the merge.
    pub target_before: Vec<Color32>,
}

/// The outcome of flattening: the entries that were replaced, which one was
/// active, and the single layer that replaced them.
pub struct FlattenedStack {
    pub entries: Vec<Entry>,
    pub active: usize,
}

fn full_region(width: u32, height: u32) -> PixelRegion {
    PixelRegion {
        x: 0,
        y: 0,
        width,
        height,
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

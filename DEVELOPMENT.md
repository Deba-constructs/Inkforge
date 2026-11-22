# INKFORGE — development log

Living document: environment notes, milestone status, architecture decisions,
verification log and outstanding work. Updated as milestones land.

Last updated: the 1.6.0 release preparation — polygon lasso, selection combine
modes, selections that follow a document transform, undoable canvas size/crop.
Milestone 7 is still open (see §7); 1.1.0 added the full
W3C blend set and clip to layer below, 1.2.0 added a 43-brush library, live
per-layer colour adjustments and document flips/rotations, 1.3.0 added
non-destructive layer masks (project format version 2), 1.4.0 added layered
OpenRaster (`.ora`) export, 1.5.0 added 53 canvas presets, unit-aware sizes,
print metadata and guides (project format version 3), and 1.6.0 makes
selections undoable and combinable, adds the polygon lasso, canvas size/crop/trim
after creation, persistent workspace preferences and PNG resolution import.

---

## 1. Environment report

Checked at the start of the project (see §5 for what this means for the Windows
build):

| Item | Result |
| --- | --- |
| Host OS used for this work | Linux (Ubuntu 24.04, x86_64 container) — **not** Windows 11 |
| Rust | rustc 1.100.0 (2026-11-09), cargo 1.100.0, installed with rustup (minimal profile) |
| Crates.io access | Working; `eframe 0.37.0` and its dependency tree resolve and build |
| Graphics stack available here | No X11/Wayland display and no `sudo` for dev packages, so the window can be compiled but **not launched** in this environment |
| Windows build | No Windows host and no MSVC linker; the 1.5.0 executable was **cross-compiled on Linux** with `cargo zigbuild` (zig 0.13's bundled MinGW toolchain) for `x86_64-pc-windows-gnu` — see §6 |
| GUI toolkit | `eframe 0.37` (default renderer family: wgpu) with `egui 0.37` |

**Consequence, stated plainly:** the project targets Windows and builds with
`x86_64-pc-windows-msvc` on a Windows machine. This container has no Windows and
no MSVC linker, so the 1.5.0 release was cross-compiled to
`x86_64-pc-windows-gnu` with `cargo zigbuild` instead: a real 64-bit Windows GUI
executable was produced and inspected (§5, §6), but it has **never been run**,
because nothing here can execute a Windows binary. The clean-machine and artist
run (the rest of Milestone 8) still has to happen on Windows.

## 2. Milestone status

| # | Milestone | State |
| --- | --- | --- |
| 1 | Project init, Rust config, desktop window, main UI | **Done** (compiles clean; window not launched — no display here) |
| 2 | Interactive canvas, freehand drawing, brush size/colour, eraser | **Done** (compiles clean; painting verified by tests and a simulated pointer drag, not yet by eye) |
| 3 | Brush presets, stroke smoothing, opacity, better painting engine | **Done** (43 presets in six shelves that differ in size, edge softness, opacity, spacing, grain and smoothing; the stabiliser adapts to pointer speed) |
| 4 | Layer architecture and layer manager | **Done** (add/delete/reorder/rename, visibility, opacity, 17 blend modes, clip to layer below, per-layer masks, live brightness/contrast/saturation/hue per layer, layer-aware undo; verified by tests, not by eye) |
| 5 | Undo/redo, selection, fill, shapes | **Done** (undo/redo landed early; rectangle selection, flood fill and line/rectangle/ellipse shapes added) |
| 6 | PNG/JPEG import-export, native `.inkforge`, reliable save/load | **Done** (projects round-trip every layer, group and property; corrupt files are refused with a message; verified by tests, not by hand in the GUI) |
| 7 | Interface refinement, performance, testing | **In progress** — live window title, recent projects, partial GPU uploads, threaded stamping and the composite tile pyramid landed; see §7 |
| 8 | Windows release build, portable exe, ZIP, artist testing | **In progress** — 1.5.0 executable, portable folder and ZIP built and inspected; the icon and version resource are embedded and were read back out of the PE; running it on Windows and artist testing remain |
| 9 | Canvas presets, print setup, guides | **Done in code** — 53 presets in seven categories, full unit/DPI/aspect/bleed/safe settings, a persisted preset library with favourites, recents, import/export and restore, PNG `pHYs`/`sRGB` metadata and project format version 3; verified by 35 automated tests, never looked at on a screen (§8) |

## 3. What is implemented so far

### Milestone 1 — workspace shell

Real, working behaviour (not painted mock-ups):

* **Window and lifecycle** — 1440×900 default window, 900×560 minimum,
  `windows_subsystem = "windows"` for release builds, quit via `Ctrl+Q`, menu
  or window close.
* **Custom theme** (`ui/theme.rs`) — charcoal palette, per-widget visuals,
  spacing and type scale; dark theme pinned so OS theme never changes the look.
* **Procedural icons** (`ui/icons.rs`) — eight tool icons, undo/redo, zoom
  in/out and the ink-drop brand mark are drawn as vectors; no bitmap assets and
  no dependency on an icon font, so they stay sharp at any DPI.
* **Top bar** — brand (click → About), File / Edit / View / Help menus, brush
  size and opacity fields with clamping, colour picker popup, undo/redo buttons,
  document name and dimensions.
* **Menus that work** — New canvas, Quit, Fit, Actual size, Zoom in/out, panel
  visibility toggles, Keyboard shortcuts, About. Entries for unimplemented
  features are disabled and their tooltip names the milestone that delivers them.
* **Left tool rail** — seven tools, selected-state highlight, tooltips with
  shortcut keys, active-colour chip.
* **Canvas viewport** — page drawn at the current zoom with a drop shadow and
  hairline border; white / transparent (real checkerboard) / custom backgrounds;
  wheel zoom anchored at the cursor; middle-drag and Hand-tool panning; fit to
  window; 100% reset; "canvas is off-screen" hint; brush-size cursor ring. The
  view can also be turned to any angle (see Milestone 7).
* **Right sidebar** — full colour picker with hex readout and eight-swatch
  palette, brush size / opacity / hardness / stabiliser sliders (validated
  setters), canvas size and per-layer memory readout, background switching,
  layer panel clearly marked as Milestone 4.
* **Bottom status bar** — active tool, honest status messages, zoom out / zoom
  in / zoom percentage (click for 100%) / Fit, canvas dimensions.
* **New canvas dialog** — name, four presets, width/height with validation and
  a swap button, megapixel + memory estimate with a warning for large canvases,
  background choice including a custom colour, inline error reporting.
* **Keyboard input** — tool keys, `Tab` for panels, `0`/`1`/`+`/`-` for zoom,
  `[`/`]` for brush size, `Ctrl+N`, `Ctrl+Q`, `Esc` to close dialogs. Typing in
  a text field suppresses letter shortcuts so renaming never switches tools.
* **Document model** — canvas name, dimensions with range validation
  (16…10 000 px), background, aspect ratio, memory estimate.

### Milestone 2 — painting engine

* **Surface** (`engine/surface.rs`) — premultiplied RGBA buffer (the same layout
  `Color32` uses), so compositing is `src * a + dst * (1 - a)` on all four
  channels. Dabs have a smoothstep hardness falloff and always keep a one-pixel
  antialiased edge; spacing scales with the brush radius so fast strokes stay
  continuous without stamping thousands of times per segment.
* **Eraser** — destination-out: alpha and colour are scaled by `1 - coverage`.
  A soft eraser therefore leaves fractional alpha on its rim by design; the
  solid core is fully removed.
* **Eyedropper** — samples the artwork (un-premultiplying so the picked colour
  matches what is on screen), falls back to the canvas background, and reports
  an honest message when there is nothing to pick.
* **Pointer input** (`ui/canvas_view.rs`) — a click stamps one dab; a drag paints
  a smoothed polyline between pointer samples, converted to canvas pixels
  through the viewport transform, so strokes land correctly at any zoom or pan.
  Releasing the button anywhere ends the stroke, and switching tools or
  creating a new canvas cancels it.
* **Stabiliser** — exponential smoothing driven by the Stabiliser slider, with
  a 30-pixel snap so fast flicks do not lag behind the cursor forever.
* **Rendering** (`engine/pyramid.rs`) — the composite is drawn from a tile
  pyramid: only the tiles on screen are uploaded and drawn, at a level chosen so
  one texel is roughly one screen pixel (nearest sampling when magnified,
  linear when zoomed out), composited over the page background.
* **Document state** — painting marks the document modified; the top bar shows a
  dot plus a tooltip. `Canvas::paint_layer` is the only way a stroke changes
  pixels.

### Undo/redo (pulled forward from Milestone 5)

* **Tile patches, not canvas snapshots** (`history/undo.rs`) — a stroke records
  only the 64×64 tiles it touches, with pixels before and after. A full-surface
  snapshot would cost 8 MB per stroke at 1080p; a typical stroke costs tens of
  kilobytes, and a diagonal 500 px brush stroke costs well under a megabyte.
* **Recording** — `Recorder::record_segment` snapshots any not-yet-seen tiles
  before each painted segment; `finish` captures the "after" pixels and drops
  tiles that did not actually change, so a click that lands off-canvas never
  creates an empty undo step.
* **Structural edits move, they do not copy** — undo/redo of a merge, flatten,
  duplicate or delete transfers the layers between the canvas and the history
  entry (`std::mem::take`) instead of cloning them, so a layer is never held
  twice and history memory drops while an edit is undone. A merge snapshots only
  the rectangle its source layer actually covers, not the whole target; a
  flatten stores the previous entries and recomputes the flattened image on
  redo, so it costs one canvas image rather than two. Redo of a merge re-runs
  the merge rather than merely deleting the layer, which also fixes redo
  leaving the target at its pre-merge pixels.
* **History** — undo/redo stacks with a 256 MB cap; when the cap is exceeded the
  oldest edits are dropped (the newest is always kept). A new edit clears the
  redo stack. Creating a new canvas clears history.
* **Wiring** — `Ctrl+Z`, `Ctrl+Y`, `Ctrl+Shift+Z`, the Edit menu and the top-bar
  buttons all call the same `InkforgeApp::undo/redo`, which are disabled when the
  corresponding stack is empty. Undo mid-stroke finishes the stroke first, then
  undoes it. Shortcuts are ignored while a text field has focus.
* Background colour changes are document settings rather than pixel edits, so
  they are not part of undo history.

### Milestone 3 — presets and stabilisation

* **Presets** (`engine/presets.rs`) — 43 tips that differ in ways the artist can
  see, not just in the numbers: Round, Ink Pen (thin, hard), Pencil (fine and
  grainy), Airbrush (90 px, soft, 15% flow), Marker (flat, 55% flow, stacks) and
  Charcoal (grainy, 30%-of-diameter spacing).
* **Spacing** — a new tip parameter: dab interval as a fraction of the diameter.
  Tight spacing paints a solid line; wide spacing leaves a broken, textured one.
* **Grain** — a new tip parameter: per-pixel alpha noise from a deterministic
  integer hash of the canvas coordinate. Grain therefore behaves like the tooth
  of the paper (stable when you paint over an area again) rather than like
  randomness per stroke, and it is reproducible in tests.
* **Selected preset is derived, not stored** — `BrushSettings::matching_preset()`
  compares values, so the highlight disappears as soon as a slider is moved and
  the UI can never claim a preset is active when it is not.
* **Stabiliser** (`engine/stabilizer.rs`) — rewritten from a fixed exponential
  filter into a speed-adaptive one: fast pointer movement reduces smoothing so
  quick strokes stay responsive, slow movement smooths heavily, and the stamp
  point may never trail the pointer by more than 26 canvas pixels. `0%` is an
  exact pass-through.
* **UI** — the sidebar lists the presets with the active one highlighted, and the
  brush section gains Spacing and Grain sliders.

### Milestone 4 — layers

* **Layer model** (`layers/layer.rs`) — every layer owns its own premultiplied
  RGBA surface plus a name, visibility flag, opacity, blend mode, an
  alpha-lock flag and a clip-to-below flag. Seventeen blend
  modes use W3C compositing maths: Normal, Multiply, Screen, Overlay, Soft
  Light, Hard Light, Linear Dodge, Color Dodge, Color Burn, Darken, Lighten,
  Difference, Exclusion, Hue, Saturation, Color and Luminosity. The layer's
  alpha scales its contribution and drives destination coverage, so opacity and
  blending combine predictably instead of being approximated. "Add" and "Linear
  Dodge" are one formula in the specification and in Photoshop, Krita and Clip
  Studio Paint alike, so this is one mode labelled *Linear Dodge (Add)* rather
  than two names for one operation. The four non-separable modes (Hue,
  Saturation, Color, Luminosity) blend the colour as a whole through the
  specified set-lum / set-sat / clip-colour steps. Premultiplied output is
  clamped to its own alpha, so a dodge cannot leave colour behind transparent
  pixels.
* **Layer stack** (`layers/stack.rs`) — bottom-to-top ordering with stable ids,
  an active index, and add / insert / remove / move operations. The last layer
  cannot be removed, so there is always somewhere to paint. Painting targets the
  active layer only, so a stroke is clipped to its layer instead of flattening
  into everything below it.
* **Cached composite** — the visible image is recomputed only for the dirty
  rectangle reported by painting or by a property change, not for the whole
  canvas on every frame. The composite is what the viewport draws and what the
  eyedropper samples.
* **Layer manager** (sidebar) — top-down rows with a live thumbnail, an eye
  toggle, the layer name and its opacity, plus add / group / delete / move up /
  move down / duplicate / merge down buttons; below them the active layer's name
  field, opacity slider, blend-mode picker and a *Lock transparent pixels*
  checkbox. The header reports the layer count, row count and the memory the
  stack uses.
* **Clip to layer below** — CSP's alpha inheritance, as a second layer property
  (`clip_to_below`). It is a compositing feature, not a pixel edit: the clipped
  layer keeps every pixel, its opacity and its blend mode, and the stack
  multiplies its source alpha by the base layer's alpha per pixel
  (`composite_pixel_masked`). A run of clipped layers all inherit from the same
  base, hiding the base hides its clipped children, and a clipped layer with
  nothing below draws nothing. Clipping is resolved per sibling run, so it also
  works inside a group. It undoes in one step and is written to the manifest.
* **Layer masks** — a layer can own one full-canvas coverage buffer. The mask
  stores coverage in its alpha channel (white = visible, black = hidden) and is
  applied by multiplying the layer's source alpha at composite time
  (`composite_entry` calls `composite_pixel_masked`), so the layer's pixels are
  never rewritten. Adding a mask is instant because a fresh mask is uniform
  (reveal all / hide all); the brush paints the mask with a forced white dab
  and the eraser removes coverage, which reuses the whole existing rasteriser
  (hardness, grain, spacing, stabiliser and selection clipping). Invert flips
  the coverage, and deleting the mask restores the layer exactly.
* **Mask undo** — three small history entries keep the promise cheap: painting
  the mask records tiles like any stroke (`MaskPixels`); adding a mask records
  only how it was created (`MaskAdded`, redo rebuilds the uniform buffer);
  deleting a mask moves the buffer between the canvas and the history entry
  (`MaskRemoved`), never copying it. Inverting records the whole-canvas tiles
  it actually changed. Masks travel with the layer through duplication, a
  document flip/rotation and merge-down (the merge bakes coverage into pixels),
  and are saved in the project file.
* **Lock transparent pixels** — CSP's alpha lock, as a layer property (so it
  undoes in one step like opacity, and older projects load with it off). The
  rasteriser has a locked path: a pixel with zero alpha is skipped, and a
  covered pixel is recoloured by mixing in *straight* colour before
  re-premultiplying by the alpha it already had, so the alpha channel is
  bit-identical afterwards. Erasing cannot lower alpha and therefore does
  nothing on a locked layer. Brush strokes, shapes, flood fill and fill
  selection all take that path; because they funnel through
  `Canvas::paint_layer` / `paint_shape` / `apply_fill` / `fill_selection`, the
  UI needed no special cases. A padlock appears on the row so the state is
  visible without selecting the layer.
* **Groups** (`layers/group.rs`) — a layer can be wrapped in a named group
  (folder), which holds that layer plus any layer added while the group is
  selected. Groups have their own visibility and opacity, multiplied into every
  child layer's contribution at composite time, so hiding or fading a folder
  affects all of its contents without touching their pixels.
* **Group rows** — the list draws a caret, a folder icon, the name and the
  opacity for a group. Collapsing hides its children without touching the
  document; it is view state, so it is deliberately not undoable. Clicking a
  group selects it and shows its properties, and painting then asks for a layer
  instead of silently picking one of its children. Deleting a group removes its
  layers with it, and a single undo restores the whole block. Groups are one
  level deep: grouping a layer that already sits in a group is refused with an
  explanation rather than nested.
* **Thumbnails** (`engine/renderer.rs`) — each layer gets a 28 px-downsampled
  texture, rebuilt only when that layer's revision changes, so an idle list costs
  nothing. Textures for layers that no longer exist are pruned every frame.
* **Duplicate and merge down** — duplicating copies pixels and properties into a
  new layer above (named "… copy"). Merging bakes the active layer into the one
  below with its own blend mode and opacity, then removes it; both are recorded
  as a single undo step, and merging stores the receiving layer's previous pixels
  so undo can restore both sides.
* **Flatten image** (Edit menu) — bakes every visible layer into one layer called
  "Flattened" and drops the rest, including hidden layers. Undo restores the
  whole previous stack, its order and its selection, because the flatten entry
  keeps both the replaced layers and the flattened result.
* **Drag to reorder, into and out of groups** — dragging a row tracks the
  pointer against the visible rows, shows an accent insertion line, and moves
  the entry on release. Dropping a layer on a folder row puts it inside that
  group; dropping it on a row of another group, or between children, follows
  that row's parent. Dropping it on a top-level row takes it out of its group.
  The folder the row will join is outlined while the pointer is over it, and it
  expands on drop if it was collapsed, so the layer never disappears into a
  closed folder. Rows dropped between a folder and its children snap to the
  nearest edge of the block, so the flat stack order is always kept valid.
  Groups only trade places with top-level rows (nesting is refused), and the
  whole move — position and group — is one undo step. Dropping a row back where
  it came from records nothing. A muted line under the list ("Drag a row onto a
  folder to move it inside.") makes the gesture discoverable, and a folder with
  no layers of its own draws a muted "Drag a layer onto this folder" where its
  children would be, so an empty folder reads as a target rather than a broken
  row. That line is not a document row: it has no widgets, takes no clicks and
  cannot be dragged. An empty folder also drops its expand caret — with no
  children there is nothing to collapse, and a dead chevron invites a click that
  does nothing; the caret returns as soon as a layer lands inside. Its folder
  icon is drawn dimmer too, so the row reads as an empty placeholder rather than
  a filled group. Hovering any folder row names what it holds — "1 layer in this
  folder", "4 layers in this folder", or "Empty folder — drag a layer onto it to
  put it inside" — counted from the block length, so it never drifts from the
  list, and ends with "Double-click to rename". Layer rows carry the same
  invitation on hover: "Double-click the name to rename this layer."
* **Renaming a row in the list** — double-clicking a layer or folder row swaps
  its name for a text field on the spot, carrying the whole current name
  selected, so the first keystroke replaces it instead of appending to it. (egui
  parks the caret at the end when a field gains focus, so the selection is stored
  on the pass after focus arrives; the state lives under `Id::unique(("layer-rename",
  id))`.) The
  draft lives in `app.rename` (`Rename { id, text, original, focus }`), and
  `finish_rename` picks the layer or group setter from the entry kind, and
  the document is untouched until the field closes: Enter or clicking away
  commits, Escape drops the edit, a blank or unchanged name records nothing. So
  a rename is one undo step whatever was typed along the way. While a row is a
  field it drops its click-and-drag sense, otherwise the row would eat the
  clicks meant for the text caret.
* **Properties panel name field** — the Name box above the opacity slider has
  the same commit rule as inline renaming, so a typed name is one undo step
  rather than one per keystroke. While it is being typed the text lives in
  `app.name_field` (`NameField { id, text, original }`) and the document is
  untouched; Enter or clicking away commits it, Escape or a blank name drops it,
  and selecting another entry commits whatever was typed. The row paints that
  draft while it is being typed (`row_display_name`), so the list and the field
  agree live, and a blank draft falls back to the name the entry still has.
* **Layer-aware history** — pixel edits carry the layer id and touch only that
  layer's tiles; adding, deleting and reordering layers are their own undo
  entries (a deleted layer is stored whole so it can come back), and property
  changes store before/after snapshots. Dragging the opacity slider merges into
  one undo step instead of dozens. Edits merge only with an edit of the *same
  field on the same entry* — matching labels — so a rename after an opacity drag
  is its own step instead of a passenger on the slider's step.

### Milestone 5 — selection, fill and shapes

Undo/redo was delivered early (see the history section above); this milestone
adds the three tools that make editing practical.

* **Selection** (`engine/selection.rs`) — the Selection tool (M) offers a
  rectangle marquee and a freehand lasso, drawn as a black/white dashed outline.
  `Ctrl+A` selects everything, `Ctrl+D` or `Esc` deselects, and a click without a
  drag drops an existing selection. Selections are stored as a bounding box plus
  an optional coverage mask: a hard rectangle has no mask (fast path), while a
  lasso is supersampled 4×4 to an antialiased mask. Painting, erasing, filling
  and shapes all clip to the coverage — `Surface::paint_segment_clipped`
  multiplies each dab's alpha by it, so nothing can leak outside and soft edges
  stay soft.
* **Magic wand** — the third selection mode, with its own Tolerance slider
  (0–128, the same per-channel measure as the fill). It reuses the flood fill's
  contiguous-region search on the visible image, so a click selects the
  connected area of similar colour and never jumps to a disconnected patch of
  the same shade. `Selection::magic_wand` turns that region into an exact
  per-pixel mask (so clipping follows the blob, not its box) and records the
  region's boundary edges for the marching ants; a boundary too ragged to draw
  cheaply falls back to the bounding box. The wand is click-only: a drag neither
  selects nor clears, and it stays inside an existing selection like the fill.
* **Fill** (`engine/fill.rs`) — a scanline flood fill with a tolerance slider.
  It samples the *visible* image (composite over the background), so the artist
  fills the colour they can see, and writes into the active layer only. It is
  bounded by the selection and records the affected tiles before writing, so one
  fill is one undo step. "Fill selection" fills the whole selection directly.
* **Shapes** (`engine/shape.rs`) — line, rectangle and ellipse. Dragging shows a
  live outline; releasing stamps the current brush tip along the path and
  commits one undoable edit. Because shapes reuse the stroke rasteriser they
  inherit the brush size, hardness, grain and the selection clip.
* **Feathering** — a Feather slider (0–40 px) applies a separable box blur to
  the selection mask when it is created, for the marquee, the lasso and the wand.
  The blur runs in O(width × height) using a sliding window, so a 40 px feather
  on a 1080p canvas is still instant. Feathering fades inward from the outline:
  the mask is stored that way, so painting, filling and "fill selection" all see
  exactly the same soft edge.
* **Grow and shrink** — `Selection::resized` dilates or erodes the coverage by
  up to 32 px, so a selection can be nudged outward or inward without
  reselecting. The sidebar drives it from one live slider: the drag's first
  change (`begin_grow`) captures the selection, every later change re-derives
  it from that capture via `preview_grow`, and releasing (`finish_grow`) keeps
  the result and springs the handle back to the middle. Because every step
  starts from the captured selection, dragging out and back restores the
  original size exactly instead of compounding. The mask is treated as grey coverage and filtered with a
  sliding-window max (grow) or min (shrink), which is a square kernel: the two
  steps cancel out exactly, a feathered edge moves with its midpoint, and the
  work stays O(pixels) via a monotonic deque. The buffer is padded with zeros
  first, so shrinking pulls the edge in even where it runs along the canvas
  border, and growing is trimmed back to the canvas. The result always carries
  a mask and its outline comes from the new pixel edges; shrinking a selection
  to nothing clears it.
* **Seeing the soft edge** — a feathered selection is easy to forget while you
  paint, so the part of the mask that is only partly applied is tinted on the
  canvas. `soft_spans` divides coverage into four buckets and merges each row
  into runs, so a feathered blob costs a few spans per row instead of one per
  pixel; the spans are stored on the selection and drawn as a single amber mesh
  (one mesh, not many translucent rectangles, which would seam at every row).
  Hard selections produce no spans at all, and beyond 8000 spans the band is
  dropped so a very ragged region cannot cost thousands of quads a frame.
* **Quick mask** — a sidebar toggle (and the `Q` key) that washes the whole selected area in
  translucent red (`Selection::preview_spans`), so the selection reads as a
  painted mask rather than an outline. Coverage is bucketed into five steps and
  merged into row runs, then drawn as one mesh; the spans are built lazily and
  cached in the selection with a `OnceCell`, because the view draws them every
  frame while an unmasked selection is just one rectangle. Above 20 000 spans
  the wash falls back to the bounding box. With Quick mask on, the feathered
  soft-edge band is skipped: the wash already fades with coverage.
* **Painting the mask** — Quick mask is editable: with it on, the brush paints coverage into
  the selection and the eraser multiplies it down, so parts of the selection can be
  added or removed freehand (`Selection::paint_segment`, reached through
  `Canvas::paint_selection`). Dabs use the rasteriser's falloff and spacing; adding
  takes the maximum of the old coverage and the dab's alpha, so overlapping dabs of
  one stroke settle at the brush's coverage instead of building past it, and erasing
  multiplies by `1 - alpha` so a soft eraser leaves a soft edge. The mask grows to
  hold paint outside the old bounds, and a hard rectangle turns into a dense mask
  first because a hole has to come from somewhere. While the stroke runs only the
  coverage and the cached wash are updated — the outline and soft band are rebuilt at
  `finish_paint` — which keeps a drag responsive on a selection that spans the canvas;
  the marching ants are hidden for the duration for the same reason. Painting with
  nothing selected starts from `Selection::empty`, and an eraser stroke that removes
  the last coverage clears the selection instead of leaving an invisible one.
  Because it edits no pixels, a mask stroke writes no history entry.
* **Invert** — `Selection::inverted` swaps the selected and unselected areas.
  The result spans the canvas and always carries a mask (255 minus the old
  coverage), so a feathered selection becomes a soft-edged hole and the
  outline comes from the new pixel edges. Inverting a full-canvas selection
  leaves nothing, so it returns `None` and the app clears the selection.
* **Tool options** — the sidebar shows a Selection section (rectangle/lasso/wand,
  wand tolerance, feather, grow/shrink and invert), a Shape section (line/rectangle/ellipse) or a Fill section
  (tolerance), depending on the active tool.
* **Input fix worth remembering** — egui reports `drag_started` only after the
  pointer has moved past its drag threshold, so strokes, selections and shapes
  now begin at `pointer.press_origin()`. Without that, a short drag lost its
  beginning and a selection collapsed to nothing; the headless selection test
  caught it.

### Milestone 6 — files

* **`io/project.rs`** — the `.inkforge` format: `"INKFORGE"` magic, a `u32`
  version, a `u32` manifest length, a UTF-8 JSON manifest, then one PNG per paint
  layer in manifest order. The manifest holds the document name, pixel size,
  background, active row, and for each entry its id, name, visibility, opacity,
  blend mode, group membership (layers) and id/name/visibility/opacity (groups).
  Layer pixels are straight-alpha PNGs instead of raw RGBA, so a flat or empty
  layer costs a few kilobytes, and the whole file is lossless.
* **Atomic saves** — a save writes to `<name>.inkforge.part` first and only then
  renames it over the real file (removing the old copy first on Windows, which
  refuses to rename over an existing file). An interrupted save can lose the
  previous version, never the new one, and a half-written project is never read
  as valid because of the magic/length checks.
* **Loading is defensive** — bad magic, an unknown version, a truncated
  manifest, missed pixel data, or a layer whose size disagrees with the canvas
  all produce a readable error instead of a panic; the current document stays
  open. `Canvas::from_entries` re-validates the size range.
* **Save / Save as / Open** — `Ctrl+S`, `Ctrl+Shift+S`, `Ctrl+O`; the first save
  asks for a path and later ones reuse it. The document badge shows the
  filename plus a bullet while there are unsaved changes, and its tooltip names
  the file or the shortcut.
* **No silent data loss** — New canvas, Open project, Open image and Quit all go
  through `guard_unsaved`: if the document is modified, a modal offers *Save and
  continue*, *Discard changes* or *Cancel*, and a failed or cancelled save keeps
  the question on screen. Closing the window with the X button cancels the close
  and asks the same question.
* **Autosave (`io/recovery.rs`)** — while a document is modified, a frame tick
  writes it to a recovery slot every 20 seconds: the same `.inkforge` container
  plus `autosave.json` describing the name, size, timestamp and original project
  path. The sidecar is written last, so a crash mid-write never advertises a
  half project. Writes are skipped while a stroke is in progress so autosave
  cannot stall the brush. The slot lives in the platform data directory
  (`%APPDATA%\INKFORGE\autosave` on Windows, `$XDG_DATA_HOME/inkforge/autosave`
  or `~/.local/share/inkforge/autosave` on Linux, `Application Support` on
  macOS).
* **Crash recovery** — at launch the slot is inspected and, if complete, the app
  opens a *Recover unsaved work* dialog showing the document name, size, when it
  was autosaved and which project file it belongs to. *Recover it* loads every
  layer back, keeps the project association for `Ctrl+S`, marks the document
  modified and immediately writes a fresh copy in case the new session also dies.
  *Discard* deletes the slot, *Keep for later* leaves it for the next launch.
  A damaged slot is reported as a file error and is not deleted. Saving the
  project, discarding a recovery, closing the window or quitting cleanly all
  clear the slot; a cancelled quit leaves it alone.
* **`io/raster.rs`** — import and export. Export flattens the visible layers
  over the document background and writes PNG (RGBA, keeps transparency) or
  JPEG (quality 92, transparency composited onto white); the layer stack is left
  untouched. Import reads PNG/JPEG as premultiplied pixels and either opens them
  as a new document (sized to the image, named after the file) or adds them as a
  new layer above the active one, cropped to the canvas and recorded as one
  undoable insertion.
* **`io/ora.rs`** — OpenRaster (`.ora`) export, the layered format Krita, GIMP
  and MyPaint open. It writes a stored ZIP itself — CRC-32 table, local and
  central headers, end record, DOS timestamps, no compressor dependency —
  holding `mimetype` (`image/openraster`, first and uncompressed), `stack.xml`,
  `mergedimage.png`, `Thumbnails/thumbnail.png` (nearest-neighbour, longest edge
  256 px) and one `data/layerN.png` per paint layer. `stack.xml` follows
  specification 0.0.6: the root stack carries no attributes, the first element
  of a stack is the uppermost, groups become `<stack>` elements with
  `isolation="isolate"`, and all 17 blend modes map to their SVG compositing
  names (`svg:src-over`, `svg:plus` for linear dodge, `svg:hue` and friends for
  the four non-separable modes). Because OpenRaster has no portable attribute
  for masks, live adjustments or clip-to-layer-below, those are baked into each
  exported layer's pixels so the file looks like the canvas; the `.inkforge`
  project keeps them editable. The merged image is produced by the same
  flattening function as PNG export, so the two can never disagree.

### Milestone 7 — interface polish and performance

Landed so far; the remaining items are listed in §7.

* **Canvas rotation** (`engine/viewport.rs`, `ui/canvas_view.rs`,
  `engine/pyramid.rs`) — the view can be turned to any angle, the way a painter
  turns a sheet on an easel, without touching one pixel of the artwork. The
  rotation lives in the same `Viewport` as zoom and pan, so `screen_to_canvas`
  and `canvas_to_screen` are exact inverses at every angle: strokes, fills,
  the eyedropper, selections, shape previews and the brush ring all land under
  the pointer on a turned canvas. Geometry is drawn through one `PageView`
  (canvas → screen including the angle), so nothing can drift out of step; the
  tile pyramid gains `draw_view`, which draws each uploaded tile as a textured
  quad whose corners are turned with the view — the same tiles and the same
  level of detail, no second texture per angle, no re-render of the composite.
  Angles within 1.5° of a quarter turn snap onto it, so upright is reachable by
  hand. Fit measures the *turned* bounding box, otherwise a 90° landscape page
  would hang off the top and bottom. The controls are the gesture (Shift+drag
  with the Pan tool, 0.006 rad per pixel), View ▸ Turn canvas left/right (15°)
  and Straighten canvas, and the status bar's angle readout, which straightens
  the canvas when clicked.

* **Drawing-tablet pressure and tilt** (`engine/pen.rs`, `input/`) — egui only
  reports pointer *positions*, so the Windows build reads the pen itself: the
  app's window is subclassed (`SetWindowSubclass`) and the `WM_POINTER` messages
  carry `GetPointerPenInfo` pressure (`0..=1024`) and per-axis tilt (degrees).
  The hook always falls through to `DefSubclassProc`, which is what promotes a
  pointer message into the mouse input winit expects, so listening for the pen
  cannot take the mouse away from the tools. A reading with the pen out of
  contact is full pressure and no tilt, so a mouse — and every non-Windows build
  — paints exactly the brush that is configured. `engine/pen.rs` turns a reading
  into a size and opacity multiplier, and a new *Drawing tablet* sidebar panel
  exposes three amounts (size from pressure 85%, opacity from pressure 55%, size
  from tilt 30%) plus the live reading, so a pressure value can be seen arriving
  before painting. `BrushSettings::dab_scaled` applies the multipliers with the
  radius floored at 0.35 px, so a feather touch leaves a hairline instead of a
  negative radius; every stroke path (brush, eraser, Quick Mask) goes through
  `InkforgeApp::current_dab`, and the brush cursor ring shows the pressurised
  radius. The Win32 structures are declared by hand in `input/pointer.rs` (no
  bindings crate), and a unit test pins `POINTER_INFO` (96 bytes) and
  `POINTER_PEN_INFO` (120 bytes; pressure at 100, tilt at 108/112) to the
  64-bit SDK layout, so a mistyped field fails the test run instead of
  corrupting memory on a tablet. **Not verified with hardware** — there is no
  pen and no Windows host here (§8).
* **Live window title** (`InkforgeApp::window_title`) — the OS window bar now
  reads `Untitled — INKFORGE`, gains a bullet while the document has unsaved
  changes (`Untitled • — INKFORGE`), and follows renames, opens and saves. It is
  sent with `ViewportCommand::Title` only when the text actually changes, not
  every frame.
* **Recent projects** (`io/recent.rs`, File ▸ Open recent) — the eight most
  recently opened or saved `.inkforge` projects, newest first, stored as
  `recent.json` in the platform data folder next to the autosave slot. Reopening
  an entry moves it to the top instead of duplicating it; entries whose file has
  been deleted or moved are pruned on load and dropped if a click still fails,
  so the menu cannot fill with dead links. *Clear recent projects* empties both
  the menu and the file. The list is a convenience, never a document: every
  operation is best-effort, and a damaged `recent.json` is ignored rather than
  shown as an error.
* **Start state** (`ui/canvas_view.rs`, `InkforgeApp::document_is_empty`) — an
  untouched document shows one small card on the page: *Start painting*, then
  the brush/eraser keys, scroll-to-zoom and middle-drag, and where to open an
  image. It is painted with the canvas painter rather than added as a widget,
  so it never eats a click or the first stroke, and it disappears the moment the
  document has a pixel. "Nothing painted" is decided by scanning the composite
  for non-transparent pixels, cached against the composite revision: the scan
  runs once per change, never per frame, and undo brings the card back because
  the revision moves again. Documents loaded from a project or an image start
  with content, so they never see it.
* **Tooltip pass** (`ui/tool_rail.rs`, `ui/sidebar.rs`) — every tool now carries
  a line saying what it does under its name and shortcut (`Tool::hint`), the
  colour chip shows the current hex value, and each brush slider, the feather
  and tolerance sliders and the layer and group opacity sliders explain what
  they change instead of only showing a value. The layer action buttons and the
  zoom controls already had tooltips. Copy was read and checked in code; it has
  not been looked at on a real screen (see the limitations).
* **Composite tile pyramid** (`engine/pyramid.rs`, `Surface::dirty`) — the
  composite is split into 256-texel tiles across up to eight levels, each level
  half the resolution of the one below. A frame picks the level where one texel
  is between half a screen pixel and a whole one, then draws only the tiles that
  intersect the visible part of the page, so a 10 000 px document at 12% zoom is
  sixteen 1024 px tiles instead of one 400 MB texture. Tiles are built lazily
  from the CPU composite — there is never a second full-resolution copy — and
  the surface's dirty rectangle decides which tiles are rebuilt, per level. A
  change with no recorded region clears the level, so correctness never depends
  on the optimisation being right. Off-screen tiles are released once a level
  passes 128 tiles, and the tiles the current frame drew are always kept.
  Measured on this machine (release, 4096 × 4096 document, one full viewport):
  the old full-canvas `ColorImage` is 16 777 216 texels / 64 MB / 37.5 ms to
  build; the pyramid uploads 786 432 texels (4.7%) at 100%, 1 048 576 (6.2%) at
  12.5%, and 262 144 (1.6%) at 6.2%, with at most 35 tiles drawn. In the
  headless UI test, a 60 px drag on a 1920 × 1080 document uploads 131 072 texels
  — two tiles, 6.3% of the canvas — and painting outside the visible area
  uploads nothing at all.
  Frame cost of an 8192 × 8192 document (256 MB of canvas pixels) in an
  800 × 600 view, release build, measured by
  `cargo test --release --test tiles panning_and_zooming -- --nocapture`:
  panning 7 200 canvas px at 50% over 150 frames costs 0.65–0.68 ms per frame on
  average and 4–8 ms in the worst one, and 125 of those 150 frames uploaded
  nothing at all (9 µs each); zooming from 100% to 5% across 60 frames — through
  every level — costs 0.45 ms per frame on average and 9.1 ms at worst. Settled
  frames at any fixed zoom draw in 3–10 µs. The one expensive moment is the
  first frame at a level the document has never shown: 13 ms at 100%, 22 ms at
  50%, 110 ms at 12.5% (35 tiles of 4 × 4-tap texels) and 11 ms at 3%.
* **Multi-threaded stamping** (`Surface::stamp_rows`, `stamp_workers`) — a large
  dab is split into horizontal bands, roughly one worker per 9 200 px of work,
  capped at eight. Every band writes its own disjoint rows of the pixel buffer:
  no locks, no shared writes, and each pixel is still computed with exactly the
  same maths, so the result does not depend on the worker count. Small and
  medium brushes stay single-threaded because dispatch would cost more than the
  dab.
* **Persistent worker pool** (`engine/pool.rs`) — bands are handed to eight
  long-lived `inkforge-stamp-*` threads instead of spawning a scope per dab.
  Building the eight band slices still happens in the painting code, but it is
  converted to pointer-plus-length work items that the workers unpack and run.
  The pool blocks the caller until every band has finished, which is what makes
  the pointer hand-off sound; a panic inside a band is caught, counted as
  finished, and re-raised on the painting thread instead of silently losing
  part of the stroke. `TaskPool::global()` returns `None` on a single-core
  machine, and sending to a dead worker falls back to running the band inline.
  Measured on this 8-core machine, release build: eight empty tasks cost 84 µs
  through the pool versus 451 µs spawning fresh scoped threads (5.4× less
  dispatch). A/B on the same machine, three runs each, large-dab timings with
  eight workers — 600 px dab: 2.0–3.6 ms with per-dab spawning, 2.1–2.4 ms with
  the pool; 300 px dab: 0.63–1.21 ms versus 0.66–0.86 ms. End to end the 600 px
  dab stamps in 5.9 ms on one worker and 1.8 ms on eight (about 3.2×), the
  300 px dab 2.8 ms and 0.84 ms. The speed-up is not linear — memory bandwidth
  and scheduling both count — but the brush no longer stalls on very large
  tips.

Deliberately **not** implemented yet (and visibly disabled or labelled in the
UI): nested groups, layer links, custom *brush* presets, perspective guides,
an ICC-based colour pipeline, and CMYK / Grayscale / 16- and 32-bit canvases.

### Milestone 9 — canvas presets and print setup (1.5.0)

* **`io/canvas_presets.rs`** — the whole feature as data plus arithmetic:
  `Category`, `Unit`, `CanvasPreset`, `CanvasSetup`, `PresetLibrary`, `Browse`
  and `Sort`. It has no UI dependencies beyond `Color32`, so the window, the
  document builder and the tests all use the same code.
* **The catalog** is rebuilt on demand from a literal table (53 entries) rather
  than read from disk, so a lost library file cannot take the presets with it.
  Ids are stable strings (`print.a4`, `comic.webtoon`) because favourites and
  recents refer to them.
* **Print presets are authored in millimetres or inches**, not pixels, and
  resolved by `unit_to_px` at 300 dpi with one rounding step: 210 mm becomes
  2480.31 → 2480 px, exactly Photoshop's A4. The same function backs the size
  fields, so a user typing 210 mm gets the identical number.
* **Units and resolution are linked, not stored as text.** The document keeps
  pixels and DPI; millimetres, centimetres and inches are projections of those
  two numbers (`px_to_unit`, `unit_to_px`). Switching units cannot drift the
  canvas because nothing is kept in the display unit.
* **Bleed enlarges the canvas.** `CanvasSetup::canvas_size_px` adds
  `round(bleed_px)` on every side and `Canvas::set_print_setup` stores the bleed
  so the trim line can be drawn inwards by it. A 3 mm bleed on A4 therefore
  creates 2550 × 3578 px with the trim at 35 px. This is the printable-file
  convention (art must reach past the trim), and it is stated in the window and
  the limitations rather than being implicit.
* **Validation happens before allocation.** `validate_canvas_size` enforces
  16–20 000 px per side and 200 MP total; `Canvas::try_new` and
  `Canvas::from_entries` both call it, so the window, the project loader and the
  tests share one rule. 20 000 px per side alone would allow 400 MP, which is
  why the area limit exists separately (800 MB per layer).
* **The project format grew to version 3** with four `#[serde(default)]`
  fields — `dpi`, `pixel_aspect`, `bleed`, `safe`. Version 1 and 2 files open
  with 72 dpi and no guides; an older build refuses a version 3 file through the
  existing version check.
* **PNG metadata (`io/png.rs`)** — the `image` crate writes a bare PNG, so
  `encode_rgba` inserts `sRGB` and `pHYs` chunks directly after `IHDR`, with
  their own CRC-32 (the same table the ZIP writer uses). `pHYs` carries the
  document's DPI as pixels per metre, with separate x and y densities when the
  pixel aspect is not square. The OpenRaster merged image gets the same metadata.
* **The preset library** is `canvas-presets.json` in INKFORGE's data folder:
  custom presets, favourite ids and the last ten used presets. Loading is
  tolerant (a damaged file silently yields the built-ins, like the recent list);
  saving reports a failure in the status line, so a read-only data folder cannot
  block painting. Imported presets whose id already exists are stored alongside
  with a fresh id instead of overwriting.
* **The window** (`ui/new_canvas.rs`) keeps the draft in `NewCanvasState` and
  performs library actions after the frame, because the library is borrowed to
  build the card list while the window is drawn. Cards are painted by hand so
  each shows the preset's real proportions; the star is hit-tested separately
  from the card; arrows walk the list and scroll the selected card into view.
* **Guides and rulers** (`ui/canvas_view.rs`) take their ruler strips out of the
  central panel rather than painting over the page, read the same `Viewport`
  mapping as the pointer, and pick a 1/2/5/10/… step that leaves room for the
  labels. The grid, trim line, safe area and trim marks all go through
  `PageView::point`, so they follow canvas rotation even though the rulers hide
  while it is turned.

### Document operations and selection history (unreleased)

Continuing the inventory in dependency order:

* **Resize, crop and trim (`engine/resize.rs`, `Surface::resize`,
  `LayerStack::resize`)** — a resize copies the overlap and discards the rest,
  so it is exact and lossless; nothing is interpolated. `Anchor` defines where
  the old origin lands in the new page, which makes shrinking a crop with a
  corner anchor. `Canvas::crop_to` is the same path with a negative offset, and
  `Canvas::visible_bounds` (the composite's opaque box) is what trimming uses.
* **Undo for page changes** — `Change::CanvasResize` stores the pre-change
  entries and replays the resize for redo, so one copy of the pixels is
  retained instead of a before/after pair. `Change::PrintSetup` stores the four
  setup values; both restore through `Canvas::restore_entries`.
* **Canvas size window (`ui/canvas_size.rs`)** — width, height, resolution,
  bleed, safe area and a 3 × 3 anchor grid. The Image menu also exposes "Crop
  to selection" and "Trim transparent edges". Applying print setup and size
  are separate undo steps.
* **Selection history** — every selection entry point now funnels through
  `InkforgeApp::set_selection`, which stores an `Option<Selection>` pair. Grow /
  Shrink previews move the selection without touching history and commit one
  step on release; a Quick Mask stroke captures the selection at stroke start
  and commits at the end. `Selection::same_shape` stops no-op steps.
* **Known limits** — a resize clears the selection (a flip or turn keeps it;
  a resize moves the page origin, not the pixels), and selection save/load is
  still open. The Canvas size window has only been rendered
  headlessly.

### Tier 1 local work after 1.5.0 (unreleased)

Directed by the remaining-work inventory, in dependency order, kept local until
release is authorised:

* **Workspace preferences (`io/settings.rs`)** — panel visibility, ruler unit,
  rulers/grid/guides/print marks and grid spacing persist in `settings.json`.
  `InkforgeApp::enable_preferences` loads and applies them, and `frame` writes
  them only when they differ from the last saved copy, so an idle frame never
  touches the disk. A failed write is reported once, not every frame.
* **PNG resolution import (`io/png.rs`, `io/raster.rs`)** — `read_dpi` walks the
  PNG chunk list up to `IDAT` and reads `pHYs`; `open_image` now reads the file
  once, decodes it and applies the density (two different densities become the
  pixel aspect). A file without the chunk keeps the default 72 dpi.
* **Undoable background** — `Change::Background` stores the before/after
  `Background` pair, merges a colour drag into one step, and is applied through
  `InkforgeApp::set_canvas_background`, which the Canvas panel now calls.
* **History merging across undo** — testing the background edit exposed that a
  property change made after undo/redo could merge into the edit undo had just
  restored, silently rewriting history. `History` now keeps `merge_open` false
  after undo/redo and after a clear, so the next change is its own step.
* **CI (`.github/workflows/ci.yml`)** — `cargo check --all-targets` and
  `cargo test --all-targets` on Ubuntu 24.04 with the X11/Wayland libraries.
  Written but never executed: the workflow only runs when the repository is
  pushed, which needs explicit authorisation.
* **Selection combine modes and the polygon lasso (1.6.0)** —
  `SelectionMode::Polygon` (click corners, close on the first one, Enter or a
  double-click; Escape cancels) reuses the lasso geometry so the mask maths is
  shared. `SelectionCombine::{Replace, Add, Subtract, Intersect}` combine two
  selections per pixel (max, scale-down, multiply), then trim to what survives.
  Every tool now finishes through `InkforgeApp::apply_selection`, which is what
  makes "add" behave the same for the marquee, both lassos and the wand.
* **Selections follow a document transform (item 24)** — `Canvas::transform` now
  re-describes the selection in the new coordinates via
  `Selection::transformed`, using `Transform::forward_pixel` (the exact inverse of
  `source_pixel`, covered by its own test). Because undo applies the inverse
  transform, the selection comes back with it and no extra history entry is
  needed. A resize still clears the selection.
* **Release preparation** — version 1.6.0 in `Cargo.toml`/`Cargo.lock`,
  `CHANGELOG.md`, `RELEASE-NOTES-1.6.0.md`, `RELEASING.md` and the rebuilt
  Windows ZIP. Nothing is pushed or published until that is authorised.
* **Not done yet** — selection save/load, free transform of a layer, filters,
  rulers on a turned canvas, and the rest of the inventory (`REMAINING-WORK.md`
  in the project files).

## 4. Architecture notes and deviations from the suggested tree

* `src/lib.rs` + `src/main.rs`: the app lives in a library and `main.rs` only
  boots the window. This is what makes `tests/` able to call real application
  code; a binary-only crate cannot be integration-tested.
* `engine/presets.rs` and `engine/stabilizer.rs` keep the tip catalogue and the
  pointer smoothing out of the UI and out of `app.rs`, so both are unit-testable
  without an egui context.
* `engine/pool.rs` is an addition beyond the suggested tree: a small persistent
  thread pool that only exists so large brush dabs do not pay thread creation
  cost per band. It knows nothing about pixels — it runs boxed tasks and waits
  for them — which keeps the unsafe pointer hand-off in one reviewed place in
  `engine/surface.rs`.
* `ui/` is split per region (`top_bar`, `tool_rail`, `sidebar`, `canvas_view`,
  `status_bar`, `dialogs`, `theme`, `icons`) rather than the suggested
  `toolbar/sidebar/panels` grouping, because each region owns distinct state
  and this keeps files short and single-purpose.
* Modules are split by responsibility, not by widget: `layers/` owns the pixel
  layers and the compositing maths, `engine/` the rasteriser and document
  operations, `history/` only undo bookkeeping, `ui/` one file per region.
  `io/` (the suggested `project/`) landed with Milestone 6: one file for the
  project container, one for raster formats, plus `io/mod.rs` for the shared
  premultiplied→straight-alpha conversion.
* `InkforgeApp::frame` holds one complete UI pass (shortcuts → actions → panels)
  and `eframe::App::ui` only calls it. That is what lets the headless tests cover
  keyboard handling; calling `ui::show` directly would skip every shortcut.
* Autosave is wired up in `InkforgeApp::new`, not `with_defaults`, so unit and
  headless tests never read or write the machine's real data directory; tests
  call `enable_autosave` with a scratch folder and a zero interval instead.
* Dependencies are added per milestone, not up front: milestone 1 needs only
  `eframe`; `image` (PNG + JPEG only, no other codecs), `serde`, `serde_json`
  and `rfd` arrived with file I/O in Milestone 6.
* `assets/icons` is unused by design: icons are vector-drawn in code. The
  `assets/brushes` folder arrives with brush presets in Milestone 3.

### Environment/API gotchas worth remembering

`eframe 0.37` / `egui 0.37` differ from most published tutorials:

* `eframe::App` requires `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)`
  instead of the older `fn update(&mut self, ctx, frame)`.
* Panels are `egui::Panel::left/right/top/bottom(id).exact_size(..).show(ui, ..)`
  (there is no `SidePanel`/`TopBottomPanel` any more) and `CentralPanel::show(ui, ..)`.
* Styles are themed: `ctx.set_theme(ThemePreference::Dark)` and
  `ctx.all_styles_mut(|style| ..)`; `ctx.set_style` no longer exists.
* `ctx.style()` → `ctx.global_style()`.
* `ctx.wants_keyboard_input()` no longer exists. `egui_wants_keyboard_input()`
  is **not** a drop-in replacement: it reports any focused widget, and clicking
  the canvas focuses it, so using it to gate shortcuts silently disables them
  after the first stroke. Use `ctx.text_edit_focused()` instead — the headless
  Ctrl+Z test caught exactly that.
* `corner_radius` is a `u8` value (`CornerRadius::same(6)`), `Margin` uses `i8`,
  and `Painter::rect_stroke` takes a `StrokeKind`.

## 5. Verification log

| Check | Command | Result |
| --- | --- | --- |
| Type/compile check, all targets | `cargo check --all-targets` | Passes, 0 errors, 0 warnings |
| Tests | `cargo test --all-targets` | Passes, 0 failures (exit 0), including the workspace tests (persisted preferences, undoable background, PNG resolution import), the document-operation tests (resize, crop, trim, print setup, selection undo) and the resize-anchor unit tests (unit conversion, validation, aspect maths, the preset library, PNG chunk writing) and 15 new canvas-preset integration tests. The rest of the breakdown: 7 painting, 39 layers/groups, 7 layer alpha lock, 16 blend modes, 9 clipping, 12 layer masks, 16 colour adjustments, 8 history, 7 brush presets, 6 stabiliser, 50 selection/fill/shapes, 18 headless UI, 9 file I/O, 12 OpenRaster export, 7 autosave/recovery, 7 recent projects, 11 tile pyramid, 7 performance, 9 pen pressure/tilt, 2 Win32 pointer layouts, 10 canvas rotation, 8 document transforms |
| Canvas rotation | `cargo test --test view_rotation` | A fresh view is upright; a quarter turn sends the page corners where they look like they go; screen and canvas stay exact inverses at eight angles with zoom and pan applied; cursor-anchored zoom on a turned view does not drift; angles accumulate and wrap into (-180°, 180°]; 1.5° near a quarter turn snaps onto it while a deliberate 0.5 rad is left alone; reset and NaN/infinite input behave; a turned landscape page fits into the window height (0.528 at 1000 × 600) instead of the width; the page rectangle itself is not turned. The turned *rendering* has not been looked at — no display here (§8) |
| Pen pressure and tilt | `cargo test --test pen` and `cargo test --lib pointer` | A mouse leaves the brush exactly as configured; half pressure paints fainter *and* narrower (a dab's footprint stops short of where full pressure reaches); a feather touch still leaves a line above `MIN_DAB_RADIUS`; 60° of tilt widens a 20 px tip to 26 px without changing opacity, and never pushes the brush past `MAX_SIZE`; 0% amounts and the off switch ignore the pen entirely; a NaN/infinite reading falls back to full pressure; a real stroke driven through `begin_stroke`/`extend_stroke` covers 18 px above its path at full pressure but not at 25%; and the Win32 `POINTER_INFO` / `POINTER_PEN_INFO` sizes and field offsets match the 64-bit SDK. The tablet path itself has **not** run — no pen and no Windows here (§8) |
| Start state | `cargo test --test ui_smoke` | An untouched canvas paints the "Start painting" card, a simulated drag removes it, and undoing the only stroke brings it back (proving the cached empty check follows the composite revision) |
| Window title | `cargo test --test ui_smoke` | The title names the document and gains a bullet exactly when the document is modified; the frame path sends the new title |
| Recent projects | `cargo test --test recent` | Newest-first ordering, reopening moving an entry to the top without duplicating it, the eight-entry cap, deleted files being pruned, a damaged list being ignored, forget/clear, and opening a recent project through the app (plus a missing file leaving the menu and showing a notice) |
| Partial uploads | `cargo test --test performance` | A dab marks only its own pixels, separate dabs accumulate into one bounding region, a full clear marks everything, a region write marks exactly that region and clearing the flag forgets it, and a short stroke on a 4096² surface does not dirty the whole canvas |
| Multi-threaded stamping | `cargo test --test performance` | One worker and four workers stamp byte-identical pixels for a large dab, a dab clipped by the canvas edge, and a large erase |
| Pool dispatch | `cargo test --release --lib pool -- --nocapture` | Eight empty tasks run through the pool in 84 µs versus 451 µs for eight freshly spawned scoped threads, and the pooled run is asserted faster |
| Stamping speed | `cargo test --release --test performance -- --nocapture` | 600 px dab: 5.9 ms single-threaded, 2.8 ms on four workers, 1.8 ms on eight; 300 px dab: 2.8 ms, 1.0 ms, 0.84 ms (8-core container, release build) |
| Pan and zoom cost (large document) | `cargo test --release --test tiles panning_and_zooming -- --nocapture` | On 8192 × 8192: panning 7 200 canvas px at 50% costs 0.65–0.68 ms mean / 4–8 ms worst per frame with 125 of 150 frames uploading nothing (9 µs each); zooming 100% → 5% over 60 frames costs 0.45 ms mean / 9.1 ms worst; settled frames at any fixed zoom draw in 3–10 µs; the first frame at a previously unseen level costs 11–110 ms |
| Partial uploads (headless) | `cargo test --test ui_smoke painting_uploads_only_the_changed_region` | A simulated drag uploads 13 700 of 2 073 600 pixels (0.66% of a 1920 × 1080 canvas) and the uploaded region is marked clean |
| Presets | `cargo test --test presets` | Defaults match preset 0, all 43 presets round-trip through their settings, every pair paints a measurably distinct mark (ink mass, coverage and a hash of the alpha shape all have to differ, 903 pairs), names and parameter sets are unique, every shelf is populated, spacing changes density, grain varies alpha reproducibly, a clean tip has a uniform core |
| Colour adjustments | `cargo test --test adjustments` | A neutral adjustment returns pixels untouched; brightness lifts and darkens by the expected amounts; positive contrast pushes a light tone up and full negative contrast flattens to mid-grey; saturation 0 turns red into its Rec. 601 grey and 200% pushes channels apart; hue +120° turns red into green and +240° into blue while grey is left alone; alpha survives; wild and NaN values are clamped; the composite applies the adjustment; resetting restores the exact original composite and leaves the layer's own pixels untouched; a clipped layer keeps its own adjustment; merging down bakes it; one undo returns the layer to neutral and redo restores it; the values survive save/load, and a project written before adjustments existed loads neutral |
| Document transforms | `cargo test --test transform` | Flip horizontal and vertical map every pixel to its mirror; a quarter turn swaps the document size and the old left column becomes the new top row; every transform is a permutation (each source pixel read exactly once, none left out) and its inverse restores the exact pixels and size; two clockwise turns equal a half turn; clockwise and counter-clockwise cover the same source pixels; forward and inverse pixel mapping round-trip; a transform carries the selection with the artwork and the inverse brings it back; every layer of the stack turns with the canvas; rotate → undo → redo through the app restores pixels and size; a turned document saves and reloads with the new dimensions and the same pixels |
| Stabiliser | `cargo test --test stabilizer` | 0% is exact, shaky paths are flattened, lag never exceeds the 26 px limit, light smoothing follows closely, a held pointer converges, non-finite input is ignored |
| Undo/redo | `cargo test --test history` | Stroke undo/redo round trip, undo restoring only the painted area, undoing an erase, redo invalidation, empty edits not recorded, 256 MB-style trimming keeping the newest edit, no-op undo on empty history, a stroke staying on its own layer |
| Layers and groups | `cargo test --test layers` | Default single layer, add-above-and-select, last-layer protection, reorder changing what composites on top, hide/show, 50% opacity maths, multiply blending, dirty-region composite caching, undoable add/delete, painting landing on the selected layer, duplicate copying pixels and properties, merge down baking both sides as one undo step, merge undo restoring the removed layer and target pixels, merge redo re-applying the pixels, the merge snapshot staying proportional to the merged layer rather than the canvas, undo moving a merged layer rather than copying it, the bottom layer refusing to merge, thumbnails keeping the canvas aspect, flatten baking the visible stack (and its undo/redo), flatten history not retaining a third canvas image, hidden layers being discarded but restored by undo, drag-to-reorder slot maths and a no-op drag adding no history, grouping a layer (and refusal to nest), group visibility and opacity reaching every child, adding a layer into the selected group, deleting a group as one undo step, painting refused while a group is selected, collapse being view-only, slots mapping to the group whose block holds them, dragging a top-level layer into a group (one undo step, undone back to the top level) and dragging a layer out of a group (undo and redo changing its group back and forth) |
| Blend modes | `cargo test --test blending` | Every W3C formula checked against hand-computed values (multiply, screen, overlay, hard light, soft light, linear dodge, colour dodge and burn, darken, lighten, difference, exclusion, hue, saturation, colour, luminosity); a transparent backdrop leaves each mode's colour intact; a half-opaque white layer stores premultiplied colour as well as half alpha (the old code scaled only alpha and painted too bright); blending happens during stack compositing; every mode has a unique key and rounds trips through save/load; an older project's missing blend key loads as Normal and `add` loads as linear dodge |
| Clip to layer below | `cargo test --test clipping` | A clipped layer shows only where the base has paint and keeps its own colour; several clipped layers share one base; hiding the base hides its children; a clipped layer with nothing below draws nothing; clipping works inside a group and a hidden group hides the run; the clipped layer keeps its blend mode and opacity; a fully transparent base hides everything; the property is one undo step; and it survives saving and loading |
| Lock transparent pixels | `cargo test --test locking` | The lock is off on a fresh layer; a locked stroke recolours an existing disc while its alpha stays exactly 255, and leaves no pixel behind where the same stroke crossed transparency; a locked eraser cannot remove paint; a locked fill recolours the covered area and creates nothing outside it; unlocking lets paint reach empty pixels again; a locked shape line stays inside the existing paint; and the flag survives saving and loading a project. The checkbox and padlock have not been looked at — no display here (§8) |
| Brush rasteriser | `cargo test --test painting` | Stroke continuity along a path, round dab shape, 50% opacity landing ~128 alpha, eraser removing paint, straight-alpha sampling, off-canvas clipping, revision bumps |
| Selection, fill, shapes | `cargo test --test editing` | Selection normalising and clamping, click-sized drags rejected, painting clipped at the selection edge, select all/clear, fill covering only the matching area, fill clipped to the selection and undoable, fill selection, line/rectangle/ellipse geometry, shapes clipped, selection drag and one-step shape commit through the app, lasso polygon coverage, feather coverage ramps, painting and filling clipped to a lasso, soft edges when painting a feathered selection, a lasso drag through the app, a wand selecting only the connected block it was clicked in (with a boundary outline), its tolerance widening the region to the neighbouring shade at 24 but not at 0, painting following a wand region's shape rather than its bounding box, a feathered wand fading at its edge, the wand mode selecting through the app, growing a rectangle by 4 px on every side and shrinking it back exactly, a 3 × 3 selection shrinking away entirely, growth stopping at the canvas edge, dragging the Grow / shrink slider being relative to the drag's start rather than cumulative, the slider reaching an empty selection and a selection that shrinks away, inverting a rectangle into a canvas-wide hole, feathering surviving an invert, a double invert returning the original, a full-canvas invert leaving nothing, the Invert button through the app, a feathered selection gaining tinted soft-edge spans that stay inside it and skip the fully selected middle, hard selections gaining none, the band surviving both grow and shrink, a hard selection's Quick mask being one solid wash over its bounds, a feathered Quick mask fading to a weaker edge, the wash being cached rather than rebuilt, the Quick mask toggle reporting its state, `Q` toggling the wash through the frame path, brush strokes adding coverage to the mask (inside and outside the old selection, growing its bounds), the eraser removing coverage, a mask stroke leaving every layer pixel untouched and writing no history entry, painting with nothing selected creating a selection, an eraser stroke that clears the last coverage clearing the selection, a painted mask regaining its pixel-edge outline when the stroke ends, and the brush painting the layer again once the Quick mask is off |
| Selection combine modes, polygon lasso, transformed selections | `cargo test --test selection` | Adding a second rectangle keeps both areas and the gap between them, and one undo restores the first alone; subtracting punches a hole and intersecting keeps only the overlap, trimmed to it; intersecting disjoint areas leaves nothing selected; the polygon lasso closes on its first corner through the app, selects the enclosed area and undoes back to the previous selection; fewer than three corners select nothing and Escape cancels without touching the selection; a quarter turn re-describes the selection in the new coordinates (the old top-left corner ends up top-right), swaps the page size, and undo restores the original bounds; a horizontal flip moves it to the opposite edge; a feathered selection keeps a partial-coverage edge through a half turn |
| Document size, crop and trim | `cargo test --test document_ops` | Nine anchors place the artwork where the grid says, growing pads with transparency while shrinking crops without resampling, resize keeps every pixel inside the overlap exactly, a crop to a selection keeps only that region, trimming drops a transparent margin, each operation is one undo step through the app, and resolution / bleed / safe area edits reach the document and undo |
| Workspace preferences | `cargo test --test workspace` | Panel visibility, ruler unit, rulers/grid/guides/print marks and grid spacing survive a restart through `settings.json`, a missing file falls back to defaults, a damaged file is reported once and painting continues, and a read-only folder does not interrupt the session |
| File I/O | `cargo test --test io` | A project with two layers, a group, a blend mode, opacity, names, group membership and painted pixels round-trips through save/load unchanged; damaged files (bad magic, truncation, a future version, a missing file) are refused with readable messages; an impossible canvas size is rejected; a PNG export reopens as a document with the same pixels; a JPEG export is readable and composites transparency onto white; an imported image becomes a cropped layer above the active one; a text file named `.png` fails cleanly |
| OpenRaster export | `cargo test --test ora` | An archive with the required layout — `mimetype` first, stored and exact, then `stack.xml`, `mergedimage.png`, `Thumbnails/thumbnail.png` and one `data/layerN.png` per layer — read back by an independent stored-ZIP reader that checks both header sets and every CRC; a 600 × 300 document getting a 256 × 128 thumbnail; layers, groups, names, opacity, visibility and all blend modes reaching `stack.xml` as SVG compositing names with XML-escaped names (`&amp;`, `&quot;`, `&lt;`); the top layer written first; a layer's exported pixels matching its paint; a mask and a live adjustment baked into the exported PNG while the in-memory layer keeps both editable; `mergedimage.png` byte-identical to a PNG export of the same document; a group's opacity and visibility surviving as stack attributes; an unwritable path reported cleanly; and a single-layer document still writing exactly one stream |
| Canvas presets | `cargo test --test canvas_presets` and `cargo test --lib canvas_presets` | All 53 documented presets exist with exactly the documented pixel sizes (measured A4 2480 × 3508, US Letter 2550 × 3300, A0 9933 × 14043, Poster 5400 × 7200), every category is populated and every id is unique; print presets carry 300 dpi and convert 210 mm / 8.5 in to the published pixel counts; pixels ↔ mm / cm / in round-trip within one pixel; the same 210 mm doubles in pixels when the DPI doubles; 1 inch is 25.4 mm and 300 px at 300 dpi; aspect labels read as 3:4, 9:16 and 12:17 for A4; the aspect lock and the ratio chips reshape the canvas; impossible sizes (too small, 0 dpi, 400 MP) are refused while a webtoon page passes; a document built from A4 with 3 mm bleed and 10 mm safe area is 2550 × 3578 px with the right guides, a transparent background and a Sketch layer, and starts unmodified; black is stored as black rather than a custom colour; the library filters by category and search, sorts by name and size (A0 first), follows favourites, floats the recently used preset to the top, survives a restart through `canvas-presets.json`, updates and deletes custom presets, restores the defaults without losing the built-ins, and exports/imports a preset file without colliding ids; creating a canvas from the window applies size, DPI, background and sketch layer and records the preset in the recents; a hand-typed size is recorded as a custom recent; a project round-trips dpi / pixel aspect / bleed / safe area; and a PNG export carries `pHYs` 11811 pixels per metre (300 dpi) plus an `sRGB` chunk |
| Autosave and recovery | `cargo test --test recovery` | Unsaved work is written to the slot with its name, size and pixels intact; a clean document writes nothing; a simulated crash leaves a copy that the next session offers, and recovering restores the artwork as unsaved work and refreshes the copy; discard deletes the slot; keep-for-later leaves it; a damaged slot is reported through the error dialog and not deleted; one real frame tick performs the autosave; a plain session has autosave off |
| Headless UI render | `cargo test --test ui_smoke` | Workspace, canvas, tool rail/sidebar hiding, a group row expanded and collapsed, an empty folder painting its "drag a layer onto this folder" line and dropping that line once a layer lands, a folder row and a layer row each renamed in place painting their draft names, the field opening with the name pre-selected, and giving the old name back on cancel, typing in the properties Name box renaming a layer row and a folder row live while the document stays untouched and then committing as one undo step, a selection drag through the frame path with the dashed overlay, all five dialog states (new canvas, unsaved changes, file error, about, shortcuts) run egui passes at 1440×900 and at the 900×560 minimum size with no panic and real paint output; the New canvas window renders its presets in all three scopes (all, favourites, recent) and in the narrow stacked layout at 900×560, including a search with no matches; and the workspace with rulers, grid, trim marks and bleed/safe guides on paints without panicking |
| Pointer painting (headless) | `cargo test --test ui_smoke` | A simulated press → move → release over the canvas paints real pixels, marks the document modified and ends the stroke |
| Undo shortcuts (headless) | `cargo test --test ui_smoke` | After a simulated stroke, a synthetic `Ctrl+Z` removes all paint, the redo stack fills, and `Ctrl+Y` restores the stroke — driven through `InkforgeApp::frame`, the same code path the window uses |
| Application launch | `cargo run` | **Not possible here** (headless Linux container, no display server) |
| Windows release build | `cargo zigbuild --release --target x86_64-pc-windows-gnu` | **Passes** (1.6.0, exit 0): a GUI-subsystem PE32+ x86-64 executable, 13 282 816 bytes, imports only DLLs that ship with Windows 10/11 (`kernel32`, `user32`, `gdi32`, `ole32`, `dxgi`, `dwmapi`, `uxtheme`, `bcryptprimitives`, `api-ms-win-crt-*`) — no VCRUNTIME, MSVCP or libgcc/libstdc++ DLL. The only warning is zig's own libunwind `dllexport` redeclaration, from the toolchain |
| Portable ZIP | `scripts/package-windows-cross.sh` | **Passes**: `dist/INKFORGE-1.6.0-windows-x86_64/` holds `INKFORGE.exe` (13 282 816 bytes), `README.txt` and `LICENSE-MIT.txt`; the ZIP is 5 800 579 bytes (5.6 MB) with SHA-256 `848f54f427ac14e7b43d3c0c942069181fdc5fc0306c82c10a198f57b907eab0`, passes Python `zipfile.testzip()`, and unpacks to exactly those files |
| Icon and version metadata in the PE | pefile over `target/x86_64-pc-windows-gnu/release/inkforge.exe` | **Passes, as a file read**: the resource directory holds seven `RT_ICON` images (1 128–33 066 bytes: the 16/24/32/48/64 px bitmaps and the 128/256 px PNGs), one `RT_GROUP_ICON`, and an `RT_VERSION` block whose `VS_FIXEDFILEINFO` has signature `0xFEEF04BD`, file and product version `1.6.0.0`, `VOS_NT_WINDOWS32` and `VFT_APP`; all eight string fields are present (`ProductName` INKFORGE, `FileDescription` "INKFORGE - a digital painting application for artists", `CompanyName`, `FileVersion` 1.6.0, `InternalName`, `LegalCopyright`, `OriginalFilename` INKFORGE.exe, `ProductVersion`). Explorer and Task Manager were **not** opened to look at it (§8) |
| Running the release on Windows | double-click `INKFORGE.exe` | **Not possible here** (Linux, no Windows, no display); the executable has never been launched — see §8 |

What is therefore verified: the code compiles cleanly against the real
`eframe`/`egui` 0.37 API, the panel and dialog code actually executes in a
headless egui pass, the brush rasteriser and stabiliser behave as specified
under test, and through the real frame path a simulated pointer drag writes
pixels into the active layer (and therefore into the composite), finishes the
stroke, and is undone and redone by Ctrl+Z / Ctrl+Y.

### Known limitations of this build

* Canvas rotation is verified mathematically (the transform, its inverse,
  snapping and the fit), but the turned rendering has never been seen: no
  display and no GPU in this environment. Whether a rotated page looks crisp,
  whether the turned tiles butt together without a seam, and whether the
  Shift+drag gesture feels right are all unconfirmed until the app runs on a
  desktop. The garment box around the visible area is used for culling, so a
  turned view may upload a few tiles that never show.
* Pen pressure and tilt are read from Windows `WM_POINTER` messages, and the
  mapping from a reading to a dab is covered by tests, but no drawing tablet has
  ever been connected to this build: whether a pen reaches the app, whether the
  pressure curve feels right, whether the artist's tablet reports tilt at all,
  and whether the message hook leaves the mouse working are all unverified. On
  Linux and macOS the brush behaves exactly as before (full pressure).
* Undo history is capped at 256 MB and silently drops the oldest edits once
  full; there is no "history size" preference yet.
* Flipping or rotating the document clears the selection instead of turning it
  with the artwork, and undo does not bring the selection back. The artwork
  itself is restored exactly.
* Layer adjustments are applied when a layer is composited, so the eyedropper
  and flood fill see the adjusted colours (they sample the visible image), while
  merging down bakes them into the target. There are no curves or levels yet,
  and no document-wide adjustment layer.
* Masks are layer-only: groups cannot have one, a mask cannot be moved to
  another layer independently, and there is no mask density or feather control.
  While mask editing is on, flood fill and shapes refuse with a message instead
  of silently painting the layer; the brush and eraser are the mask tools.
  Mask documents are project format version 2 (and print documents version 3),
  so older builds refuse them (with a "newer INKFORGE" message) rather than
  misreading the streams; version 1 and 2 projects still open in 1.5.0, at
  72 dpi with no guides. A mask is a full canvas buffer, so it costs one extra
  canvas image of memory per masked layer.
* Layer-mask rendering has been verified at the composite level and through
  save/load, never by eye on a screen (§8).
* The 43 brush presets are parameter sets the existing rasteriser understands:
  size, hardness, opacity, spacing, grain and smoothing. There is still no
  stamp-tip or dual-brush engine, so "bristle" and "watercolour" brushes are
  shaped by spacing and grain rather than by a real hair or wet-media simulation.
  None of them has been looked at on a screen (§8).
* Undo covers pixel edits, layer structure/properties, selections (including
  Quick Mask strokes), the canvas background and canvas size/crop. A selection
  is not saved inside a project, and an image resize clears it.
* Quick Mask painting is verified headlessly (coverage, bounds growth, erasing
  and the outline rebuild). How the wash follows a stroke, and the marchings
  ants reappearing when it ends, have not been seen on a real screen.
* Tile rendering has only been verified headlessly (upload counts, levels,
  averaging); how tile borders look on a real screen at odd zoom levels has not
  been inspected, and a hairline seam at a tile edge would come from linear
  filtering there.
* The first frame at a pyramid level the document has not shown yet builds that
  level's visible tiles inside one frame: up to 110 ms on an 8192 × 8192
  document at 12.5%. Every later frame at that zoom is in the microseconds. A
  wheel-zoom sweep therefore stutters once per level instead of once per frame;
  spreading the build over several frames is not implemented yet.
* The composite is drawn from tiles, but every tile is a separate egui texture:
  a pan across a new area binds the tiles that came into view (tens of textures
  per frame) rather than a single draw call. Coarse tiles average at most 4 × 4
  samples per texel, so a fully zoomed-out view is a filtered approximation of
  the painting rather than an exact mipmap, and the coarsest levels are rebuilt
  from the CPU composite when the pixels underneath them change.
* Deleting a layer still keeps that layer's pixels in history (it has to, to
  bring it back), and a merge keeps the whole merged-away layer. Undo and redo
  no longer duplicate those layers, but the retained copy still counts against
  the 256 MB cap; strokes stay tile-sized.
* Clipping is resolved against the sibling run the layer sits in. A clipped
  layer placed directly under a folder clips to nothing (folders are not a base
  layer), and clipping cannot reach down into a folder's contents. Merge down
  and flatten composite through the stack, so they keep clipping; merging a
  clipped layer down clears the flag (as Photoshop does) but the pixels are
  baked unclipped, so the merged result can differ from what was on screen.
  The *Clip to layer below* checkbox and its effect have not been looked at on
  a real screen (§8).
* Layers cannot be linked. Merging down bakes the upper layer with its own blend
  mode and opacity; if the receiving layer also has a blend mode or partial
  opacity, the merged pixels are very close to, but not bit-identical with, what
  was on screen.
* Groups are one level deep and cannot be nested, duplicated or merged; a
  group row cannot be dropped onto another group. Dragging a layer into or out
  of a group works, but a folder that is collapsed only shows the layer after
  the drop (it expands then), and a drop inside another group's block lands at
  that block's nearest edge rather than at the exact pixel the pointer left.
* Flattening discards hidden layers rather than baking them (as other editors
  do); undo restores them, but the flattened image cannot be un-flattened once
  the history entry is dropped.
* A flatten keeps no record of the old stack outside the undo history.
* The selection itself is not part of the undo history: creating, growing,
  shrinking, inverting or dropping one — including a magic-wand selection — is not
  undoable. Grow and shrink use a square window rather than a disc, and a
  selection grown past the canvas edge is clipped to it. The wand compares
  premultiplied channels, so a colour that differs mainly in transparency can
  be separated differently than it looks on screen, and a boundary too ragged to
  draw as ants (over 4000 segments) is outlined by its bounding box instead.
* Feathering is baked into the selection mask when it is created, and changing
  the slider later does not re-feather an existing selection. The soft edge is
  shown as a four-step tinted band rather than a continuous gradient, and a
  region whose band would need more than 8000 spans is left untinted. The Quick
  mask wash is a five-step gradient at 88/255 peak opacity, not a true per-pixel
  gradient, and it is clipped to a raised bounding box when it would need more
  than 20 000 spans.
* Flood fill is a hard-edged pixel mask: no antialiasing, no expand/grow, and no
  "sample this layer only" mode.
* Shapes have no Shift-constrained or centre-out modes and cannot be re-edited
  after they are committed.
* `.inkforge` stores one PNG per layer rather than tile/delta compression, so a
  many-layer document is lossless but larger on disk than a compressed PSD-style
  container would be.
* The recent-projects list remembers eight `.inkforge` projects and cannot be
  reordered or edited by hand; images opened as documents are not added to it,
  because they have no project path until they are saved as one.
* Autosave keeps one recovery slot for the single open document, every 20
  seconds; the interval is not configurable from the UI yet. Up to 20 seconds of
  work can be lost in a crash, and because each autosave PNG-encodes every layer
  it can momentarily pause the interface on very large documents (writes are
  skipped mid-stroke). Older recovery copies are overwritten rather than kept as
  a version history.
* JPEG export always uses quality 92 and composites transparency onto white;
  there is no quality control, no DPI/metadata writing and no WebP/TIFF/PSD
  output. Imported images are placed at the top-left and cropped to the canvas —
  there is no scaling, placement or transform on import.
* OpenRaster is export-only: `.ora` files cannot be opened, so a round trip
  through Krita or GIMP and back is not possible yet. Masks, live adjustments
  and clip-to-layer-below are baked into the exported layer pixels (the picture
  is right, the editable mask is not in the file), group opacity is written on
  an isolated `<stack>` rather than multiplied into each child, and no DPI
  (`xres`/`yres`) is written. The archive has been verified structurally —
  layout, CRCs, XML, pixels, thumbnail — and with Python's `zipfile`, but it has
  **not** been opened in Krita, GIMP or MyPaint (§8).
* Opening an image as a document does not associate a project path, so `Ctrl+S`
  asks for a `.inkforge` name; PNG/JPEG are export-only.
* The native file dialogs (`rfd`) could not be opened in this headless
  container, so the picker itself — only the read/write code behind it — is
  unverified until the app runs on a desktop.
* The New canvas window, the preset cards, the star buttons, the rulers and
  every guide are covered by headless egui passes and by tests of the model
  behind them, but they have **not been looked at on a screen**. Card sizing,
  the two-column fallback below 940 px, ruler label density at very small and
  very large zooms, and whether the amber trim line reads clearly over light
  artwork are the parts most likely to need a visual pass (§8).
* Only 8-bit RGB (sRGB) canvases exist. CMYK, Grayscale and 16/32-bit depth are
  not offered, because the compositing engine is 8-bit RGBA; the window states
  this instead of showing controls that would do nothing. There is no ICC
  profile selection, and PNG exports declare sRGB rather than embedding a
  profile.
* There are no perspective guides or vanishing-point assistants, and nothing
  snaps to the grid or to the print guides — they are measurement overlays.
* Rulers are hidden while the canvas is turned. The grid, trim line, safe area
  and trim marks still follow the rotation because they are drawn through the
  same page transform as the tiles.
* Bleed enlarges the created canvas (3 mm per side on A4 is 35 px per side),
  which is the print-file convention but differs from Photoshop's New Document
  dialog, where the canvas stays at trim size. Safe area is measured inwards
  from the trim line, not the canvas edge.
* The New canvas window's printed size in inches and millimetres is computed
  from the DPI and the pixel aspect; it does not read an ICC profile's intended
  display size because there is no ICC support.
* Opening a PNG reads its `pHYs` chunk (1.6.0), so an opened image keeps the
  resolution it declared; two different densities become the pixel aspect.
  JPEG still opens at 72 dpi, because the writer in this build embeds no JFIF
  density. Exporting writes `pHYs` and `sRGB`.
* The 1.6.0 Windows package was cross-compiled on Linux and verified only as a
  file: 64-bit, GUI subsystem, system-only imports. It has never been
  executed, so its GPU initialisation, window creation, fonts and file dialogs
  on Windows are all unconfirmed — as is anything about how it looks at a given
  DPI. The first launch on a Windows machine is the real test (Milestone 8).
* The executable's icon and version resource were verified by parsing the built
  PE, not by looking at Windows: the `.ico` images and the `VERSIONINFO` strings
  are present and correct, but how Explorer, the task bar and Task Manager draw
  them at a given DPI, and whether the icon reads well at 16 px on a real
  desktop, have not been seen. The About dialog shows the same 1.6.0 from the
  compiled-in crate version.
* The ZIP is unsigned. A downloaded copy will raise a SmartScreen warning on
  first run unless it is code-signed, which it is not.
* The GNU build ships instead of an MSVC build because no MSVC linker exists in
  this container. Behaviour should be identical (same crates, same wgpu backend),
  but the canonical release path remains `scripts\package-windows.ps1` on a
  Windows machine, and that path has not been run yet.
* Large dabs are split across up to eight pooled cores, but small and medium
  brushes are still single-threaded. The speed-up is about 3× on eight cores,
  not linear, and stamping remains CPU-side rather than GPU-side.
* Presets are built in. There is no preset editor and no `assets/brushes`
  persistence of the artist's own tips yet.
* Grain is pixel-level noise: it is stable per canvas position, but it does not
  follow stroke direction and there is no paper-texture support.
* The start-state card and the expanded tooltips have only been verified
  headlessly (the card's text is asserted in the rendered output, the tooltips
  are read in code). How they look and wrap at real sizes, fonts and DPI is
  unconfirmed until the app runs on a desktop.
* The start card needs roughly 180 × 90 px of visible page; on a page that is
  zoomed so small that it no longer fits, it is skipped rather than shrunk.
* Nothing has been looked at on a screen: no display and no GPU in this
  environment, so colours, icon legibility, spacing, DPI behaviour and the
  feel of the brush are unconfirmed visually. The first thing to do on the
  Windows machine is `cargo run`; wgpu driver behaviour is likewise untested
  here.

## 6. Building on Windows 11 (x86_64-pc-windows-msvc)

```powershell
# One-time toolchain setup
winget install --id Rustlang.Rustup
rustup default stable-msvc
rustup target add x86_64-pc-windows-msvc

# Debug run
cargo run

# Release build
cargo build --release --target x86_64-pc-windows-msvc
```

The executable is `target\x86_64-pc-windows-msvc\release\inkforge.exe`.
The MSVC linker comes with "Build Tools for Visual Studio" (Desktop C++
workload). Note that wgpu on Windows needs a working DX12 (or Vulkan) driver,
which any recent GPU has.

### 6a. Packaging the portable release

`scripts\package-windows.ps1` builds the MSVC release and stages

```
dist\INKFORGE-<version>-windows-x86_64\
├── INKFORGE.exe
├── README.txt          (artist-facing: features, shortcuts, requirements)
└── LICENSE-MIT.txt
```

plus `dist\INKFORGE-<version>-windows-x86_64.zip` (`Compress-Archive`). The
README text lives in `packaging/README.txt` and is shared by both packaging
scripts, so the two builds ship identical paperwork.

Both scripts produce the same executable, resources included. `build.rs` fills
in `packaging/windows/inkforge.rc.in` with the crate version and the icon's
path, compiles it to a COFF object and passes that object to the linker, so the
version Explorer shows can never drift from `Cargo.toml`. The resource compiler
is whichever one the machine has: `rc.exe` (on `PATH`, or found under
`%ProgramFiles(x86)%\Windows Kits\10\bin\<version>\x64`), `windres`,
`llvm-rc`, or zig's `rc`. Set `RC` (rc.exe-style) or `WINDRES` to override it.
The icon is drawn by `python3 scripts/make-icon.py`, which writes
`packaging/windows/inkforge.ico` plus a plain preview PNG of the same artwork.

### 6b. Cross-compiling the release from Linux

This is how 1.5.0 was actually produced here, since the container has no
Windows and no MSVC linker:

```bash
rustup target add x86_64-pc-windows-gnu
cargo install cargo-zigbuild     # zig 0.13.0 unpacked somewhere on PATH
scripts/package-windows-cross.sh
```

`cargo zigbuild --release --target x86_64-pc-windows-gnu` compiles every crate
for Windows and links with zig's bundled MinGW toolchain and its static
libunwind/libgcc. The result is a single GUI-subsystem PE32+ executable with no
sidecar DLLs.

The icon and version resource come from the same `build.rs` step, compiled by
`zig rc` (zig's drop-in `rc.exe`), so cross-compiling needs no Windows SDK.
One quirk is handled there and worth remembering: `zig rc` reads a path that
starts with `/` as a switch, so the compiler is run from Cargo's `OUT_DIR` with
bare file names rather than absolute paths. Cargo's `lto`, `codegen-units = 1` and `strip = "symbols"`
settings apply exactly as for the MSVC build.

One harmless warning is printed by the linker during this build: zig's own
`libunwind` sources redeclare `_Unwind_*` functions with a `dllexport`
attribute (13 such warnings, surfaced by Cargo as
`warning: linker stderr: ...`). It comes from the toolchain archive, not from
INKFORGE code, and the link succeeds; the MSVC build would not emit it.

The GNU and MSVC builds are not byte-identical, but both use the same wgpu
backend family and the same Rust standard library behaviour. If the artist's
machine has any doubt, the MSVC script is the canonical release path.

## 7. Next up

Milestone 7, remaining:

1. Interface refinement: **start state landed**; tooltips were written and
   exercised headlessly, but still need a look on a real screen.
2. Comfort features that need no new file formats: **canvas rotation landed**
   and so did image transforms — flipping and turning the artwork itself, with
   every layer and mask permuted losslessly. Still to come: the selection
   following a transform, and scaling/skewing a layer or a selection. Canvas
   rotation is view state: it never rotates the pixels, so saving, exporting
   and reopening are unaffected. (Dragging layers into and out of groups, the
   magic wand and layer masks landed with this item too.)
3. Wider test coverage of the UI paths added in Milestone 6 (save prompts,
   keyboard shortcuts, error dialogs) and a pass over DEVELOPMENT.md to keep it
   current.
4. A visual pass over the New canvas window: the 53 cards, the star buttons,
   the narrow two-column layout, and ruler label density at extreme zooms.
5. Higher colour depth and a real colour-management path (16-bit surfaces, ICC
   profiles, CMYK and Grayscale modes) if the artist needs print colour
   accuracy; today the window says plainly that only 8-bit RGB exists.
6. Perspective guides and snapping, which would be assistants over the
   existing viewport rather than measurement overlays.

Milestone 8, remaining:

7. Run the packaged `INKFORGE.exe` on a clean Windows 10/11 machine (no Rust, no
   Visual C++ redistributable) and on the artist's machine; confirm the window
   opens, the GPU path initialises, painting feels right and saving/opening a
   project works by hand. Nothing in §5 covers this.
8. Code-signing, if the artist intends to distribute the ZIP publicly;
   Windows SmartScreen will otherwise warn on first run of an unsigned
   download.
9. Connect a drawing tablet to the Windows build: a stroke should thin with
   pressure and widen with tilt, the sidebar should show a live reading, and the
   pen should still drive every tool through the mouse messages. This is
   unverified hardware code (§5, §8 limitations).

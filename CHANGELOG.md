# Changelog

Everything that shipped, newest first. Versions 1.0.0 to 1.4.0 were released as
portable Windows ZIPs; 1.5.0 ("Beta 3") is the last published release. 1.6.0 is
prepared and has not been published yet — see [RELEASING.md](RELEASING.md).

## 1.6.0 — prepared, not yet published

The release that finishes the editing half of the app: selections became
undoable and combinable, the page can be resized or cropped after it was
created, and a turning document keeps its selection.

### Selections

- Every selection change is now an undo step: rectangle, freehand lasso, magic
  wand, Select all, Deselect, Invert, Grow/Shrink and Quick mask strokes. A
  drag or a slider movement collapses into one entry, and a change that picks
  the same pixels records nothing.
- New **polygon lasso**: click each corner, then click the first corner (or
  press Enter) to close it; Escape drops a half-drawn one, and a live rubber
  band follows the pointer.
- New **combine modes** — New, Add, Subtract, Intersect — work with every
  selection tool, including the wand and both lassos. Coverage is combined per
  pixel, so feathered edges meet softly instead of being clipped.
- A selection now travels with the artwork when a document is flipped or turned:
  a quarter turn re-describes it in the new coordinates, and undo carries it
  back. Feathered edges survive the move.

### Canvas

- New **Canvas size…** window (Image menu): change width, height, resolution,
  bleed and safe area on an existing document, with a nine-point anchor grid
  that decides where the artwork sits when the page grows or shrinks. Nothing is
  resampled.
- **Crop to selection** and **Trim transparent edges** join it in the Image
  menu. Both share the same lossless path and are one undo step each.
- Canvas resizes, print setup edits and canvas background changes are all
  undoable, and a colour-picker drag collapses into a single step.

### Files and workspace

- **PNG resolution is read on open**: a 300 dpi proof reopens at 300 dpi, and
  two different densities become the pixel aspect. JPEG still opens at 72 dpi.
- **Workspace preferences persist** in `settings.json`: panel visibility, ruler
  unit, rulers, grid, guides, print marks and grid spacing survive a restart. A
  missing, damaged or read-only file falls back to defaults silently.
- Fixed a history bug found while testing: a setting changed right after an undo
  could merge into the edit undo had just restored. History now closes merging
  across undo and redo.

### Project

- Added `.github/workflows/ci.yml` (`cargo check` and `cargo test` on Ubuntu
  with the window libraries installed). It has not run yet, because nothing has
  been pushed.

## 1.5.0 — Beta 3

- Canvas preset system: a New canvas window with 53 presets in seven categories
  (illustration, comic and manga, print, photography, animation, game art,
  design/UI), each showing proportions, pixels, aspect ratio, megapixels and
  resolution before you commit.
- Every setting editable: size in pixels, millimetres, centimetres or inches;
  DPI; orientation; aspect-ratio lock and quick ratios; pixel aspect; white,
  black, transparent or custom background; bleed and safe-area margins; an
  optional starting sketch layer.
- Presets can be searched, filtered, sorted, starred, saved, edited, deleted,
  imported and exported as JSON, or reset to the defaults.
- Print setup that reaches the file: bleed added to the canvas, trim and
  safe-area guides, DPI written into exported PNGs with the sRGB declaration.
- Rulers in pixels, millimetres, centimetres or inches, a configurable grid, and
  trim / bleed / safe-area outlines.
- Canvas rotation: Shift-drag with the Pan tool, 15° steps in the View menu, and
  a click-to-straighten angle in the status bar.

## 1.4.0

- Layered OpenRaster (`.ora`) export that Krita, GIMP and MyPaint can open.
- Non-destructive layer masks with a project format version 2 (version 1
  projects still open).

## 1.3.0

- 43 built-in brush presets in six shelves.
- Live per-layer colour adjustment (brightness, contrast, saturation, hue) that
  never rewrites pixels.
- Whole-document flips and 90°/180° rotations, lossless and undoable.

## 1.2.0

- The full W3C blend-mode set (17 modes).
- Clip to layer below (alpha inheritance).

## 1.1.0

- Expanded blend engine and the first packaged Windows build.

## 1.0.0

- The editor workspace: GPU canvas, brush and eraser, eyedropper, layers and
  groups, undo/redo, `.inkforge` projects, PNG/JPEG import and export, crash
  recovery, canvas rotation and large-canvas tiling.

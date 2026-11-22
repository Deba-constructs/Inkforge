# INKFORGE 1.6.0

The release that finishes the editing half of the app. Selections became
undoable and combinable, the page can be resized or cropped after it was
created, and a flipping or turning document keeps its selection.

**Download:** `INKFORGE-1.6.0-windows-x86_64.zip` (5.6 MB, 5 800 579 bytes)

- SHA-256: `848f54f427ac14e7b43d3c0c942069181fdc5fc0306c82c10a198f57b907eab0`
- 64-bit Windows 10/11, portable: unzip it, double-click `INKFORGE.exe`.
  Nothing to install, no Rust and no Visual C++ redistributable needed.

## Selections

- Every selection change is an undo step now: rectangle, freehand lasso, magic
  wand, Select all, Deselect, Invert, Grow/Shrink and Quick mask strokes. A drag
  or a slider movement collapses into one entry, and a selection that picks the
  same pixels records nothing.
- **Polygon lasso**: click each corner, then click the first corner (or press
  Enter) to close it. Escape drops a half-drawn one, and a rubber band follows
  the pointer so the next corner is easy to place.
- **New / Add / Subtract / Intersect** combine modes, with every selection tool
  including the wand and both lassos. Coverage is combined per pixel, so
  feathered edges meet softly instead of being clipped, and the result is
  trimmed to what survived.
- A selection **travels with the artwork** when a document is flipped or turned.
  A quarter turn re-describes it in the new coordinates and undo carries it
  back; feathered edges survive the move.

## Canvas

- **Canvas size…** in the Image menu: edit width, height, resolution, bleed and
  safe area of a document that already exists, with a nine-point anchor grid
  deciding where the artwork sits when the page grows or shrinks. Nothing is
  resampled.
- **Crop to selection** and **Trim transparent edges** join it. Both use the same
  lossless path and are one undo step each.
- Canvas resizes, print setup edits and background colour changes are all
  undoable; a colour-picker drag collapses into a single step.

## Files and workspace

- **PNG resolution is read on open**: a 300 dpi proof reopens at 300 dpi, and two
  different densities become the pixel aspect. (JPEG still opens at 72 dpi.)
- **Workspace preferences persist** in `settings.json`: panel visibility, ruler
  unit, rulers, grid, guides, print marks and grid spacing survive a restart. A
  missing, damaged or read-only file falls back to defaults without interrupting
  painting.
- Fixed a history bug found while testing: a setting changed right after an undo
  could merge into the edit undo had just restored. History now closes merging
  across undo and redo.

## Honest limitations

- There is still no free transform of a layer or selection: the whole document
  can be flipped and turned, but a layer cannot be scaled, rotated to an
  arbitrary angle, skewed or put into perspective.
- A named selection cannot be saved and recalled later.
- Resizing the canvas clears the selection, because it is described in the old
  pixel coordinates.
- Only 8-bit RGB (sRGB) is supported; there is no CMYK, Grayscale, 16/32-bit
  depth or ICC profile support.
- The executable is unsigned, so SmartScreen warns on the first run.
- This build was compiled on Linux. It has not been launched on Windows, never
  seen on a real screen and never driven by a drawing tablet. The interface
  layout, GPU start-up, fonts, native dialogs and pen feel are all unconfirmed;
  please report what looks or feels wrong.

Full detail is in `CHANGELOG.md` and `DEVELOPMENT.md` in the repository.

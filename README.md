# INKFORGE

[![Latest release](https://img.shields.io/github/v/release/Deba-constructs/Inkforge?label=release)](https://github.com/Deba-constructs/Inkforge/releases/latest)
[![Licence: MIT](https://img.shields.io/badge/licence-MIT-blue.svg)](#licence)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

A desktop digital painting application for artists, written in Rust with
`egui`/`eframe` and a GPU (wgpu) renderer.

**Download for Windows** — the [1.5.0 release](https://github.com/Deba-constructs/Inkforge/releases/latest)
is a portable ZIP: unzip it, double-click `INKFORGE.exe`, nothing to install
and no runtime to set up.

INKFORGE is built milestone by milestone. This repository currently contains
**Milestones 1 to 6 plus most of Milestone 7**: the full editor workspace, a working painting engine
(freehand strokes with real colour, size, opacity, hardness, spacing, grain and
speed-adaptive stabilisation), an eraser, an eyedropper, 43 built-in brush
presets in six shelves, a real layer stack with groups, visibility, opacity,
reordering, 17 blending modes, live colour adjustments, per-layer masks that
hide and reveal artwork without deleting it, clipping to the layer below and a
per-layer transparency lock, flips and rotations of the whole document, rectangle, freehand and magic-wand selections with feathering that clip
every edit, a flood fill, line / rectangle / ellipse shapes, undo/redo covering all of
it, and real file I/O: `.inkforge` projects that keep every layer, PNG and JPEG
opening, importing and export, layered OpenRaster (`.ora`) export that Krita,
GIMP and MyPaint open, a window title that tracks the document and
its unsaved state, a recent-projects menu, GPU uploads that copy only the part
of the canvas that changed, canvas rotation that turns the sheet like a drawing
board, and large brushes stamped across several CPU cores.
Version 1.5.0 adds the **canvas preset system**: a New canvas window with 53
presets across illustration, comic and manga, print, photography, animation,
game art and UI, every setting editable down to the last margin, presets you can
save, favourite, search and share, plus real print setup — bleed included in the
canvas, safe-area and trim guides, rulers in pixels or millimetres, and DPI that
reaches the exported PNG.
Version 1.5.0 has been built into a
portable Windows executable and packaged as a downloadable ZIP (see
[Release package](#release-package)). See
[DEVELOPMENT.md](DEVELOPMENT.md) for the exact state of every feature.

## Current status

| Area | State |
| --- | --- |
| Native window, dark charcoal theme, custom vector icons | Working |
| Top bar: File / Edit / Image / View / Help menus, brush size + opacity, colour, undo/redo buttons | Working (menu entries for unimplemented features are disabled and labelled) |
| Left tool rail: brush, eraser, colour picker, fill, selection, shapes, pan | Working; every tool is wired to the canvas |
| Painting: freehand strokes, size, colour, opacity, hardness, spacing, grain, speed-adaptive stabiliser, eraser | Working |
| Brush presets: 43 brushes in six shelves (General, Inking, Sketching, Painting, Watercolour, Effects) — Round, Ink Pen, G-Pen, Technical Pen, Pencil, Charcoal, Chalk, Flat Bristle, Palette Knife, Airbrush, Watercolour, Splatter and more | Working; every preset is a full parameter set, so they differ in size, edge softness, opacity, spacing, grain and smoothing rather than in name only, the list scrolls in its own shelf, and the active one is highlighted |
| Drawing tablet: pen pressure drives dab width and opacity, tilt fans the tip out, with three amounts in the sidebar | Working in the Windows build, and the mapping from a reading to a dab is covered by tests; the tablet path itself has not been tried with a real pen (see [DEVELOPMENT.md](DEVELOPMENT.md) limitations) |
| Eyedropper | Working (samples the artwork, falls back to the canvas background) |
| Undo / redo of strokes, erases and layer changes (`Ctrl+Z`, `Ctrl+Y`, `Ctrl+Shift+Z`) | Working; history is capped at 256 MB, drops the oldest edits, and moves layers between the document and history instead of copying them |
| Layers: add, delete, reorder (buttons or dragging rows), duplicate, merge down, flatten, rename (double-click a row), show/hide, opacity, 17 blend modes, Clip to layer below and Lock transparent pixels | Working; rows show a live thumbnail (and a padlock when locked), painting always lands on the selected layer and strokes are clipped to it, and on a locked layer paint only recolours pixels that already exist — the eraser cannot take paint away |
| Layer masks: add a mask that reveals or hides the whole layer, paint it with the brush (reveal) and eraser (hide), invert it, delete it | Working; the mask is a separate coverage buffer, so no pixel is ever destroyed: delete the mask and the layer is exactly as it was. A half-filled mask badge marks the row, mask editing is a checkbox in the layer panel, and adding, painting, inverting and deleting all undo. Fill and shapes deliberately refuse while mask editing is on, with a message pointing at the brush |
| Colour adjustment per layer: brightness, contrast, saturation and hue sliders with Reset | Working; applied live while the layer is composited, so the pixels are never rewritten, the sliders undo and redo like any other layer property, and the values travel in the project file |
| Image transforms: flip horizontal / vertical and rotate 90° clockwise / counter-clockwise / 180° | Working from the Image menu; every layer is permuted losslessly (nothing is resampled), a quarter turn swaps the canvas dimensions, and undo applies the opposite transform rather than storing a copy of the canvas — any selection is cleared, because it was described in the old coordinates |
| Layer groups: group a layer, add more layers into it, collapse/expand, double-click to rename, group visibility and opacity | Working; group opacity and visibility apply to every layer inside, an empty folder shows where to drag a layer, and painting needs a layer selected rather than the group |
| Drag layers into and out of groups | Working; drop a layer on a folder row to put it inside, or on a top-level row to take it out, and the move (position and group) undoes in one step |
| Selection: rectangular marquee, freehand lasso and magic wand (tolerance slider) with optional feathering, grow / shrink by dragging a slider, invert, Quick mask view, dashed outline, select all, deselect | Working; painting, erasing, filling and shapes are all clipped to the selection, soft edges included; the wand selects the connected area of similar colour under the click and outlines its real boundary, and dragging the live Grow / shrink slider moves the selection edge (right grows, left shrinks) and Invert selects everything outside the current selection, all without touching the artwork; the partly-selected part of a feathered edge is tinted on the canvas so you can see how soft it is while you paint, and Quick mask washes the whole selected area in translucent red and is editable: with it on, the brush paints into the selection and the eraser takes parts of it away, including outside the old selection |
| Fill: flood fill sampling the visible image, tolerance slider, fill selection | Working; one fill is one undo step |
| Shapes: line, rectangle and ellipse drawn with the current brush, with a live preview | Working; committed as a single undo step |
| Canvas viewport: page rendering, composited layers, white / transparent / custom background, zoom at cursor, pan, fit, 100% | Working |
| Canvas rotation: turn the view to any angle (Shift+drag with the Pan tool), snapping to upright near 0° / 90° / 180°, 15° steps in the View menu, angle readout in the status bar that straightens the canvas when clicked | Working; painting, selections and every overlay follow the angle, and the maths is covered by tests, but the turned rendering has not been looked at on a screen (see [DEVELOPMENT.md](DEVELOPMENT.md) limitations) |
| Right sidebar: colour picker + palette, preset list, brush sliders, canvas info, layer manager | Working |
| Bottom status bar: tool, status message, zoom controls, canvas dimensions | Working |
| Start state on an untouched canvas | Working; a small card names the first three things to try and disappears as soon as there is a painted pixel — it comes back if you undo the only stroke |
| Tooltips | Working; every tool, brush slider, fill/selection option and layer button names its shortcut and explains what it changes |
| New canvas window: 53 presets in seven categories, search, category filter, favourites, recents, sorting by name / size / category / recently used, dimension and aspect readout on every card | Working; each category holds exactly the documented sizes — illustration 7, comic & manga 7, print 11 (A0–A6 and US Letter/Legal/Tabloid/Poster, resolved from millimetres or inches at 300 dpi), photography 9, animation 6, game art 7, design & UI 6. Cards draw a sheet in the preset's real proportions, clicking one fills every field, and arrow keys / Enter work without a mouse |
| Canvas settings: width and height in pixels, millimetres, centimetres or inches; DPI; portrait/landscape switch; aspect-ratio lock with 1:1, 4:5, 3:4, 2:3, 16:9 and 9:16 shortcuts; pixel aspect ratio; white, black, transparent or custom background; bleed and safe-area margins; optional starting sketch layer | Working; conversions round to whole pixels exactly (A4 is 2480 × 3508 at 300 dpi, US Letter 2550 × 3300), the canvas size after bleed is shown, and every value reaches the created document and the saved project |
| Preset management: save, edit, delete, favourite, search, filter, sort, restore defaults, import and export | Working; the library lives in `canvas-presets.json` in INKFORGE's data folder, a damaged or hand-edited file falls back to the built-ins instead of failing, and an imported preset whose id already exists is stored alongside rather than overwriting it |
| Validation and allocation guard | Working; a size must be 16–20 000 px per side and at most 200 megapixels, checked *before* any buffer is allocated, so an impossible request is a readable message rather than a frozen machine. The limits cover real pages: a webtoon is 800 × 12 800, an A0 at 300 dpi 9933 × 14 043, a manga spread 7280 × 5150 |
| Print metadata | Working; PNG export writes `pHYs` (the DPI, with separate horizontal and vertical densities when the pixel aspect is not square) and `sRGB` chunks, and `.inkforge` projects (format version 3) keep the resolution, pixel aspect, bleed and safe area |
| Rulers, grid and guides | Working; View ▸ Guides turns rulers in pixels / mm / cm / inches, a configurable grid, trim marks and bleed/safe-area outlines on and off. Rulers read the same viewport maths as the pointer and hide themselves while the canvas is turned |
| Keyboard shortcuts, About and Shortcuts windows | Working |
| File I/O: `.inkforge` projects (layers, groups, blends, all of it), PNG/JPEG open and import, PNG/JPEG export of the flattened image, and OpenRaster (`.ora`) export of the whole layer stack | Working; Save / Save as / Open with `Ctrl+S`, `Ctrl+Shift+S`, `Ctrl+O`, and an unsaved-changes prompt before anything replaces your work. The `.ora` file is checked by reading the archive back (structure, CRCs, layer tree, pixels); it has not been opened in Krita or GIMP yet |
| Autosave and crash recovery | Working; unsaved work is written to a recovery copy every 20 seconds and offered back the next time the app starts |
| Window title | Working; shows the document name and a bullet while there are unsaved changes |
| Recent projects | Working; File ▸ Open recent lists the last eight `.inkforge` projects, newest first, and forgets files that have gone |
| Painting performance | Working; only the changed rectangle of the composite is re-uploaded to the GPU, and large brush dabs are split across up to eight CPU cores |
| Large-document panning and zooming | Working; the composite is drawn from a 256-texel tile pyramid, so an 8192 × 8192 document pans at 0.65 ms and zooms at 0.45 ms per frame on average (measured, release build) |

Nothing in the UI pretends to work: controls that have no implementation are
disabled and carry a tooltip saying which milestone delivers them.

## What is missing

Version 1.5.0 is a genuinely usable painting app rather than a finished one.
These are the gaps, and the table above does not paper over them:

* **No image transform.** A layer or selection cannot be scaled, flipped,
  skewed or rotated as artwork — canvas rotation turns the view, not the paint.
* **Masks are layer-only.** A layer can have one mask (reveal/hide, painted
  with the brush and eraser), but groups cannot, a mask cannot be moved to
  another layer on its own, and there is no mask density or feather control.
  Adjustment layers with curves or levels are not implemented.
* **The project format is version 3.** Version 2 added masks and version 3 the
  print setup, so older builds refuse a newer file with a clear "made by a
  newer INKFORGE" message instead of misreading it. Version 1 and 2 projects
  still open in 1.5.0, at 72 dpi with no bleed.
* **Only 8-bit RGB is supported.** CMYK, Grayscale and 16/32-bit canvases are
  not offered at all — the engine composites 8-bit sRGB, and pretending
  otherwise would make the depth and mode fields lies. There is no ICC
  profile selection; exported PNGs declare sRGB.
* **No perspective guides.** The grid and the print guides are visual aids;
  there are no vanishing-point assistants and nothing snaps to a guide.
* **Rulers disappear while the canvas is turned**, because there is no upright
  axis to measure along at an angle.
* **Bleed enlarges the canvas.** A print preset with 3 mm bleed becomes
  70 px wider and taller than its trim size, with the trim line drawn inside;
  that is the printable-file convention, but it is not what Photoshop's New
  Document dialog does, so it is stated here rather than implied.
* **OpenRaster is export-only.** `.ora` files cannot be opened, and the format
  has no portable attribute for masks, live colour adjustments or clip to layer
  below, so those are baked into each exported layer's pixels — the picture
  looks the same, but the mask and the adjustment are no longer editable in the
  app that opens it. Group opacity is written as a stack attribute over an
  isolated group, a slight difference from INKFORGE's per-child group opacity
  when a group is partly transparent.
* **The Windows executable is unsigned**, so SmartScreen warns the first time it
  runs. Choose *More info ▸ Run anyway*, or build it yourself.
* **The pen path has not met a real tablet.** Pressure and tilt are implemented
  and covered by tests, but only a mouse has driven the released build.

## Building

Requires a Rust toolchain (1.85 or newer).

```bash
# Debug run
cargo run

# Tests
cargo test

# Optimised Windows release build
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
```

The Windows binary lands in
`target/x86_64-pc-windows-msvc/release/inkforge.exe`. It is self-contained:
the Rust runtime is statically linked, so end users need no Rust installation.

### Release package

`1.5.0` is published on the [releases page](https://github.com/Deba-constructs/Inkforge/releases/latest)
as `INKFORGE-1.5.0-windows-x86_64.zip` (5.5 MB, 5 777 776 bytes). It holds
`INKFORGE.exe` (12.6 MB, 13 228 544 bytes, 64-bit Windows GUI executable), a
`README.txt` for the artist and the licence, and the release notes list its
SHA-256. Nothing needs installing: unzip it and run the executable. The two
scripts below rebuild the same ZIP locally into `dist/`.

```bash
# On Windows, with the MSVC toolchain:
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1

# On Linux, cross-compiled with cargo-zigbuild (how the 1.5.0 ZIP was produced):
rustup target add x86_64-pc-windows-gnu
cargo install cargo-zigbuild    # needs `zig` on PATH
scripts/package-windows-cross.sh
```

Both scripts write the same layout: a portable folder under `dist/` and its
`.zip`. The executable imports only DLLs that ship with Windows 10 and 11
(`kernel32`, `user32`, `d3d`/`dxgi`, `dwmapi`, the `api-ms-win-crt-*` runtime),
so no Visual C++ redistributable or MinGW runtime is needed. The executable
carries its own icon (seven sizes, 16 to 256 px) and version metadata, so
Explorer, the task bar and Task Manager show **INKFORGE 1.5.0** instead of a
generic binary.

Both packaging scripts go through `build.rs`, which compiles
`packaging/windows/inkforge.rc.in` with whichever resource compiler the machine
has — the Windows SDK's `rc.exe`, `windres`, `llvm-rc` or zig's `rc` — so a
Windows build needs no extra manual step. The icon itself is drawn by
`scripts/make-icon.py`; rerun it to regenerate `packaging/windows/inkforge.ico`
after changing the artwork.

On Linux the app builds with the default system libraries used by `winit`
(X11 or Wayland). On Windows nothing beyond the executable is required.

## Keyboard shortcuts

| Keys | Action |
| --- | --- |
| `B` `E` `I` `G` `M` `U` `H` | Brush, eraser, colour picker, fill, selection, shapes, pan |
| Mouse wheel | Zoom at the cursor |
| Middle drag (or Hand tool + drag) | Pan the canvas |
| Shift + drag with the Hand tool | Turn the canvas; it snaps back to upright near 0° / 90° / 180° |
| `0` / `1` | Fit canvas to window / actual size |
| `+` / `-` | Zoom in / out |
| `[` / `]` | Decrease / increase brush size |
| `Tab` | Show or hide the side panels |
| `Ctrl+Z` / `Ctrl+Y` / `Ctrl+Shift+Z` | Undo / redo the last edit |
| `Ctrl+A` / `Ctrl+D` | Select the whole canvas / deselect |
| `Q` | Show or hide the Quick mask wash — with it on, the brush adds to the selection and the eraser removes from it |
| `Ctrl+N` | New canvas |
| `Esc` | Close the current dialog, or deselect |
| `Ctrl+S` / `Ctrl+Shift+S` | Save the project / save it under a new name |
| `Ctrl+O` | Open a `.inkforge` project |
| `Ctrl+Q` | Quit |

## Layout of the source

```
src/
├── main.rs            window setup only
├── lib.rs             library root (lets tests/ use the app modules)
├── app.rs             application state, tools, shortcuts, dialogs
├── engine/
│   ├── canvas.rs      document model: size, background, layer operations
│   ├── brush.rs       brush parameters and resolved dabs
│   ├── pen.rs         pressure and tilt turned into a dab size and opacity
│   ├── selection.rs   marquee/lasso/wand selections, feathering, clipping maths
│   ├── fill.rs        scanline flood fill
│   ├── shape.rs       line / rectangle / ellipse outlines
│   ├── surface.rs     premultiplied RGBA pixels, stamping, erasing, sampling
│   ├── transform.rs   document flips and rotations as lossless pixel permutations
│   ├── presets.rs     the brush library: 43 presets in six shelves
│   ├── stabilizer.rs  speed-adaptive stroke smoothing
│   ├── pyramid.rs     composite tile pyramid: levels, tiles and uploads
│   ├── renderer.rs    layer thumbnail textures
│   └── viewport.rs    zoom / pan / rotation transform maths
├── input/
│   ├── mod.rs         one place to ask "what is the pen doing right now"
│   ├── pointer.rs     the two Win32 pointer structures, with layout tests
│   └── windows.rs     WM_POINTER hook that reads pressure and tilt
├── layers/
│   ├── layer.rs       one layer, its properties and the blend maths
│   ├── group.rs       layer groups and their properties
│   └── stack.rs       entries, groups, ordering and the cached composite
├── history/
│   └── undo.rs        tile-based undo/redo with a memory cap
├── io/
│   ├── project.rs     the .inkforge container: JSON manifest + PNG layers and masks
│   ├── raster.rs      PNG/JPEG decode, flatten-and-export, image import
│   ├── png.rs         PNG encoding with pHYs (DPI) and sRGB chunks
│   ├── canvas_presets.rs  preset catalog, units, canvas setup, preset library
│   ├── ora.rs         OpenRaster export: stack.xml, PNG streams, stored ZIP
│   ├── recent.rs      the persisted recent-projects list
│   └── recovery.rs    autosave slot, sidecar metadata, crash recovery
└── ui/
    ├── theme.rs       charcoal palette and widget styling
    ├── icons.rs       procedurally drawn tool icons and brand mark
    ├── top_bar.rs     brand, menus, brush/colour controls, history
    ├── tool_rail.rs   left tool selection
    ├── sidebar.rs     colour, brush, canvas and layer panels
    ├── canvas_view.rs page rendering, zoom and pan, rulers, grid and guides
    ├── new_canvas.rs  the canvas preset browser and setup window
    ├── status_bar.rs  status message and zoom controls
    └── dialogs.rs     unsaved changes, recovery, file errors, about, shortcuts
tests/
├── model.rs           canvas validation and viewport maths tests
├── painting.rs        brush stamping, opacity, erasing and sampling tests
├── history.rs         undo/redo, redo invalidation, history memory cap
├── layers.rs          layer stack, compositing, blending, layer commands
├── locking.rs         Lock transparent pixels: recolour without changing alpha
├── editing.rs         selection clipping, lasso/feather/wand, flood fill, shapes
├── presets.rs         preset round-tripping, 43 distinct marks, spacing, grain
├── adjustments.rs     brightness/contrast/saturation/hue, undo, save, merge
├── transform.rs       flip/rotate pixel mapping, lossless inverse, save, undo
├── masks.rs           layer masks: compositing, painting, invert, undo, save
├── stabilizer.rs      smoothing, lag limit, pass-through
├── pen.rs             pressure/tilt mapping, mouse fallback, bad readings
├── io.rs              project round trips, corrupt files, PNG/JPEG export and import
├── ora.rs             OpenRaster archive layout, layer tree, baked masks and adjustments
├── canvas_presets.rs  the 53-preset catalog, units, validation, library, PNG print metadata
├── recovery.rs        autosave ticking, crash recovery, discard and keep-for-later
├── recent.rs          recent-project ordering, cap, pruning and reopening
├── tiles.rs           composite tile pyramid: levels, uploads, downscaling
├── performance.rs     dirty-region tracking and large-dab threading
└── ui_smoke.rs        headless render + simulated pointer painting
```

Release tooling lives outside `src/`: `build.rs` (embeds the icon and version
resource in Windows builds), `scripts/package-windows.ps1` (MSVC build and ZIP
on Windows), `scripts/package-windows-cross.sh` (the same package,
cross-compiled from Linux), `scripts/make-icon.py` (draws the icon),
`packaging/windows/` (the `.ico` and the `VERSIONINFO` template) and
`packaging/README.txt` (the read-me shipped inside the ZIP).

## Licence

MIT — see [`LICENSE-MIT`](LICENSE-MIT).

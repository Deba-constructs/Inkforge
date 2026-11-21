INKFORGE — a desktop painting application for artists
=====================================================

There is nothing to install. Double-click INKFORGE.exe and start painting.
The program is a single self-contained executable: it does not need Rust, a
Visual C++ redistributable or any other runtime, and it writes nothing outside
the files you save (plus a small autosave folder, below).


What it does
------------

* Freehand brush and eraser with size, opacity, hardness, spacing, grain and
  speed-adaptive stroke smoothing, plus 43 built-in brush presets in six
  shelves: General, Inking (G-Pen, Technical Pen, Fineliner, Calligraphy),
  Sketching (pencils, charcoal, chalk, pastel, crayon, marker), Painting
  (bristle, filbert, palette knife, impasto, airbrush), Watercolour and Effects
  (spray, splatter, stipple, smoke). The preset list scrolls in its own shelf.
* Drawing-tablet pen pressure and tilt: pressure sets how wide and how solid
  each dab is and tilt fans the tip out, with the three amounts in the Drawing
  tablet panel. A mouse, or a pen lifted off the tablet, paints at full
  pressure, so nothing changes if you do not have one.
* Layers with visibility, opacity, 17 blend modes, reordering, renaming,
  duplication, merge down, flatten, and one level of layer folders that layers
  can be dragged into and out of.
* Lock transparent pixels on a layer: paint then only recolours the pixels that
  are already there, so a line drawing can be coloured without spilling. The
  eraser also does nothing on a locked layer, because its alpha is frozen.
* Colour adjustment on any layer: brightness, contrast, saturation and hue
  sliders, with a Reset button. They change how the layer is composited, never
  its pixels, so dragging them back to neutral restores the original exactly;
  the values are saved in the project.
* Flip the whole document horizontally or vertically, or rotate it 90° either
  way or 180°, from the Image menu. Every layer turns with it, nothing is
  resampled, and undo puts it back the way it was.
* Clip a layer to the one below it (alpha inheritance): the layer keeps its
  colours, opacity and blend mode but can only show where the layer underneath
  has paint, so a multiply shadow stays inside the flat colour it belongs to.
* Selections: rectangular marquee, freehand lasso and magic wand (colour
  tolerance), with feathering, grow / shrink, invert, and a Quick mask view
  that you can paint into with the brush and erase out of with the eraser.
  Painting, erasing, filling and shapes are all clipped to the selection.
* Flood fill, and line / rectangle / ellipse shapes drawn with the current
  brush.
* Undo and redo across painting, erasing, filling, shapes, layers and groups.
* Files: .inkforge projects keep every layer, folder and property; PNG and JPEG
  open, import and export. Your work is autosaved to a recovery copy every 20
  seconds while it has unsaved changes, and offered back if the app ever
  closes unexpectedly.
* Pan and zoom that stay quick on very large canvases (an 8192 x 8192
  document pans at about 0.7 ms per frame).
* Canvas rotation: hold Shift and drag with the Pan tool (H) to turn the sheet
  to any angle, like turning paper on a drawing board. The artwork itself is
  not touched, and the angle snaps back to upright near 0, 90 and 180 degrees.
  The View menu turns it in 15 degree steps or straightens it, and the angle in
  the bottom bar straightens it when clicked.


First steps
-----------

1. Paint on the page with the left mouse button. `[` and `]` change the brush
   size, and the right sidebar holds the brush, colour and layer panels.
2. Scroll to zoom at the cursor; drag with the middle mouse button to pan,
   and hold Shift while dragging with the Pan tool to turn the canvas.
3. Use File > Save as (Ctrl+Shift+S) to keep your work as an .inkforge project,
   and File > Export to write a PNG or JPEG.


Keyboard shortcuts
------------------

  B E I G M U H   Brush, eraser, colour picker, fill, selection, shapes, pan
  Mouse wheel     Zoom at the cursor
  Middle drag      Pan the canvas
  Shift + drag    Turn the canvas (Pan tool)
  0 / 1           Fit canvas to the window / actual size
  + / -           Zoom in / out
  [ / ]           Smaller / larger brush
  Tab             Show or hide the side panels
  Ctrl+Z          Undo
  Ctrl+Y          Redo (Ctrl+Shift+Z also works)
  Ctrl+A          Select the whole canvas
  Ctrl+D          Deselect
  Q               Quick mask on / off — the brush adds to the selection,
                  the eraser removes from it
  Ctrl+N          New canvas
  Ctrl+S          Save the project
  Ctrl+Shift+S    Save the project under a new name
  Ctrl+O          Open an .inkforge project
  Esc             Close the current dialog, or deselect
  Ctrl+Q          Quit


Requirements
------------

Windows 10 or 11, 64-bit. The interface is drawn on the GPU, so a machine with
a working Direct3D 12 or Vulkan driver is needed; any graphics hardware from
the last several years has one. On a machine without a usable driver the window
cannot create its rendering surface and will not start.

Autosave lives in %APPDATA%\INKFORGE\autosave. Deleting that folder only
discards a crash-recovery copy, never a file you saved.


Known limitations
-----------------

These are the honest edges of this build; they are listed in full in
DEVELOPMENT.md in the source repository.

* Selections (including Quick mask painting) and background colour changes are
  not part of undo history.
* Undo history is capped at 256 MB and drops the oldest edits once full.
* There is no image transform (scale, rotate, skew, flip).
* Pen pressure and tilt are read from Windows pointer messages and have not
  been tried with a real tablet yet; the Drawing tablet panel shows the live
  reading, so you can see whether your pen is reporting pressure and tilt.
* Feathering is baked in when a selection is made; changing the slider later
  does not re-feather it.
* Flood fill is a hard-edged pixel mask with no anti-aliasing.
* Layer folders cannot be nested, merged or duplicated.
* JPEG export is fixed at quality 92 and composites transparency onto white.
* Presets are built in; there is no preset editor.
* This build was compiled and cross-compiled on Linux; it has not been launched
  on Windows or looked at on a real screen yet, so how the interface looks at
  your DPI and how the brush feels are still unconfirmed. Please report what
  feels wrong.


Licence
-------

INKFORGE is released under the MIT licence (see LICENSE-MIT.txt).

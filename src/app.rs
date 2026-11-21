//! Application state and wiring.
//!
//! `InkforgeApp` owns the document, the tool selection and the UI flags; the
//! `ui` modules only read and mutate this state, so everything the user does is
//! a real state change rather than a painted mock-up.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use inkforge::egui::{Color32, Context, Key, Modifiers, Pos2, ViewportCommand};

use crate::engine::{
    Background, BrushDab, BrushSettings, Canvas, PenSample, PenSettings, PixelRegion, Selection,
    SelectionMode, ShapeKind, Stabilizer, ThumbnailCache, TilePyramid, Viewport, MAX_FEATHER,
};
use crate::history::{Edit, History, Recorder};
use crate::io::{self, recent, recovery, ImageFormat, RecoveryInfo};
use crate::layers::{Adjustment, BlendMode, Entry, GroupProps, LayerId, LayerProps};
use crate::ui;

/// The painting tools in the left rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
    Eyedropper,
    Fill,
    Selection,
    Shape,
    Hand,
}

impl Tool {
    /// Rail order, top to bottom.
    pub const ALL: [Self; 7] = [
        Self::Brush,
        Self::Eraser,
        Self::Eyedropper,
        Self::Fill,
        Self::Selection,
        Self::Shape,
        Self::Hand,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Brush => "Brush",
            Self::Eraser => "Eraser",
            Self::Eyedropper => "Colour picker",
            Self::Fill => "Fill",
            Self::Selection => "Selection",
            Self::Shape => "Shapes",
            Self::Hand => "Pan",
        }
    }

    /// Single-key shortcut, shown in tooltips and the shortcuts window.
    pub fn shortcut_label(self) -> &'static str {
        match self {
            Self::Brush => "B",
            Self::Eraser => "E",
            Self::Eyedropper => "I",
            Self::Fill => "G",
            Self::Selection => "M",
            Self::Shape => "U",
            Self::Hand => "H",
        }
    }

    /// Key that activates the tool.
    pub fn shortcut(self) -> Key {
        match self {
            Self::Brush => Key::B,
            Self::Eraser => Key::E,
            Self::Eyedropper => Key::I,
            Self::Fill => Key::G,
            Self::Selection => Key::M,
            Self::Shape => Key::U,
            Self::Hand => Key::H,
        }
    }

    /// Whether the tool edits pixels. Pan only moves the view, so its status
    /// message says that instead of claiming a selection.
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Hand)
    }

    /// One line describing what the tool does, shown under its name in the
    /// tooltip so the rail teaches the tool instead of only naming it.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Brush => "Paint freehand strokes with the current tip.",
            Self::Eraser => "Remove paint from the active layer.",
            Self::Eyedropper => "Pick a colour from the artwork.",
            Self::Fill => "Flood-fill the area you click.",
            Self::Selection => "Select an area; every edit is clipped to it.",
            Self::Shape => "Draw a line, rectangle or ellipse.",
            Self::Hand => "Drag to move the canvas; Shift+drag to turn it.",
        }
    }
}

/// Floating windows owned by the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    NewCanvas,
    About,
    Shortcuts,
    /// An action that would discard unsaved work; ask the artist first.
    Unsaved(PendingAction),
    /// A file operation failed. Shown prominently instead of only in the status
    /// line, because a failed save must not go unnoticed.
    Notice(String),
    /// An autosaved document survived a crash or an unclean exit.
    Recovery,
}

/// How often unsaved work is written to the recovery slot.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(20);

/// What to do once the artist has dealt with unsaved changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingAction {
    NewCanvas,
    OpenProject,
    OpenImage,
    /// Index into the recent-projects list.
    OpenRecent(usize),
    Quit,
}

/// Working state of the "New canvas" dialog.
#[derive(Clone, Debug)]
pub struct NewCanvasState {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub background: Background,
    pub custom_color: Color32,
    pub error: Option<String>,
}

impl Default for NewCanvasState {
    fn default() -> Self {
        Self {
            name: String::from("Untitled"),
            width: 1920,
            height: 1080,
            background: Background::White,
            custom_color: Color32::from_rgb(0xF2, 0xEF, 0xE6),
            error: None,
        }
    }
}

/// Keyboard and menu actions collected once per frame.
#[derive(Default)]
struct Actions {
    new_canvas: bool,
    quit: bool,
    undo: bool,
    redo: bool,
    tool: Option<Tool>,
    toggle_panels: bool,
    fit: bool,
    actual_size: bool,
    zoom_in: bool,
    zoom_out: bool,
    brush_smaller: bool,
    brush_bigger: bool,
    select_all: bool,
    deselect: bool,
    quick_mask: bool,
    save: bool,
    save_as: bool,
    open_project: bool,
}

/// State of an in-progress freehand stroke.
#[derive(Clone, Copy, Debug)]
pub struct StrokeState {
    /// The layer this stroke paints into, fixed when the stroke starts.
    layer: LayerId,
    /// Last stamped canvas position.
    last: Pos2,
    /// Speed-adaptive smoothing of the pointer path.
    stabilizer: Stabilizer,
    erase: bool,
    /// True when the stroke paints the layer's mask instead of its pixels.
    mask: bool,
}

/// State of a stroke that paints the selection instead of a layer, while the
/// Quick Mask view is on.
#[derive(Clone, Copy, Debug)]
pub struct MaskStroke {
    /// Last stamped canvas position.
    last: Pos2,
    /// Speed-adaptive smoothing of the pointer path.
    stabilizer: Stabilizer,
    erase: bool,
}

/// A layer or group row renamed in place in the layer list.
pub struct Rename {
    pub id: LayerId,
    /// Text in the field; the entry keeps its old name until this is committed.
    pub text: String,
    /// Name to compare against, so a no-op rename records no history.
    pub original: String,
    /// True until the field has taken keyboard focus once.
    pub focus: bool,
}

/// A name being typed into the properties panel, not yet committed.
pub struct NameField {
    pub id: LayerId,
    /// Text in the field; the entry keeps its old name until the field closes.
    pub text: String,
    /// Name to compare against, so a no-op edit records no history.
    pub original: String,
}

/// A layer row being dragged in the sidebar layer list.
#[derive(Clone, Copy, Debug)]
pub struct LayerDrag {
    /// Stack index the row started at.
    pub from: usize,
    /// Stack insertion slot the pointer is currently over (`0..=entry_count`),
    /// measured before the dragged block is removed.
    pub to: usize,
    /// Group the row would join on release; `None` means the top level.
    pub parent: Option<LayerId>,
}

/// Root application state.
pub struct InkforgeApp {
    pub canvas: Canvas,
    pub brush: BrushSettings,
    /// How the tablet's pressure and tilt shape the brush.
    pub pen: PenSettings,
    pub tool: Tool,
    pub color: Color32,
    pub view: Viewport,
    pub show_tool_rail: bool,
    pub show_sidebar: bool,
    pub dialog: Option<Dialog>,
    pub new_canvas: NewCanvasState,
    /// Tile pyramid cache for the composite.
    pub tiles: TilePyramid,
    /// Small per-layer preview textures for the layer list.
    pub thumbnails: ThumbnailCache,
    /// Row being dragged to reorder layers, if any.
    pub layer_drag: Option<LayerDrag>,
    /// Layer or group row being renamed in the layer list, if any.
    pub rename: Option<Rename>,
    /// Name being typed into the properties panel, if any.
    pub name_field: Option<NameField>,
    /// Shape drawn by the Shape tool.
    pub shape: ShapeKind,
    /// When on and the active layer has a mask, the brush and eraser paint the
    /// mask instead of the layer's pixels.
    pub edit_mask: bool,
    /// Largest per-channel difference the fill tool treats as the same colour.
    pub fill_tolerance: u8,
    /// Largest per-channel difference the magic wand treats as the same colour.
    pub wand_tolerance: u8,
    /// Show the whole selection as a translucent wash over the canvas.
    pub quick_mask: bool,
    /// Pixels the Grow / Shrink slider is currently offset by, and the selection
    /// captured when that drag began, so the slider is relative and not
    /// cumulative.
    pub grow_offset: f32,
    grow_base: Option<Selection>,
    /// Selection drag in progress, in canvas coordinates (start, current).
    pub selection_drag: Option<(Pos2, Pos2)>,
    /// Rectangle marquee or freehand lasso.
    pub selection_mode: SelectionMode,
    /// Feather radius applied to new selections, in pixels.
    pub feather: f32,
    /// Lasso path in progress, in canvas coordinates.
    pub lasso_points: Option<Vec<Pos2>>,
    /// Shape drag in progress, in canvas coordinates (start, current).
    pub shape_drag: Option<(Pos2, Pos2)>,
    /// Active stroke, while the pointer is down on the canvas.
    pub stroke: Option<StrokeState>,
    /// Selection-painting stroke in progress, while Quick Mask is on.
    mask_stroke: Option<MaskStroke>,
    /// Undo/redo stacks.
    pub history: History,
    /// The `.inkforge` file this document came from or was last saved to.
    pub project_path: Option<PathBuf>,
    /// Last folder used by a file dialog, so the next one opens there.
    pub last_dir: Option<PathBuf>,
    /// Autosave slot for crash recovery, when enabled.
    pub autosave_dir: Option<PathBuf>,
    /// Recent projects, newest first, as shown in File ▸ Open recent.
    pub recent: Vec<PathBuf>,
    /// Folder the recent list is stored in, when persistence is on.
    pub recent_dir: Option<PathBuf>,
    /// Recovery the app found on launch and has not dealt with yet.
    pub recovery: Option<RecoveryInfo>,
    autosave_interval: Duration,
    last_autosave: Instant,
    /// The last autosave failure, so it is reported once and not every tick.
    autosave_error: Option<String>,
    /// Snapshots the "before" pixels of the stroke in progress.
    recorder: Option<Recorder>,
    /// Latest reading from the tablet, refreshed once per frame.
    pen_sample: PenSample,
    status: String,
    /// Set when the app is exiting so the window command is only sent once.
    closing: bool,
    /// Last title sent to the OS window, so it is only updated when it changes.
    last_title: String,
    /// Cached "nothing has been painted yet", keyed by the composite revision so
    /// the full-canvas scan runs once per change instead of once per frame.
    empty_check: Option<(u64, bool)>,
}

impl InkforgeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        ui::theme::apply(&cc.egui_ctx);

        let mut app = Self::with_defaults();
        if let Some(dir) = recovery::default_dir() {
            app.enable_autosave(dir, AUTOSAVE_INTERVAL);
            app.start_session();
        }
        if let Some(dir) = recovery::data_dir() {
            app.enable_recent(dir);
        }
        app
    }

    /// State-only constructor. Used by [`Self::new`] and by the headless UI
    /// test, which has no eframe `CreationContext`.
    pub fn with_defaults() -> Self {
        Self {
            canvas: Canvas::default_artwork(),
            brush: BrushSettings::default(),
            pen: PenSettings::default(),
            tool: Tool::Brush,
            color: Color32::from_rgb(0x1B, 0x1B, 0x1E),
            view: Viewport::new(),
            show_tool_rail: true,
            show_sidebar: true,
            dialog: None,
            new_canvas: NewCanvasState::default(),
            tiles: TilePyramid::new(),
            thumbnails: ThumbnailCache::new(),
            layer_drag: None,
            rename: None,
            name_field: None,
            shape: ShapeKind::default(),
            edit_mask: false,
            fill_tolerance: 24,
            wand_tolerance: 24,
            quick_mask: false,
            grow_offset: 0.0,
            grow_base: None,
            selection_drag: None,
            selection_mode: SelectionMode::default(),
            feather: 0.0,
            lasso_points: None,
            shape_drag: None,
            stroke: None,
            mask_stroke: None,
            history: History::new(),
            project_path: None,
            last_dir: None,
            autosave_dir: None,
            recent: Vec::new(),
            recent_dir: None,
            recovery: None,
            autosave_interval: AUTOSAVE_INTERVAL,
            last_autosave: Instant::now(),
            autosave_error: None,
            recorder: None,
            pen_sample: PenSample::MOUSE,
            status: String::from(
                "Ready — paint with the brush (B), erase with E, pick a colour with I.",
            ),
            closing: false,
            last_title: String::new(),
            empty_check: None,
        }
    }

    /// True while nothing has been painted on the document.
    ///
    /// The canvas uses this for its start hint. The scan is cached against the
    /// composite revision, so painting one pixel flips the answer immediately
    /// and idle frames never scan the canvas again.
    pub fn document_is_empty(&mut self) -> bool {
        let composite = self.canvas.composite_mut();
        let revision = composite.revision();
        if let Some((cached, empty)) = self.empty_check {
            if cached == revision {
                return empty;
            }
        }
        let empty = composite.is_empty();
        self.empty_check = Some((revision, empty));
        empty
    }

    /// Title for the OS window bar: the document, a bullet while it has
    /// unsaved changes, and the product name.
    pub fn window_title(&self) -> String {
        let marker = if self.canvas.modified() { " •" } else { "" };
        format!("{}{} — INKFORGE", self.canvas.name(), marker)
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    /// The latest tablet reading. A mouse — or a pen hovering out of contact —
    /// reads as full pressure and no tilt.
    pub fn pen_sample(&self) -> PenSample {
        self.pen_sample
    }

    /// Fill in a tablet reading. The platform path does this once per frame;
    /// tests set a specific reading to check how it shapes the brush.
    pub fn set_pen_sample(&mut self, sample: PenSample) {
        self.pen_sample = sample;
    }

    /// The dab the next stamp will use: the brush shaped by the current pen
    /// reading. Every stroke path goes through here, so pressure and tilt reach
    /// painting, erasing and Quick Mask alike.
    pub fn current_dab(&self, erase: bool) -> BrushDab {
        self.brush
            .dab_scaled(self.color, erase, self.pen.tip(self.pen_sample))
    }

    /// Read the tablet once per frame, and repaint while it changes so the
    /// sidebar readout and the brush cursor follow a pen that is held still.
    fn refresh_pen(&mut self, ctx: &Context) {
        let sample = crate::input::latest();
        if sample != self.pen_sample {
            self.pen_sample = sample;
            ctx.request_repaint();
        }
    }

    pub fn select_tool(&mut self, tool: Tool) {
        self.end_stroke();
        self.selection_drag = None;
        self.lasso_points = None;
        self.shape_drag = None;
        self.tool = tool;
        if tool.is_active() {
            self.set_status(format!("{} tool selected", tool.label()));
        } else {
            self.set_status("Pan tool selected — drag to move the canvas");
        }
    }

    pub fn open_dialog(&mut self, dialog: Dialog) {
        if matches!(dialog, Dialog::NewCanvas) {
            self.new_canvas = NewCanvasState {
                name: format!("{} copy", self.canvas.name()),
                width: self.canvas.width(),
                height: self.canvas.height(),
                background: self.canvas.background(),
                ..NewCanvasState::default()
            };
        }
        self.dialog = Some(dialog);
    }

    pub fn close_dialog(&mut self) {
        self.dialog = None;
    }

    /// Apply the "New canvas" dialog, reporting validation errors inline.
    pub fn create_canvas(&mut self) {
        let state = self.new_canvas.clone();
        let background = match state.background {
            Background::Custom(_) => Background::Custom(state.custom_color),
            other => other,
        };

        match Canvas::try_new(&state.name, state.width, state.height, background) {
            Ok(canvas) => {
                self.set_status(format!(
                    "Created {} ({})",
                    canvas.name(),
                    canvas.dimensions_label()
                ));
                self.adopt_document(canvas, None);
                self.dialog = None;
            }
            Err(message) => self.new_canvas.error = Some(message),
        }
    }

    /// Stamp a single dab and finish at once (a click without dragging).
    pub fn dab_at(&mut self, canvas_pos: Pos2) {
        self.begin_stroke(canvas_pos);
        self.end_stroke();
    }

    /// Start a freehand stroke at a canvas position.
    pub fn begin_stroke(&mut self, canvas_pos: Pos2) {
        if !matches!(self.tool, Tool::Brush | Tool::Eraser) {
            return;
        }

        let erase = self.tool == Tool::Eraser;
        let dab = self.current_dab(erase);

        // In Quick Mask the brush paints the selection, not the layer.
        if self.quick_mask {
            self.begin_mask_stroke(canvas_pos, dab);
            return;
        }

        let Some(layer) = self.canvas.active_layer_id() else {
            self.set_status("Select a layer to paint on — groups hold no pixels of their own.");
            return;
        };

        // Mask editing only engages when the layer actually has a mask; a
        // layer without one keeps painting normally.
        let painting_mask = self.edit_mask && self.canvas.layer_has_mask(layer);

        // Capture the "before" pixels first so this stroke can be undone.
        let mut recorder = Recorder::new();
        let target = if painting_mask {
            self.canvas.layer_mask(layer)
        } else {
            self.canvas.layer_surface(layer)
        };
        if let Some(surface) = target {
            recorder.record_segment(surface, canvas_pos, canvas_pos, dab.radius);
        }
        self.recorder = Some(recorder);

        if painting_mask {
            self.canvas
                .paint_layer_mask(layer, canvas_pos, canvas_pos, &dab);
            self.set_status("Painting the layer mask — white reveals, the eraser hides.");
        } else {
            self.canvas.paint_layer(layer, canvas_pos, canvas_pos, &dab);
        }

        self.stroke = Some(StrokeState {
            last: canvas_pos,
            stabilizer: Stabilizer::begin(canvas_pos),
            erase,
            mask: painting_mask,
            layer,
        });
    }

    /// Start a stroke that paints into the selection (Quick Mask is on).
    fn begin_mask_stroke(&mut self, canvas_pos: Pos2, dab: crate::engine::BrushDab) {
        // With nothing selected the stroke needs an empty field to paint into.
        if self.canvas.selection().is_none() {
            self.canvas
                .set_selection(Some(Selection::empty(self.canvas.width(), self.canvas.height())));
        }
        self.canvas.paint_selection(canvas_pos, canvas_pos, &dab);

        self.mask_stroke = Some(MaskStroke {
            last: canvas_pos,
            stabilizer: Stabilizer::begin(canvas_pos),
            erase: dab.erase,
        });

        let status = if dab.erase {
            "Removing from the selection — Quick Mask is on."
        } else {
            "Adding to the selection — Quick Mask is on."
        };
        self.set_status(status);
    }

    /// True while a stroke is painting the selection rather than a layer.
    pub fn is_painting_mask(&self) -> bool {
        self.mask_stroke.is_some()
    }

    /// Continue the active stroke, smoothing the pointer path.
    pub fn extend_stroke(&mut self, canvas_pos: Pos2) {
        if let Some(mut state) = self.mask_stroke {
            let smoothed = state.stabilizer.push(canvas_pos, self.brush.smoothing());
            let dab = self.current_dab(state.erase);
            self.canvas.paint_selection(state.last, smoothed, &dab);
            state.last = smoothed;
            self.mask_stroke = Some(state);
            return;
        }

        let Some(mut state) = self.stroke else {
            self.begin_stroke(canvas_pos);
            return;
        };

        let smoothed = state.stabilizer.push(canvas_pos, self.brush.smoothing());
        let dab = self.current_dab(state.erase);

        let target = if state.mask {
            self.canvas.layer_mask(state.layer)
        } else {
            self.canvas.layer_surface(state.layer)
        };
        if let Some(recorder) = self.recorder.as_mut() {
            if let Some(surface) = target {
                recorder.record_segment(surface, state.last, smoothed, dab.radius);
            }
        }

        if state.mask {
            self.canvas
                .paint_layer_mask(state.layer, state.last, smoothed, &dab);
        } else {
            self.canvas
                .paint_layer(state.layer, state.last, smoothed, &dab);
        }

        state.last = smoothed;
        self.stroke = Some(state);
    }

    /// Finish the active stroke and commit it to history.
    pub fn end_stroke(&mut self) {
        // A Quick Mask stroke edits the selection, so it has no pixels to undo.
        if self.mask_stroke.take().is_some() {
            if self.canvas.finish_selection_paint() {
                self.set_status("Selection painted.");
            } else {
                self.set_status("The selection is empty — selection cleared.");
            }
            return;
        }

        let Some(recorder) = self.recorder.take() else {
            self.stroke = None;
            return;
        };

        let Some(state) = self.stroke else {
            return;
        };
        let label = if state.erase {
            "erase"
        } else if state.mask {
            "paint layer mask"
        } else {
            "brush stroke"
        };

        let target = if state.mask {
            self.canvas.layer_mask(state.layer)
        } else {
            self.canvas.layer_surface(state.layer)
        };
        let patches = match target {
            Some(surface) => recorder.finish(surface),
            None => Vec::new(),
        };
        if !patches.is_empty() {
            let edit = if state.mask {
                Edit::mask_pixels(state.layer, patches, label)
            } else {
                Edit::pixels(state.layer, patches, label)
            };
            self.history.push(edit);
        }

        self.stroke = None;
    }

    /// Undo the newest edit.
    pub fn undo(&mut self) {
        // A stroke still in progress becomes a completed edit first.
        self.end_stroke();
        match self.history.undo(&mut self.canvas) {
            Some(label) => self.set_status(format!("Undid {label}")),
            None => self.set_status("Nothing to undo."),
        }
    }

    /// Redo the newest undone edit.
    pub fn redo(&mut self) {
        self.end_stroke();
        match self.history.redo(&mut self.canvas) {
            Some(label) => self.set_status(format!("Redid {label}")),
            None => self.set_status("Nothing to redo."),
        }
    }

    /// Eyedropper: sample the artwork, falling back to the canvas background.
    pub fn pick_colour_at(&mut self, canvas_pos: Pos2) {
        match self
            .canvas
            .sample(canvas_pos)
            .or_else(|| self.canvas.background().fill())
        {
            Some(colour) => {
                self.color = colour;
                self.set_status(format!(
                    "Picked #{:02X}{:02X}{:02X}",
                    colour.r(),
                    colour.g(),
                    colour.b()
                ));
            }
            None => self.set_status("Nothing to pick there — that area is transparent."),
        }
    }

    // ---- files ---------------------------------------------------------

    /// New canvas, after checking for unsaved work.
    pub fn request_new_canvas(&mut self) {
        if self.guard_unsaved(PendingAction::NewCanvas) {
            self.open_dialog(Dialog::NewCanvas);
        }
    }

    /// Save to the current project file, asking for a path the first time.
    /// Returns whether anything was written.
    pub fn save_project(&mut self) -> bool {
        self.end_stroke();
        match self.project_path.clone() {
            Some(path) => self.write_project(path),
            None => self.save_project_as(),
        }
    }

    /// Always ask for a path before saving.
    pub fn save_project_as(&mut self) -> bool {
        self.end_stroke();

        let name = format!("{}.inkforge", default_file_stem(self.canvas.name()));
        let mut dialog = rfd::FileDialog::new()
            .add_filter("INKFORGE project", &["inkforge"])
            .set_file_name(name);
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }

        let Some(path) = dialog.save_file() else {
            self.set_status("Save cancelled — nothing was written.");
            return false;
        };
        let path = ensure_extension(path, &["inkforge"]);
        self.write_project(path)
    }

    fn write_project(&mut self, path: PathBuf) -> bool {
        match io::save_project(&path, &self.canvas) {
            Ok(()) => {
                self.last_dir = path.parent().map(Path::to_path_buf);
                self.project_path = Some(path.clone());
                self.remember_recent(&path);
                self.canvas.mark_saved();
                self.clear_recovery();
                self.set_status(format!("Saved {}", display_path(&path)));
                true
            }
            Err(error) => {
                self.fail("The project could not be saved", error);
                false
            }
        }
    }

    /// Open an `.inkforge` project, after checking for unsaved work.
    pub fn open_project(&mut self) {
        if self.guard_unsaved(PendingAction::OpenProject) {
            self.pick_project();
        }
    }

    fn pick_project(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("INKFORGE project", &["inkforge"]);
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }

        let Some(path) = dialog.pick_file() else {
            self.set_status("Open cancelled.");
            return;
        };

        match io::load_project(&path) {
            Ok(canvas) => {
                let label = format!("{} ({})", canvas.name(), canvas.dimensions_label());
                self.adopt_document(canvas, Some(path.clone()));
                self.last_dir = path.parent().map(Path::to_path_buf);
                self.remember_recent(&path);
                self.set_status(format!("Opened {label}"));
            }
            Err(error) => self.fail("The project could not be opened", error),
        }
    }

    // ---- recent projects ------------------------------------------------

    /// Show and persist the recent-projects list stored in `dir`.
    pub fn enable_recent(&mut self, dir: PathBuf) {
        self.recent = recent::load(&dir);
        self.recent_dir = Some(dir);
    }

    /// Recent projects, newest first. Paths that no longer exist are gone.
    pub fn recent_projects(&self) -> &[PathBuf] {
        &self.recent
    }

    fn remember_recent(&mut self, path: &Path) {
        if let Some(dir) = &self.recent_dir {
            self.recent = recent::remember(dir, path);
        }
    }

    fn forget_recent(&mut self, path: &Path) {
        if let Some(dir) = &self.recent_dir {
            self.recent = recent::forget(dir, path);
        }
    }

    /// Empty the recent-projects menu and its stored list.
    pub fn clear_recent(&mut self) {
        if let Some(dir) = &self.recent_dir {
            recent::clear(dir);
        }
        self.recent.clear();
        self.set_status("Recent projects cleared.");
    }

    /// Open an entry from the recent menu, after checking for unsaved work.
    pub fn open_recent(&mut self, index: usize) {
        if self.guard_unsaved(PendingAction::OpenRecent(index)) {
            self.pick_recent(index);
        }
    }

    fn pick_recent(&mut self, index: usize) {
        let Some(path) = self.recent.get(index).cloned() else {
            self.set_status("That project is no longer in the recent list.");
            return;
        };

        match io::load_project(&path) {
            Ok(canvas) => {
                let label = format!("{} ({})", canvas.name(), canvas.dimensions_label());
                self.adopt_document(canvas, Some(path.clone()));
                self.last_dir = path.parent().map(Path::to_path_buf);
                self.remember_recent(&path);
                self.set_status(format!("Opened {label}"));
            }
            Err(error) => {
                // A deleted or moved file should not keep failing in the menu.
                self.forget_recent(&path);
                self.fail("The recent project could not be opened", error);
            }
        }
    }

    /// Open a PNG or JPEG as a brand new document.
    pub fn open_image_file(&mut self) {
        if self.guard_unsaved(PendingAction::OpenImage) {
            self.pick_image_file();
        }
    }

    fn pick_image_file(&mut self) {
        let mut dialog =
            rfd::FileDialog::new().add_filter("PNG or JPEG image", &["png", "jpg", "jpeg"]);
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }

        let Some(path) = dialog.pick_file() else {
            self.set_status("Open cancelled.");
            return;
        };

        match io::open_image(&path) {
            Ok(canvas) => {
                let label = format!("{} ({})", canvas.name(), canvas.dimensions_label());
                self.adopt_document(canvas, None);
                self.last_dir = path.parent().map(Path::to_path_buf);
                self.set_status(format!(
                    "Opened image {label} — File ▸ Save project keeps it editable"
                ));
            }
            Err(error) => self.fail("The image could not be opened", error),
        }
    }

    /// Add a PNG or JPEG as a new layer in the current document.
    pub fn import_image_layer(&mut self) {
        self.end_stroke();

        let mut dialog =
            rfd::FileDialog::new().add_filter("PNG or JPEG image", &["png", "jpg", "jpeg"]);
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }

        let Some(path) = dialog.pick_file() else {
            self.set_status("Import cancelled.");
            return;
        };

        match io::read_image(&path) {
            Ok((width, height, pixels)) => {
                let stem = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| String::from("Image"));

                let Some((index, layer)) =
                    self.canvas.add_image_layer(&stem, width, height, &pixels)
                else {
                    self.set_status("The image could not be added to this document.");
                    return;
                };

                let name = layer.name().to_owned();
                self.last_dir = path.parent().map(Path::to_path_buf);
                self.history
                    .push(Edit::inserted(index, Entry::Layer(layer), "import image"));

                let cropped = width > self.canvas.width() || height > self.canvas.height();
                self.set_status(if cropped {
                    format!(
                        "Imported {name} ({width} × {height}) — cropped to the {} × {} canvas",
                        self.canvas.width(),
                        self.canvas.height()
                    )
                } else {
                    format!("Imported {name} ({width} × {height})")
                });
            }
            Err(error) => self.fail("The image could not be imported", error),
        }
    }

    /// Flatten the visible artwork and write it as PNG or JPEG.
    pub fn export_image(&mut self, format: ImageFormat) {
        self.end_stroke();

        let mut extensions = vec![format.extension()];
        if let Some(extra) = format.extra_extension() {
            extensions.push(extra);
        }
        let name = format!(
            "{}.{}",
            default_file_stem(self.canvas.name()),
            format.extension()
        );

        let mut dialog = rfd::FileDialog::new()
            .add_filter(format.label(), &extensions)
            .set_file_name(name);
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }

        let Some(path) = dialog.save_file() else {
            self.set_status("Export cancelled — nothing was written.");
            return;
        };
        let path = ensure_extension(path, &extensions);

        match io::export_image(&mut self.canvas, &path, format) {
            Ok(()) => {
                self.last_dir = path.parent().map(Path::to_path_buf);
                self.set_status(format!(
                    "Exported the flattened image to {}",
                    display_path(&path)
                ));
            }
            Err(error) => self.fail("The image could not be exported", error),
        }
    }

    /// True when it is safe to continue; otherwise ask about unsaved work.
    fn guard_unsaved(&mut self, action: PendingAction) -> bool {
        if self.canvas.modified() {
            self.dialog = Some(Dialog::Unsaved(action));
            false
        } else {
            true
        }
    }

    /// Continue a guarded action once the artist has answered.
    pub fn proceed(&mut self, action: PendingAction) {
        self.dialog = None;
        match action {
            PendingAction::NewCanvas => self.open_dialog(Dialog::NewCanvas),
            PendingAction::OpenProject => self.pick_project(),
            PendingAction::OpenImage => self.pick_image_file(),
            PendingAction::OpenRecent(index) => self.pick_recent(index),
            PendingAction::Quit => self.force_quit(),
        }
    }

    /// Replace the document after a successful open.
    fn adopt_document(&mut self, canvas: Canvas, project: Option<PathBuf>) {
        self.canvas = canvas;
        self.project_path = project;
        self.stroke = None;
        self.mask_stroke = None;
        self.recorder = None;
        self.selection_drag = None;
        self.lasso_points = None;
        self.shape_drag = None;
        self.edit_mask = false;
        self.history.clear();
        self.view.request_fit();
        // Whatever was in the slot belonged to the document being replaced.
        self.clear_recovery();
        self.dialog = None;
    }

    /// Report a file failure in the status line and in a modal window.
    fn fail(&mut self, summary: &str, error: String) {
        self.set_status(format!("{summary}: {error}"));
        self.dialog = Some(Dialog::Notice(format!("{summary}.\n\n{error}")));
    }

    // ---- autosave and crash recovery -----------------------------------

    /// Turn on autosave into `dir`, checked every `interval`.
    pub fn enable_autosave(&mut self, dir: PathBuf, interval: Duration) {
        self.autosave_dir = Some(dir);
        self.autosave_interval = interval;
        self.last_autosave = Instant::now();
    }

    pub fn autosave_enabled(&self) -> bool {
        self.autosave_dir.is_some()
    }

    pub fn autosave_interval(&self) -> Duration {
        self.autosave_interval
    }

    /// Look for work left behind by a crash. Called once, at launch.
    pub fn start_session(&mut self) {
        let Some(dir) = self.autosave_dir.clone() else {
            return;
        };
        if let Some(info) = recovery::inspect(&dir) {
            self.recovery = Some(info);
            self.dialog = Some(Dialog::Recovery);
        }
    }

    /// Write unsaved work to the recovery slot now. Returns whether it was
    /// written; a clean document has nothing worth recovering.
    pub fn autosave_now(&mut self) -> bool {
        let Some(dir) = self.autosave_dir.clone() else {
            return false;
        };
        if !self.canvas.modified() {
            return false;
        }

        self.last_autosave = Instant::now();
        match recovery::save_recovery(&dir, &self.canvas, self.project_path.as_deref()) {
            Ok(()) => {
                self.autosave_error = None;
                true
            }
            Err(error) => {
                // Report a broken autosave once, not every 20 seconds.
                if self.autosave_error.as_deref() != Some(error.as_str()) {
                    self.autosave_error = Some(error.clone());
                    self.set_status(format!("Autosave failed: {error}"));
                }
                false
            }
        }
    }

    /// One autosave tick, called once per frame.
    fn maybe_autosave(&mut self) {
        if self.autosave_dir.is_none() || !self.canvas.modified() {
            return;
        }
        // Never stall a live stroke to write a copy of the document.
        if self.stroke.is_some() {
            self.last_autosave = Instant::now();
            return;
        }
        if self.last_autosave.elapsed() < self.autosave_interval {
            return;
        }
        self.autosave_now();
    }

    /// Remove the recovery slot after a save, a discard or a clean exit.
    pub fn clear_recovery(&mut self) {
        if let Some(dir) = self.autosave_dir.clone() {
            recovery::clear(&dir);
        }
    }

    /// Bring recovered artwork back, still associated with its project file.
    pub fn restore_recovery(&mut self) {
        let Some(info) = self.recovery.take() else {
            self.close_dialog();
            return;
        };

        match recovery::load(&info.dir) {
            Ok((canvas, project)) => {
                let label = format!("{} ({})", canvas.name(), canvas.dimensions_label());
                self.adopt_document(canvas, project);
                // The pixels are not in the project file yet, so this counts as
                // an unsaved change.
                self.canvas.mark_modified();
                self.last_autosave = Instant::now();
                self.set_status(format!("Recovered {label} — save it with Ctrl+S"));
                // Take a fresh copy immediately in case this session also dies.
                self.autosave_now();
            }
            Err(error) => {
                self.recovery = Some(info);
                self.fail("The autosaved work could not be recovered", error);
            }
        }
    }

    /// Throw the recovered work away.
    pub fn discard_recovery(&mut self) {
        if let Some(info) = self.recovery.take() {
            recovery::clear(&info.dir);
        }
        self.close_dialog();
        self.set_status("Discarded the autosaved work.");
    }

    /// Keep the recovery slot for the next launch.
    pub fn postpone_recovery(&mut self) {
        self.close_dialog();
        self.set_status("Autosaved work kept — it will be offered again next launch.");
    }

    // ---- layer commands -----------------------------------------------

    /// Add an empty layer above the active entry; inside a group it joins it.
    pub fn add_layer(&mut self) {
        self.end_stroke();
        let (index, layer) = self.canvas.layer_add_above_active();
        self.history
            .push(Edit::inserted(index, Entry::Layer(layer), "add layer"));
        self.set_status(format!("Added {}", self.canvas.active_entry().name()));
    }

    /// Wrap the active layer in a new group.
    pub fn add_group(&mut self) {
        self.end_stroke();
        match self.canvas.group_add_around_active() {
            Some((index, entry)) => {
                self.history.push(Edit::inserted(index, entry, "new group"));
                self.set_status(format!("Created {}", self.canvas.active_entry().name()));
            }
            None => self.set_status(
                "Select a top-level layer to group — groups cannot be nested yet.",
            ),
        }
    }

    /// Delete the active entry: a layer, or a whole group with its children.
    /// The last remaining paint layer is kept so there is always somewhere to
    /// paint.
    pub fn delete_active_layer(&mut self) {
        self.end_stroke();

        let index = self.canvas.active_layer_index();
        let name = self.canvas.active_entry().name().to_owned();
        let is_group = self.canvas.active_entry().is_group();

        match self.canvas.remove_entries_at(index) {
            Some(entries) => {
                let label = if is_group { "delete group" } else { "delete layer" };
                self.history.push(Edit::removed(index, entries, label));
                self.set_status(format!("Deleted {name}"));
            }
            None => self.set_status("A document needs at least one layer."),
        }
    }

    /// Move the active entry up (`1`) or down (`-1`) among its siblings.
    pub fn move_active_layer(&mut self, offset: i32) {
        self.end_stroke();
        let from = self.canvas.active_layer_index();
        if !self.canvas.move_active_by(offset) {
            return;
        }
        let to = self.canvas.active_layer_index();
        self.history.push(Edit::moved(from, to, "reorder layers"));
        self.set_status("Reordered layers");
    }

    /// Move a block to a new stack index (drag and drop), one undo step.
    pub fn move_layer_to(&mut self, from: usize, to: usize) {
        self.end_stroke();
        if from == to || to >= self.canvas.entry_count() {
            return;
        }
        self.canvas.layer_move(from, to);
        self.history.push(Edit::moved(from, to, "reorder layers"));
        self.set_status("Reordered layers");
    }

    /// Start dragging a layer row; the sidebar updates [`LayerDrag::to`] and
    /// [`LayerDrag::parent`] as the pointer moves over other rows.
    pub fn begin_layer_drag(&mut self, from: usize) {
        let parent = self.canvas.entry(from).and_then(Entry::parent);
        self.layer_drag = Some(LayerDrag {
            from,
            to: from,
            parent,
        });
    }

    /// Drop the dragged row where the pointer left it, if that moved it.
    ///
    /// A drop can land inside a group or back at the top level, and the whole
    /// move (position and group) is recorded as one undo step.
    pub fn finish_layer_drag(&mut self) {
        let Some(drag) = self.layer_drag.take() else {
            return;
        };
        self.end_stroke();

        let Some(entry) = self.canvas.entry(drag.from) else {
            return;
        };
        let group_before = entry.parent();
        let is_group = entry.is_group();
        let name = entry.name().to_owned();

        // Groups stay at the top level: nesting is not supported.
        if is_group && drag.parent.is_some() {
            return;
        }

        let len = self.canvas.block_len(drag.from);
        let slot = drag.to.min(self.canvas.entry_count());
        // The slot is measured before the block is taken out, so a drop above
        // the block shifts down by its height once the block is gone.
        let dest = if slot >= drag.from + len {
            slot - len
        } else if slot <= drag.from {
            slot
        } else {
            drag.from
        };

        let Some(to) = self.canvas.layer_move_into(drag.from, dest, drag.parent) else {
            return;
        };
        if to == drag.from && drag.parent == group_before {
            return;
        }

        let label = match (group_before, drag.parent) {
            (None, Some(_)) => "move into group",
            (Some(_), None) => "move out of group",
            (Some(before), Some(after)) if before != after => "move between groups",
            _ => "reorder layers",
        };
        self.history
            .push(Edit::moved_into(drag.from, to, group_before, drag.parent, label));

        // Show where the layer went, even when the folder was collapsed.
        if let Some(group) = drag.parent {
            self.canvas.set_group_collapsed(group, false);
        }

        let status = match drag.parent {
            Some(group) => match self.canvas.group(group) {
                Some(group) => format!("Moved {name} into {}", group.name()),
                None => format!("Moved {name} into a group"),
            },
            None if group_before.is_some() => format!("Moved {name} out of its group"),
            None => String::from("Reordered layers"),
        };
        self.set_status(status);
    }

    /// Bake every visible layer into a single layer.
    pub fn flatten_image(&mut self) {
        self.end_stroke();
        if self.canvas.layer_count() <= 1 {
            self.set_status("The image is already a single layer.");
            return;
        }

        match self.canvas.layer_flatten() {
            Some(flattened) => {
                self.history
                    .push(Edit::flattened(flattened, "flatten image"));
                self.set_status("Flattened all layers into one");
            }
            None => self.set_status("Nothing to flatten."),
        }
    }

    /// Copy the active layer, pixels and all, above itself.
    pub fn duplicate_layer(&mut self) {
        self.end_stroke();
        if self.canvas.active_entry().is_group() {
            self.set_status("Groups cannot be duplicated yet.");
            return;
        }

        let index = self.canvas.active_layer_index();
        if let Some((_, snapshot)) = self.canvas.layer_duplicate(index) {
            self.history.push(Edit::inserted(
                index + 1,
                Entry::Layer(snapshot),
                "duplicate layer",
            ));
            self.set_status(format!(
                "Duplicated as {}",
                self.canvas.active_entry().name()
            ));
        }
    }

    /// Bake the active layer into the layer below it.
    pub fn merge_layer_down(&mut self) {
        self.end_stroke();
        if self.canvas.active_entry().is_group() {
            self.set_status("Select a layer to merge, not a group.");
            return;
        }

        let index = self.canvas.active_layer_index();
        if index == 0 {
            self.set_status("There is nothing below this layer to merge into.");
            return;
        }

        let name = self.canvas.active_entry().name().to_owned();
        match self.canvas.layer_merge_down(index) {
            Some(merged) => {
                self.history.push(Edit::merged(merged, "merge down"));
                self.set_status(format!("Merged {name} into the layer below"));
            }
            None => self.set_status("Nothing to merge."),
        }
    }

    /// Make another entry (layer or group) active.
    pub fn select_entry(&mut self, index: usize) {
        self.end_stroke();
        // A panel edit belongs to the entry that was selected when it started.
        self.finish_name_draft(true);
        self.canvas.set_active_layer(index);
    }

    pub fn set_layer_visibility(&mut self, id: LayerId, visible: bool) {
        self.update_layer_props(id, "layer visibility", |props| props.visible = visible);
    }

    pub fn set_layer_name(&mut self, id: LayerId, name: String) {
        self.update_layer_props(id, "layer name", |props| {
            if !name.trim().is_empty() {
                props.name = name;
            }
        });
    }

    pub fn set_layer_opacity(&mut self, id: LayerId, opacity: f32) {
        self.update_layer_props(id, "layer opacity", |props| {
            props.opacity = opacity.clamp(0.0, 1.0)
        });
    }

    pub fn set_layer_blend(&mut self, id: LayerId, blend: BlendMode) {
        self.update_layer_props(id, "blend mode", |props| props.blend = blend);
    }

    /// Turn "Lock transparent pixels" on or off for one layer.
    ///
    /// A property change like opacity or visibility, so it undoes in one step
    /// and never touches a pixel.
    pub fn set_layer_alpha_lock(&mut self, id: LayerId, locked: bool) {
        self.update_layer_props(id, "lock transparent pixels", |props| {
            props.alpha_locked = locked
        });
    }

    /// Clip a layer to the alpha of the layer below it (Clip Studio Paint's
    /// "Clip to layer below", Krita's alpha inheritance).
    ///
    /// A compositing property, not a pixel edit: the layer keeps every pixel,
    /// its opacity and its blend mode, and simply cannot show where the base
    /// layer has no paint. Undoes in one step like any other property.
    pub fn set_layer_clip(&mut self, id: LayerId, clip: bool) {
        self.update_layer_props(id, "clip to layer below", |props| {
            props.clip_to_below = clip
        });
    }

    /// Turn mask editing on or off for the active layer.
    ///
    /// While it is on, the brush and eraser paint the layer's mask rather than
    /// its pixels, so an artist can hide and reveal parts of the artwork
    /// without deleting anything.
    pub fn set_edit_mask(&mut self, on: bool) {
        self.end_stroke();
        self.edit_mask = on;
        if on {
            self.set_status(
                "Mask editing on — brush reveals, eraser hides. Fill and shapes still \
                 paint the layer.",
            );
        } else {
            self.set_status("Mask editing off — painting the layer again.");
        }
    }

    /// Add a mask to one layer (`reveal` shows the whole layer, otherwise hides
    /// it) as one undo step.
    pub fn add_layer_mask(&mut self, id: LayerId, reveal: bool) {
        self.end_stroke();
        if self.canvas.layer_has_mask(id) {
            self.set_status("This layer already has a mask.");
            return;
        }
        if self.canvas.add_layer_mask(id, reveal) {
            self.history.push(Edit::mask_added(id, reveal));
            self.edit_mask = true;
            self.set_status(if reveal {
                "Added a layer mask (reveal all) — paint black with the eraser to hide."
            } else {
                "Added a layer mask (hide all) — paint white with the brush to reveal."
            });
        }
    }

    /// Delete a layer's mask as one undo step. The pixels were never touched,
    /// so the layer comes back exactly as it was.
    pub fn delete_layer_mask(&mut self, id: LayerId) {
        self.end_stroke();
        if let Some(mask) = self.canvas.set_layer_mask(id, None) {
            self.history.push(Edit::mask_removed(id, mask));
            self.edit_mask = false;
            self.set_status("Deleted the layer mask.");
        }
    }

    /// Invert a layer's mask (white ↔ black) as one undo step.
    pub fn invert_layer_mask(&mut self, id: LayerId) {
        self.end_stroke();
        let region = PixelRegion {
            x: 0,
            y: 0,
            width: self.canvas.width(),
            height: self.canvas.height(),
        };
        let mut recorder = Recorder::new();
        let Some(mask) = self.canvas.layer_mask(id) else {
            return;
        };
        recorder.record_region(mask, region);

        if self.canvas.invert_layer_mask(id).is_none() {
            return;
        }

        let patches = match self.canvas.layer_mask(id) {
            Some(mask) => recorder.finish(mask),
            None => Vec::new(),
        };
        if !patches.is_empty() {
            self.history
                .push(Edit::mask_pixels(id, patches, "invert layer mask"));
            self.set_status("Inverted the layer mask.");
        }
    }

    /// Set the live colour adjustment of one layer.
    ///
    /// The adjustment is applied when the layer is composited and never touches
    /// its pixels, so it undoes in one step and can be dragged back to neutral
    /// without losing any paint.
    pub fn set_layer_adjustment(&mut self, id: LayerId, adjust: Adjustment) {
        self.update_layer_props(id, "layer adjustment", |props| props.adjust = adjust);
    }

    /// Flip or rotate the whole document, as one undo step.
    ///
    /// The layers are permuted in place (nothing is resampled), so undo applies
    /// the opposite transform instead of keeping a copy of the canvas, and a
    /// quarter turn swaps the document's width and height.
    pub fn transform_document(&mut self, transform: crate::engine::Transform) {
        self.end_stroke();
        let before = (self.canvas.width(), self.canvas.height());

        self.canvas.transform(transform);
        self.history.push(Edit::transformed(transform));

        let after = (self.canvas.width(), self.canvas.height());
        if before != after {
            // The page changed shape, so the old zoom no longer frames it.
            self.view.request_fit();
        }

        self.set_status(format!(
            "{} — {} × {}",
            transform.label(),
            after.0,
            after.1
        ));
    }

    pub fn set_group_visibility(&mut self, id: LayerId, visible: bool) {
        self.update_group_props(id, "group visibility", |props| props.visible = visible);
    }

    /// Start renaming a layer or group in place, prefilled with its name.
    pub fn begin_rename_entry(&mut self, id: LayerId) {
        let name = self
            .canvas
            .group_props(id)
            .map(|props| props.name)
            .or_else(|| self.canvas.layer_props(id).map(|props| props.name));
        let Some(name) = name else {
            return;
        };
        self.rename = Some(Rename {
            id,
            text: name.clone(),
            original: name,
            focus: true,
        });
    }

    /// Close the inline rename. Leaving the field commits; Escape cancels.
    /// Nothing is written until then, so a rename is a single undo step.
    pub fn finish_rename(&mut self, commit: bool) {
        let Some(state) = self.rename.take() else {
            return;
        };
        if !commit {
            return;
        }

        let name = state.text.trim().to_owned();
        if name.is_empty() || name == state.original {
            return;
        }
        if self.canvas.group_props(state.id).is_some() {
            self.set_group_name(state.id, name);
        } else {
            self.set_layer_name(state.id, name);
        }
    }

    /// Type a name in the properties panel without touching the document.
    pub fn set_name_draft(&mut self, id: LayerId, text: String) {
        if let Some(field) = self.name_field.as_mut() {
            if field.id == id {
                field.text = text;
                return;
            }
        }
        let original = self
            .canvas
            .group_props(id)
            .map(|props| props.name)
            .or_else(|| self.canvas.layer_props(id).map(|props| props.name));
        let Some(original) = original else {
            return;
        };
        self.name_field = Some(NameField {
            id,
            text,
            original,
        });
    }

    /// Close the properties panel name field. Leaving the field commits once,
    /// so the whole edit is a single undo step; Escape drops it.
    pub fn finish_name_draft(&mut self, commit: bool) {
        let Some(field) = self.name_field.take() else {
            return;
        };
        let name = field.text.trim().to_owned();
        if !commit || name.is_empty() || name == field.original {
            return;
        }
        if self.canvas.group_props(field.id).is_some() {
            self.set_group_name(field.id, name);
        } else {
            self.set_layer_name(field.id, name);
        }
    }

    pub fn set_group_name(&mut self, id: LayerId, name: String) {
        self.update_group_props(id, "group name", |props| {
            if !name.trim().is_empty() {
                props.name = name;
            }
        });
    }

    pub fn set_group_opacity(&mut self, id: LayerId, opacity: f32) {
        self.update_group_props(id, "group opacity", |props| {
            props.opacity = opacity.clamp(0.0, 1.0)
        });
    }

    /// Collapsed state is view state, so it never enters the undo history.
    pub fn toggle_group_collapsed(&mut self, id: LayerId) {
        let collapsed = self.canvas.group_is_collapsed(id);
        self.canvas.set_group_collapsed(id, !collapsed);
    }

    /// Apply and record one property change on a group.
    fn update_group_props(
        &mut self,
        id: LayerId,
        label: &'static str,
        edit: impl FnOnce(&mut GroupProps),
    ) {
        let Some(before) = self.canvas.group_props(id) else {
            return;
        };

        let mut after = before.clone();
        edit(&mut after);
        if after == before {
            return;
        }

        self.canvas.set_group_props(id, after.clone());
        self.history.push(Edit::group_props(id, before, after, label));
    }

    /// Apply and record one property change on a layer.
    fn update_layer_props(
        &mut self,
        id: LayerId,
        label: &'static str,
        edit: impl FnOnce(&mut LayerProps),
    ) {
        let Some(before) = self.canvas.layer_props(id) else {
            return;
        };

        let mut after = before.clone();
        edit(&mut after);
        if after == before {
            return;
        }

        self.canvas.set_layer_props(id, after.clone());
        self.history.push(Edit::props(id, before, after, label));
    }

    // ---- selection, fill and shapes -----------------------------------

    /// Begin dragging a rectangular selection.
    pub fn begin_selection_drag(&mut self, at: Pos2) {
        self.end_stroke();
        self.selection_drag = Some((at, at));
    }

    pub fn update_selection_drag(&mut self, at: Pos2) {
        if let Some((start, _)) = self.selection_drag {
            self.selection_drag = Some((start, at));
        }
    }

    /// Finish the selection drag; a click without a drag clears it.
    pub fn end_selection_drag(&mut self) {
        let Some((start, end)) = self.selection_drag.take() else {
            return;
        };

        match Selection::rectangle(
            start,
            end,
            self.canvas.width(),
            self.canvas.height(),
            self.feather,
        ) {
            Some(selection) => {
                self.set_status(format!("Selection: {}", selection.label()));
                self.canvas.set_selection(Some(selection));
            }
            None => {
                self.canvas.clear_selection();
                self.set_status("Selection cleared.");
            }
        }
    }

    /// Start a freehand lasso.
    pub fn begin_lasso(&mut self, at: Pos2) {
        self.end_stroke();
        self.lasso_points = Some(vec![at]);
    }

    /// Add a point to the lasso path, skipping points that are too close to
    /// keep the polygon (and its mask maths) small.
    pub fn extend_lasso(&mut self, at: Pos2) {
        let Some(points) = self.lasso_points.as_mut() else {
            return;
        };
        let far_enough = points
            .last()
            .map(|last| (at - *last).length() > 1.5)
            .unwrap_or(true);
        if far_enough && points.len() < 4096 {
            points.push(at);
        }
    }

    /// Close the lasso into a selection.
    pub fn end_lasso(&mut self) {
        let Some(points) = self.lasso_points.take() else {
            return;
        };

        match Selection::lasso(&points, self.canvas.width(), self.canvas.height(), self.feather)
        {
            Some(selection) => {
                self.set_status(format!("Selection: {}", selection.label()));
                self.canvas.set_selection(Some(selection));
            }
            None => {
                self.canvas.clear_selection();
                self.set_status("The lasso enclosed no area — selection cleared.");
            }
        }
    }

    /// Magic wand: select the connected area of similar colour clicked on.
    pub fn wand_at(&mut self, at: Pos2) {
        self.end_stroke();

        match self.canvas.wand_selection_at(at, self.wand_tolerance, self.feather) {
            Some(selection) => {
                self.set_status(format!("Selection: {}", selection.label()));
                self.canvas.set_selection(Some(selection));
            }
            None => {
                self.canvas.clear_selection();
                self.set_status("Nothing similar there — selection cleared.");
            }
        }
    }

    pub fn set_wand_tolerance(&mut self, tolerance: u8) {
        self.wand_tolerance = tolerance;
    }

    /// Capture the selection that a Grow / Shrink drag starts from.
    pub fn begin_grow(&mut self) {
        self.grow_base = self.canvas.selection().cloned();
        self.grow_offset = 0.0;
    }

    /// Resize the captured selection by `offset` pixels, negative shrinking it.
    ///
    /// Every call works from the captured selection, so dragging back towards
    /// the middle restores the original size instead of compounding.
    pub fn preview_grow(&mut self, offset: f32) {
        if self.grow_base.is_none() {
            self.begin_grow();
        }
        self.grow_offset = offset;

        let Some(base) = self.grow_base.clone() else {
            return;
        };
        let pixels = offset.round() as i32;
        let resized = if pixels == 0 {
            Some(base)
        } else {
            base.resized(pixels, self.canvas.width(), self.canvas.height())
        };
        self.canvas.set_selection(resized);
    }

    /// End a Grow / Shrink drag: the selection stays where the slider left it
    /// and the slider returns to the middle.
    pub fn finish_grow(&mut self) {
        let pixels = self.grow_offset.round() as i32;
        let started = self.grow_base.is_some();
        self.grow_offset = 0.0;
        self.grow_base = None;

        if !started {
            if pixels != 0 {
                self.set_status("Nothing is selected.");
            }
            return;
        }

        if self.canvas.selection().is_none() {
            self.set_status("The selection shrank away — selection cleared.");
        } else if pixels == 0 {
            self.set_status("Selection back to its original size.");
        } else {
            let verb = if pixels > 0 { "Grew" } else { "Shrank" };
            self.set_status(format!("{verb} the selection by {} px.", pixels.abs()));
        }
    }

    pub fn toggle_quick_mask(&mut self) {
        self.set_quick_mask(!self.quick_mask);
    }

    pub fn set_quick_mask(&mut self, on: bool) {
        self.quick_mask = on;
        if !on {
            self.set_status("Quick mask off.");
        } else if self.canvas.selection().is_none() {
            self.set_status("Quick mask on — nothing is selected yet.");
        } else {
            self.set_status(
                "Quick mask on — the brush adds to the selection, the eraser removes from it.",
            );
        }
    }

    /// Swap the selection for everything outside it.
    pub fn invert_selection(&mut self) {
        if self.canvas.selection().is_none() {
            self.set_status("Nothing is selected.");
            return;
        }

        match self.canvas.invert_selection() {
            Some(_) => self.set_status("Inverted the selection."),
            None => {
                self.canvas.clear_selection();
                self.set_status("The whole canvas was selected — selection cleared.");
            }
        }
    }


    pub fn set_selection_mode(&mut self, mode: SelectionMode) {
        self.selection_mode = mode;
        self.set_status(format!("Selection tool: {}", mode.label()));
    }

    pub fn set_feather(&mut self, feather: f32) {
        self.feather = if feather.is_finite() {
            feather.clamp(0.0, MAX_FEATHER)
        } else {
            0.0
        };
    }

    pub fn select_all(&mut self) {
        self.canvas.select_all();
        self.set_status("Selected the whole canvas");
    }

    pub fn deselect(&mut self) {
        self.canvas.clear_selection();
        self.set_status("Selection cleared.");
    }

    /// Begin dragging a shape with the Shape tool.
    pub fn begin_shape_drag(&mut self, at: Pos2) {
        self.end_stroke();
        self.shape_drag = Some((at, at));
    }

    pub fn update_shape_drag(&mut self, at: Pos2) {
        if let Some((start, _)) = self.shape_drag {
            self.shape_drag = Some((start, at));
        }
    }

    /// Commit the previewed shape as one undoable edit.
    pub fn commit_shape(&mut self) {
        let Some((start, end)) = self.shape_drag.take() else {
            return;
        };
        let Some(layer) = self.canvas.active_layer_id() else {
            self.set_status("Select a layer to draw on.");
            return;
        };

        if self.edit_mask && self.canvas.layer_has_mask(layer) {
            self.set_status(
                "Mask editing is on: turn it off to draw shapes on the layer. The brush \
                 and eraser are the mask tools.",
            );
            return;
        }

        let dab = self.brush.dab(self.color, false);
        let Some(region) = self
            .canvas
            .shape_region(self.shape, start, end, dab.radius)
        else {
            self.set_status("The shape is outside the canvas.");
            return;
        };

        let mut recorder = Recorder::new();
        if let Some(surface) = self.canvas.layer_surface(layer) {
            recorder.record_region(surface, region);
        }
        self.canvas.paint_shape(self.shape, start, end, &dab);

        let patches = match self.canvas.layer_surface(layer) {
            Some(surface) => recorder.finish(surface),
            None => Vec::new(),
        };
        if !patches.is_empty() {
            self.history.push(Edit::pixels(layer, patches, "shape"));
            self.set_status(format!("Drew a {}", self.shape.label().to_lowercase()));
        }
    }

    /// Flood fill at a canvas position with the current colour.
    pub fn fill_at(&mut self, at: Pos2) {
        self.end_stroke();
        let Some(layer) = self.canvas.active_layer_id() else {
            self.set_status("Select a layer to fill.");
            return;
        };

        if self.edit_mask && self.canvas.layer_has_mask(layer) {
            self.set_status(
                "Mask editing is on: turn it off to fill the layer. The brush and eraser \
                 are the mask tools.",
            );
            return;
        }

        let Some(mask) = self.canvas.fill_mask_at(at, self.fill_tolerance) else {
            self.set_status("Nothing to fill there.");
            return;
        };

        let mut recorder = Recorder::new();
        if let Some(surface) = self.canvas.layer_surface(layer) {
            recorder.record_region(surface, mask.bounds);
        }

        let count = mask.count();
        if self.canvas.apply_fill(layer, &mask, self.color).is_none() {
            return;
        }

        let patches = match self.canvas.layer_surface(layer) {
            Some(surface) => recorder.finish(surface),
            None => Vec::new(),
        };
        if !patches.is_empty() {
            self.history.push(Edit::pixels(layer, patches, "fill"));
            self.set_status(format!("Filled {count} pixels"));
        }
    }

    /// Fill the whole selection with the current colour as one undo step.
    pub fn fill_selection(&mut self) {
        let Some(selection) = self.canvas.selection() else {
            self.set_status("Nothing is selected.");
            return;
        };
        let Some(layer) = self.canvas.active_layer_id() else {
            self.set_status("Select a layer to fill.");
            return;
        };

        let region = selection.rect();
        let mut recorder = Recorder::new();
        if let Some(surface) = self.canvas.layer_surface(layer) {
            recorder.record_region(surface, region);
        }

        if self.canvas.fill_selection(layer, self.color).is_none() {
            return;
        }

        let patches = match self.canvas.layer_surface(layer) {
            Some(surface) => recorder.finish(surface),
            None => Vec::new(),
        };
        if !patches.is_empty() {
            self.history.push(Edit::pixels(layer, patches, "fill selection"));
            self.set_status("Filled the selection");
        }
    }

    pub fn set_shape(&mut self, shape: ShapeKind) {
        self.shape = shape;
        self.set_status(format!("Shape tool: {}", shape.label()));
    }

    pub fn set_fill_tolerance(&mut self, tolerance: u8) {
        self.fill_tolerance = tolerance;
    }

    pub fn request_quit(&mut self) {
        if self.canvas.modified() && !self.closing {
            self.dialog = Some(Dialog::Unsaved(PendingAction::Quit));
            return;
        }
        self.force_quit();
    }

    /// Close the window without any further questions.
    pub fn force_quit(&mut self) {
        self.dialog = None;
        // A clean exit has either saved the work or been told to drop it, so no
        // recovery prompt should appear next time.
        self.clear_recovery();
        self.closing = true;
    }

    /// Collect keyboard input. Typing in a text field suppresses single-letter
    /// tool shortcuts so renaming a layer never switches tools.
    ///
    /// Note: `Context::egui_wants_keyboard_input` is the wrong test here — it
    /// reports *any* focused widget, and clicking the canvas focuses it, which
    /// would swallow every shortcut after the first stroke. `text_edit_focused`
    /// asks the question that actually matters.
    fn collect_actions(&self, ctx: &Context) -> Actions {
        let typing = ctx.text_edit_focused();
        ctx.input_mut(|input| {
            let ctrl_shift = Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::default()
            };
            let mut actions = Actions {
                new_canvas: input.consume_key(Modifiers::CTRL, Key::N),
                quit: input.consume_key(Modifiers::CTRL, Key::Q),
                save: input.consume_key(Modifiers::CTRL, Key::S),
                save_as: input.consume_key(ctrl_shift, Key::S),
                open_project: input.consume_key(Modifiers::CTRL, Key::O),
                ..Actions::default()
            };

            // While a text field has focus, single-letter keys and Ctrl+Z belong
            // to the field rather than to the canvas.
            if typing {
                return actions;
            }

            actions.redo = input.consume_key(ctrl_shift, Key::Z)
                || input.consume_key(Modifiers::CTRL, Key::Y);
            actions.undo = input.consume_key(Modifiers::CTRL, Key::Z);

            for tool in Tool::ALL {
                if input.key_pressed(tool.shortcut()) {
                    actions.tool = Some(tool);
                    break;
                }
            }

            actions.toggle_panels = input.key_pressed(Key::Tab);
            actions.fit = input.key_pressed(Key::Num0);
            actions.actual_size = input.key_pressed(Key::Num1);
            actions.zoom_in = input.key_pressed(Key::Plus) || input.key_pressed(Key::Equals);
            actions.zoom_out = input.key_pressed(Key::Minus);
            actions.brush_smaller = input.key_pressed(Key::OpenBracket);
            actions.brush_bigger = input.key_pressed(Key::CloseBracket);
            actions.select_all = input.consume_key(Modifiers::CTRL, Key::A);
            actions.deselect = input.consume_key(Modifiers::CTRL, Key::D);
            actions.quick_mask = input.key_pressed(Key::Q);
            actions
        })
    }

    fn apply_actions(&mut self, ctx: &Context, actions: Actions) {
        if actions.quit {
            self.request_quit();
        }
        if actions.new_canvas {
            self.request_new_canvas();
        }
        if actions.open_project {
            self.open_project();
        }
        if actions.save {
            self.save_project();
        }
        if actions.save_as {
            self.save_project_as();
        }
        if actions.undo {
            self.undo();
        }
        if actions.redo {
            self.redo();
        }
        if let Some(tool) = actions.tool {
            self.select_tool(tool);
        }
        if actions.toggle_panels {
            let visible = self.show_tool_rail || self.show_sidebar;
            self.show_tool_rail = !visible;
            self.show_sidebar = !visible;
        }
        if actions.fit {
            self.view.request_fit();
        }
        if actions.actual_size {
            self.view.request_actual_size();
        }
        if actions.zoom_in {
            self.view.zoom_in();
        }
        if actions.zoom_out {
            self.view.zoom_out();
        }
        if actions.quick_mask {
            self.toggle_quick_mask();
        }
        if actions.brush_smaller {
            self.brush.scale_size(1.0 / 1.15);
        }
        if actions.brush_bigger {
            self.brush.scale_size(1.15);
        }
        if actions.select_all {
            self.select_all();
        }
        if actions.deselect {
            self.deselect();
        }

        // Escape closes the front-most dialog, or drops the selection.
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        if self.dialog.is_some() {
            if escape {
                self.close_dialog();
            }
        } else if escape && self.canvas.selection().is_some() {
            self.deselect();
        }

        // Closing the window with unsaved work asks first instead of dropping it.
        if ctx.input(|input| input.viewport().close_requested())
            && !self.closing
            && self.canvas.modified()
            && !matches!(self.dialog, Some(Dialog::Unsaved(_)))
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::Unsaved(PendingAction::Quit));
        }

        if self.closing {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

/// Filename-safe form of a document name, used as the default save name.
fn default_file_stem(name: &str) -> String {
    let mut stem = String::with_capacity(name.len());
    let mut last_dash = false;
    for character in name.trim().chars() {
        if character.is_alphanumeric() {
            stem.push(character);
            last_dash = false;
        } else if !last_dash && !stem.is_empty() {
            stem.push('-');
            last_dash = true;
        }
    }
    let stem = stem.trim_matches('-').to_owned();
    if stem.is_empty() {
        String::from("artwork")
    } else {
        stem
    }
}

/// Append a format extension when the artist did not type one.
fn ensure_extension(mut path: PathBuf, extensions: &[&str]) -> PathBuf {
    let has_known = path
        .extension()
        .map(|extension| {
            let extension = extension.to_string_lossy().to_ascii_lowercase();
            extensions.iter().any(|known| *known == extension)
        })
        .unwrap_or(false);

    if !has_known {
        path.set_extension(extensions[0]);
    }
    path
}

fn display_path(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl InkforgeApp {
    /// One complete frame: shortcuts, resulting actions, then the workspace.
    ///
    /// `eframe::App::ui` is a thin wrapper around this so the headless tests can
    /// drive the same code path, keyboard handling included.
    pub fn frame(&mut self, ui: &mut inkforge::egui::Ui) {
        // The root `Ui` is how egui 0.37 hands us the window surface; panels are
        // nested inside it, and the context is only needed for input and
        // window-level commands.
        let ctx = ui.ctx().clone();
        self.refresh_pen(&ctx);
        let actions = self.collect_actions(&ctx);
        self.apply_actions(&ctx, actions);
        self.maybe_autosave();

        ui::show(self, ui);
        self.update_window_title(&ctx);
    }

    /// Keep the OS window title in step with the document and its unsaved
    /// state. Sent after painting so a new stroke shows up immediately.
    fn update_window_title(&mut self, ctx: &Context) {
        let title = self.window_title();
        if self.last_title != title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

impl eframe::App for InkforgeApp {
    fn ui(&mut self, ui: &mut inkforge::egui::Ui, frame: &mut eframe::Frame) {
        // The pen listener needs the native window, which only exists inside
        // this callback; it installs once and is a no-op afterwards.
        #[cfg(target_os = "windows")]
        crate::input::attach(&*frame);
        #[cfg(not(target_os = "windows"))]
        let _ = frame;

        self.frame(ui);
    }
}

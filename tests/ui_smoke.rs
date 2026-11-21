//! Headless render and input checks for the workspace.
//!
//! This container has no display, so the window cannot be opened here. Running
//! egui's own pass drives the real panel/dialog/input code (layout, ids,
//! painting, shortcuts) and catches problems a type check cannot.

use inkforge::app::InkforgeApp;
use inkforge::app::{Dialog, PendingAction};
use inkforge::egui::{self, pos2, vec2, Rect};
use inkforge::ui;

fn input(size: [f32; 2]) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(size[0], size[1]))),
        ..Default::default()
    }
}

/// Run one UI pass and return how many paint primitives it produced.
fn run_pass(ctx: &egui::Context, app: &mut InkforgeApp, size: [f32; 2]) -> usize {
    let mut output = ctx.run_ui(input(size), |ui| app.frame(ui));
    output.textures_delta.clear();
    output.shapes.len()
}

/// A real backend would upload these; a test only has to consume them, since
/// epaint asserts on dropped texture deltas.
fn consume(mut output: egui::FullOutput) {
    output.textures_delta.clear();
}

#[test]
fn workspace_renders_headless_in_every_dialog_state() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();

    for dialog in [
        None,
        Some(Dialog::NewCanvas),
        Some(Dialog::About),
        Some(Dialog::Shortcuts),
        Some(Dialog::Unsaved(PendingAction::Quit)),
        Some(Dialog::Notice(String::from("The test file could not be opened."))),
        Some(Dialog::Recovery),
    ] {
        let label = format!("{dialog:?}");
        app.dialog = dialog;
        // Two passes: egui panels and windows settle their state on the
        // second frame, which is where id/layout asserts usually surface.
        for _ in 0..2 {
            let shapes = run_pass(&ctx, &mut app, [1440.0, 900.0]);
            assert!(
                shapes > 0,
                "workspace produced no paint primitives for {label}"
            );
        }
    }
}

#[test]
fn workspace_renders_at_the_minimum_window_size() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();

    // Smallest window the app allows (see `main.rs`), with panels both shown
    // and hidden: this is where cramped layout would panic or divide by zero.
    for hidden in [false, true] {
        app.show_tool_rail = !hidden;
        app.show_sidebar = !hidden;
        let shapes = run_pass(&ctx, &mut app, [900.0, 560.0]);
        assert!(shapes > 0, "no paint primitives at minimum size");
    }

    assert!(app.view.zoom() > 0.0);
}

#[test]
fn the_selection_sidebar_renders_each_mode() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.select_tool(inkforge::app::Tool::Selection);

    // Each mode shows different options: the wand adds its tolerance slider.
    for mode in inkforge::engine::SelectionMode::ALL {
        app.set_selection_mode(mode);
        let shapes = run_pass(&ctx, &mut app, [1440.0, 900.0]);
        assert!(shapes > 0, "no paint primitives in {} mode", mode.label());
    }
}

#[test]
fn a_selection_drag_through_the_frame_path_selects_an_area() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    let _ = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    app.select_tool(inkforge::app::Tool::Selection);
    simulate_drag_paint(&ctx, &mut app);

    assert!(
        app.canvas.selection().is_some(),
        "dragging with the selection tool should select an area"
    );

    // The marching-ants overlay must render without panicking.
    let shapes = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    assert!(shapes > 0);
}

#[test]
fn a_group_row_renders_expanded_and_collapsed() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    app.add_group();

    // Two passes settle the list layout; a group row paints a caret and folder
    // icon, children paint thumbnails.
    let shapes = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    assert!(shapes > 0);

    let group = app.canvas.active_entry().id();
    app.toggle_group_collapsed(group);
    let shapes = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    assert!(shapes > 0, "a collapsed group must still render");
}

#[test]
fn dragging_the_pointer_paints_the_canvas() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    // First pass lays the workspace out so the canvas view has a real rect.
    let _ = run_pass(&ctx, &mut app, [1440.0, 900.0]);

    simulate_drag_paint(&ctx, &mut app);

    assert!(
        app.canvas.modified(),
        "painting should mark the document modified"
    );
    assert!(
        has_paint(&mut app),
        "dragging over the canvas painted nothing"
    );
    assert!(
        app.stroke.is_none(),
        "the stroke must end when the button is released"
    );
    assert!(app.history.can_undo(), "the stroke should be undoable");
}

#[test]
fn ctrl_z_and_ctrl_y_drive_pixel_history_through_the_ui() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    let _ = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    simulate_drag_paint(&ctx, &mut app);

    assert!(has_paint(&mut app), "setup: the drag should have painted");

    let ctrl = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };

    press_key(&ctx, &mut app, egui::Key::Z, ctrl);
    assert!(!has_paint(&mut app), "Ctrl+Z should remove the stroke");
    assert!(
        !has_paint(&mut app),
        "no paint may survive an undo of the only stroke"
    );
    assert!(app.history.can_redo());

    press_key(&ctx, &mut app, egui::Key::Y, ctrl);
    assert!(has_paint(&mut app), "Ctrl+Y should bring the stroke back");
}

#[test]
fn q_toggles_the_quick_mask_through_the_ui() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    let _ = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    assert!(!app.quick_mask, "Quick mask starts off");

    press_key(&ctx, &mut app, egui::Key::Q, egui::Modifiers::NONE);
    assert!(app.quick_mask, "Q should turn the Quick mask on");

    press_key(&ctx, &mut app, egui::Key::Q, egui::Modifiers::NONE);
    assert!(!app.quick_mask, "Q should turn it off again");
}

#[test]
fn painting_uploads_only_the_changed_region() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    let _ = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    let whole_canvas = app.canvas.width() as usize * app.canvas.height() as usize;
    assert!(
        app.tiles.uploads() >= 1,
        "the first frame must upload the visible composite tiles"
    );

    simulate_drag_paint(&ctx, &mut app);

    let upload = app
        .tiles
        .last_upload()
        .expect("painting must upload the changed pixels");
    let texels = app.tiles.last_upload_texels();
    eprintln!(
        "uploaded {} × {} canvas px in {texels} texels of {whole_canvas} pixels ({:.2}% of the canvas)",
        upload.width,
        upload.height,
        100.0 * texels as f64 / whole_canvas as f64
    );
    assert!(
        texels < whole_canvas / 8,
        "painting uploaded {texels} of {whole_canvas} pixels; the whole canvas is being copied"
    );
    assert!(
        texels <= 4 * (inkforge::engine::TILE as usize * inkforge::engine::TILE as usize),
        "painting should touch a few tiles, not {texels} pixels"
    );
    assert_eq!(
        app.canvas.composite().dirty(),
        None,
        "an uploaded region must be marked clean"
    );
}

#[test]
fn the_window_title_names_the_document_and_flags_unsaved_work() {
    let mut app = InkforgeApp::with_defaults();
    assert_eq!(app.window_title(), "Untitled — INKFORGE");

    app.canvas.mark_modified();
    assert_eq!(
        app.window_title(),
        "Untitled • — INKFORGE",
        "unsaved work must be visible in the title bar"
    );
}

#[test]
fn a_start_hint_shows_on_an_untouched_canvas_and_leaves_after_the_first_stroke() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    assert!(
        app.document_is_empty(),
        "a fresh document should report no paint"
    );

    let texts = run_pass_text(&ctx, &mut app, [1440.0, 900.0]);
    assert!(
        texts.iter().any(|text| text.contains("Start painting")),
        "an untouched canvas should show the start hint, painted: {texts:?}"
    );

    simulate_drag_paint(&ctx, &mut app);
    assert!(
        !app.document_is_empty(),
        "painting must switch the document out of its empty state"
    );

    let texts = run_pass_text(&ctx, &mut app, [1440.0, 900.0]);
    assert!(
        !texts.iter().any(|text| text.contains("Start painting")),
        "the start hint must disappear once something is painted"
    );

    // Undo takes the only stroke back, so the hint has to return: this proves
    // the cached scan follows the composite revision rather than staying stuck.
    app.undo();
    assert!(
        app.document_is_empty(),
        "undoing the only stroke should empty the document again"
    );
}

#[test]
fn an_empty_group_row_invites_a_drop_and_stops_once_filled() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    app.select_entry(0);
    app.add_group();
    let group = app.canvas.entries()[0].id();

    // Drag the folder's only layer out, which leaves the folder empty.
    app.begin_layer_drag(1);
    {
        let drag = app.layer_drag.as_mut().expect("drag started");
        drag.to = 3;
        drag.parent = None;
    }
    app.finish_layer_drag();
    assert_eq!(app.canvas.entries()[0].id(), group);

    let texts = run_pass_text(&ctx, &mut app, [1440.0, 900.0]);
    assert!(
        texts.iter().any(|text| text.contains("Drag a layer")),
        "an empty folder should invite a drop, painted: {texts:?}"
    );

    // Drop a layer back in, and the line goes away.
    app.begin_layer_drag(1);
    {
        let drag = app.layer_drag.as_mut().expect("drag started");
        drag.to = 1;
        drag.parent = Some(group);
    }
    app.finish_layer_drag();

    let texts = run_pass_text(&ctx, &mut app, [1440.0, 900.0]);
    assert!(
        !texts.iter().any(|text| text.contains("Drag a layer")),
        "the hint must leave once the folder has a layer: {texts:?}"
    );
}

#[test]
fn a_group_row_being_renamed_paints_the_draft_in_place_of_its_name() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    app.select_entry(0);
    app.add_group();
    let group = app.canvas.entries()[0].id();
    let original = app
        .canvas
        .group_props(group)
        .expect("group props")
        .name
        .clone();

    app.begin_rename_entry(group);
    app.rename.as_mut().expect("rename started").text = String::from("Sky studies");

    // Tall window: the layer list sits below the colour/brush panels, and a
    // widget clipped out of view is never painted.
    let texts = run_pass_text(&ctx, &mut app, [1440.0, 2100.0]);
    assert!(
        texts.iter().any(|text| text.contains("Sky studies")),
        "the row should paint the draft name, painted: {texts:?}"
    );
    assert!(
        app.canvas.group_props(group).expect("group props").name == original,
        "typing must not touch the document until the field closes"
    );

    // The field opens with the whole name selected, so typing replaces it. The
    // first pass takes focus; the selection lands on the pass after that.
    let _ = run_pass_text(&ctx, &mut app, [1440.0, 2100.0]);
    let state = egui::widgets::text_edit::TextEditState::load(
        &ctx,
        egui::Id::unique(("layer-rename", group)),
    )
    .expect("the field should store its state");
    let range = state.cursor.char_range().expect("the name should be selected");
    let (a, b) = (range.primary.index.0, range.secondary.index.0);
    assert_eq!(
        (a.min(b), a.max(b)),
        (0, String::from("Sky studies").chars().count())
    );

    // Cancelling leaves the document alone and the row back to its name.
    app.finish_rename(false);
    let texts = run_pass_text(&ctx, &mut app, [1440.0, 2100.0]);
    assert!(!texts.iter().any(|text| text.contains("Sky studies")));
}

#[test]
fn a_layer_row_being_renamed_paints_the_draft_in_place_of_its_name() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let layer = app.canvas.entries()[0].id();
    let original = app
        .canvas
        .layer_props(layer)
        .expect("layer props")
        .name
        .clone();

    app.begin_rename_entry(layer);
    app.rename.as_mut().expect("rename started").text = String::from("Ink pass");

    // Tall window: the layer list sits below the colour/brush panels, and a
    // widget clipped out of view is never painted.
    let texts = run_pass_text(&ctx, &mut app, [1440.0, 2100.0]);
    assert!(
        texts.iter().any(|text| text.contains("Ink pass")),
        "the row should paint the draft name, painted: {texts:?}"
    );
    assert!(
        app.canvas.layer_props(layer).expect("layer props").name == original,
        "typing must not touch the document until the field closes"
    );

    // The same field behaviour as folders: the name opens pre-selected.
    let _ = run_pass_text(&ctx, &mut app, [1440.0, 2100.0]);
    let state = egui::widgets::text_edit::TextEditState::load(
        &ctx,
        egui::Id::unique(("layer-rename", layer)),
    )
    .expect("the field should store its state");
    let range = state.cursor.char_range().expect("the name should be selected");
    let (a, b) = (range.primary.index.0, range.secondary.index.0);
    assert_eq!(
        (a.min(b), a.max(b)),
        (0, String::from("Ink pass").chars().count())
    );
}

#[test]
fn typing_in_the_properties_name_box_renames_the_row_live() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    let layer = app.canvas.entries()[0].id();
    let original = app
        .canvas
        .layer_props(layer)
        .expect("layer props")
        .name
        .clone();

    app.set_name_draft(layer, String::from("Ink pass"));

    // Tall window: the layer list sits below the colour/brush panels, and a
    // widget clipped out of view is never painted.
    let texts = run_pass_text(&ctx, &mut app, [1440.0, 2100.0]);
    assert!(
        texts.iter().any(|text| text.contains("Ink pass")),
        "the row should follow the field while it is typed, painted: {texts:?}"
    );
    assert!(
        app.canvas.layer_props(layer).expect("layer props").name == original,
        "typing must not touch the document until the field closes"
    );

    // Closing the field writes the name once; a single undo takes it back.
    app.finish_name_draft(true);
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        "Ink pass"
    );
    app.undo();
    assert_eq!(
        app.canvas.layer_props(layer).expect("layer props").name,
        original
    );
}

#[test]
fn typing_in_the_properties_name_box_renames_a_folder_row_live() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.add_layer();
    app.select_entry(0);
    app.add_group();
    let group = app.canvas.entries()[0].id();
    let original = app
        .canvas
        .group_props(group)
        .expect("group props")
        .name
        .clone();

    app.set_name_draft(group, String::from("Sky studies"));

    // Tall window: the layer list sits below the colour/brush panels, and a
    // widget clipped out of view is never painted.
    let texts = run_pass_text(&ctx, &mut app, [1440.0, 2100.0]);
    assert!(
        texts.iter().any(|text| text.contains("Sky studies")),
        "the folder row should follow the field while it is typed, painted: {texts:?}"
    );
    assert!(
        app.canvas.group_props(group).expect("group props").name == original,
        "typing must not touch the document until the field closes"
    );

    // Closing the field writes the name once; a single undo takes it back.
    app.finish_name_draft(true);
    assert_eq!(
        app.canvas.group_props(group).expect("group props").name,
        "Sky studies"
    );
    app.undo();
    assert_eq!(
        app.canvas.group_props(group).expect("group props").name,
        original
    );
}

/// Run one pass and collect every string it painted, so a test can assert on
/// visible copy instead of only on state.
fn run_pass_text(ctx: &egui::Context, app: &mut InkforgeApp, size: [f32; 2]) -> Vec<String> {
    let output = ctx.run_ui(input(size), |ui| app.frame(ui));
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    consume(output);
    texts
}

fn collect_text(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, out);
            }
        }
        _ => {}
    }
}

fn has_paint(app: &mut InkforgeApp) -> bool {
    app.canvas
        .composite()
        .pixels()
        .iter()
        .any(|pixel| pixel.a() > 0)
}

/// Press at the canvas centre, drag 60 px and release.
fn simulate_drag_paint(ctx: &egui::Context, app: &mut InkforgeApp) {
    let press_at = pos2(720.0, 450.0);
    let release_at = pos2(780.0, 490.0);
    let size = [1440.0, 900.0];

    let mut press = input(size);
    press.events = vec![
        egui::Event::PointerMoved(press_at),
        egui::Event::PointerButton {
            pos: press_at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
    ];
    consume(ctx.run_ui(press, |ui| app.frame(ui)));

    let mut drag = input(size);
    drag.events = vec![egui::Event::PointerMoved(release_at)];
    consume(ctx.run_ui(drag, |ui| app.frame(ui)));

    let mut release = input(size);
    release.events = vec![egui::Event::PointerButton {
        pos: release_at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    }];
    consume(ctx.run_ui(release, |ui| app.frame(ui)));
}

fn press_key(
    ctx: &egui::Context,
    app: &mut InkforgeApp,
    key: egui::Key,
    modifiers: egui::Modifiers,
) {
    let mut frame = input([1440.0, 900.0]);
    frame.events = vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }];
    consume(ctx.run_ui(frame, |ui| app.frame(ui)));
}

#[test]
fn the_new_canvas_window_renders_every_preset_view() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.dialog = Some(Dialog::NewCanvas);

    // Apply a print preset so the bleed, safe-area and colour rows are live.
    let mut setup = app.new_canvas.setup.clone();
    let a4 = inkforge::io::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == "print.a4")
        .expect("A4 preset");
    setup.apply_preset(&a4);
    setup.set_bleed_mm(3.0);
    setup.set_safe_mm(10.0);
    setup.sketch_layer = true;
    app.new_canvas.setup = setup;
    app.new_canvas.selected = Some(String::from("print.a4"));

    for scope in [
        inkforge::io::Scope::All,
        inkforge::io::Scope::Favorites,
        inkforge::io::Scope::Recent,
    ] {
        app.new_canvas.scope = scope;
        for size in [[1440.0, 900.0], [900.0, 560.0]] {
            for _ in 0..2 {
                let shapes = run_pass(&ctx, &mut app, size);
                assert!(shapes > 0, "the New canvas window painted nothing");
            }
        }
    }

    // A search that matches nothing must still lay out.
    app.new_canvas.search = String::from("no such preset at all");
    let shapes = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    assert!(shapes > 0);
}

#[test]
fn rulers_grid_and_print_guides_render_on_the_workspace() {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.show_rulers = true;
    app.show_grid = true;
    app.show_guides = true;
    app.show_print_marks = true;
    app.units = inkforge::io::Unit::Millimeters;

    // A print canvas with bleed exercises the trim and safe-area guides.
    let canvas = inkforge::engine::Canvas::try_new(
        "Guides",
        640,
        480,
        inkforge::engine::Background::White,
    )
    .expect("canvas");
    app.canvas = canvas;
    app.canvas.set_print_setup(300.0, 1.0, 20.0, 40.0);
    app.view.request_fit();

    let shapes = run_pass(&ctx, &mut app, [1440.0, 900.0]);
    assert!(shapes > 0, "guides produced no paint output");
}

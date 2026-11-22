//! Workspace settings that outlive a session: persisted preferences, the
//! resolution read from an opened PNG, and the canvas background in undo.

use std::fs;
use std::path::PathBuf;

use inkforge::app::InkforgeApp;
use inkforge::egui::{self, pos2, vec2, Color32, Rect};
use inkforge::engine::{Background, Canvas};
use inkforge::io::{self, ImageFormat, Preferences, Unit};

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("inkforge-workspace-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// One real frame through the same path the window uses, so anything the frame
/// does on its own (saving preferences) is exercised.
fn run_frame(ctx: &egui::Context, app: &mut InkforgeApp) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.frame(ui));
    output.textures_delta.clear();
}

#[test]
fn workspace_preferences_are_written_and_restored() {
    let dir = scratch("preferences");
    let ctx = egui::Context::default();
    inkforge::ui::theme::apply(&ctx);

    let mut app = InkforgeApp::with_defaults();
    app.enable_preferences(dir.clone());
    assert!(app.show_sidebar, "a fresh workspace keeps its panels");
    assert_eq!(app.units, Unit::Pixels);

    app.show_sidebar = false;
    app.show_rulers = true;
    app.show_grid = true;
    app.units = Unit::Millimeters;
    app.grid_spacing = 128.0;
    run_frame(&ctx, &mut app);

    assert!(
        dir.join(inkforge::io::settings::SETTINGS_FILE).is_file(),
        "the frame must write the settings file"
    );
    let saved = Preferences::load(&dir);
    assert!(!saved.show_sidebar);
    assert!(saved.show_rulers);
    assert!(saved.show_grid);
    assert_eq!(saved.units, Unit::Millimeters);
    assert_eq!(saved.grid_spacing, 128.0);

    // A later session starts from the saved workspace, not the defaults.
    let mut restarted = InkforgeApp::with_defaults();
    restarted.enable_preferences(dir.clone());
    assert!(!restarted.show_sidebar);
    assert!(restarted.show_rulers);
    assert_eq!(restarted.units, Unit::Millimeters);
    assert_eq!(restarted.grid_spacing, 128.0);
    assert_eq!(restarted.preferences(), saved);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_workspace_without_a_settings_folder_still_saves_when_it_can() {
    let dir = scratch("preferences-nested").join("deeper");
    let mut app = InkforgeApp::with_defaults();
    app.enable_preferences(dir.clone());
    app.show_grid = true;

    // `save_preferences` is exercised through a frame; the folder is created.
    let ctx = egui::Context::default();
    inkforge::ui::theme::apply(&ctx);
    run_frame(&ctx, &mut app);

    assert!(dir.join(inkforge::io::settings::SETTINGS_FILE).is_file());
    let _ = fs::remove_dir_all(dir.parent().expect("parent"));
}

#[test]
fn the_canvas_background_is_undoable() {
    let mut app = InkforgeApp::with_defaults();
    assert_eq!(app.canvas.background(), Background::White);

    app.set_canvas_background(Background::Transparent);
    assert_eq!(app.canvas.background(), Background::Transparent);
    assert!(app.canvas.modified());

    app.undo();
    assert_eq!(
        app.canvas.background(),
        Background::White,
        "undo restores the background"
    );
    app.redo();
    assert_eq!(app.canvas.background(), Background::Transparent);
}

#[test]
fn a_background_colour_drag_is_one_undo_step() {
    let mut app = InkforgeApp::with_defaults();
    app.set_canvas_background(Background::Custom(Color32::from_rgb(20, 40, 60)));
    app.set_canvas_background(Background::Custom(Color32::from_rgb(30, 50, 70)));
    app.set_canvas_background(Background::Custom(Color32::from_rgb(40, 60, 80)));

    assert_eq!(
        app.history.undo_depth(),
        1,
        "consecutive colour changes collapse into one step"
    );
    app.undo();
    assert_eq!(app.canvas.background(), Background::White);

    // A different setting after it is its own step.
    app.redo();
    app.set_canvas_background(Background::Transparent);
    assert_eq!(app.history.undo_depth(), 2);
}

#[test]
fn opening_a_png_keeps_the_resolution_it_declares() {
    let dir = scratch("png-dpi");
    let path = dir.join("proof.png");

    let mut canvas = Canvas::try_new("Proof", 40, 30, Background::White).expect("canvas");
    canvas.set_print_setup(300.0, 1.5, 0.0, 0.0);
    io::export_image(&mut canvas, &path, ImageFormat::Png).expect("export");

    let opened = io::open_image(&path).expect("open");
    assert_eq!((opened.width(), opened.height()), (40, 30));
    assert!(
        (opened.dpi() - 300.0).abs() < 1.0,
        "dpi came back as {}",
        opened.dpi()
    );
    assert!(
        (opened.pixel_aspect() - 1.5).abs() < 0.01,
        "pixel aspect came back as {}",
        opened.pixel_aspect()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_default_png_opens_at_a_sane_resolution() {
    let dir = scratch("png-default-dpi");
    let path = dir.join("plain.png");

    let mut canvas = Canvas::try_new("Plain", 24, 24, Background::White).expect("canvas");
    io::export_image(&mut canvas, &path, ImageFormat::Png).expect("export");

    let opened = io::open_image(&path).expect("open");
    assert!((opened.dpi() - 72.0).abs() < 1.0, "{}", opened.dpi());
    assert!((opened.pixel_aspect() - 1.0).abs() < 0.001);

    let _ = fs::remove_dir_all(&dir);
}

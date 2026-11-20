//! Tests for autosave and crash recovery.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use inkforge::app::{Dialog, InkforgeApp};
use inkforge::egui::{pos2, Color32};
use inkforge::engine::{Background, BrushSettings, Canvas};
use inkforge::io::recovery;

fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "inkforge-recovery-{}-{name}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// An app with a small canvas and no native dialogs anywhere in sight.
fn app_with(dir: &PathBuf) -> InkforgeApp {
    let mut app = InkforgeApp::with_defaults();
    app.canvas = Canvas::try_new("Crash Test", 32, 32, Background::White).expect("canvas");
    let mut brush = BrushSettings::default();
    brush.set_size(8.0);
    brush.set_hardness(1.0);
    brush.set_opacity(1.0);
    app.brush = brush;
    app.enable_autosave(dir.clone(), Duration::ZERO);
    app
}

fn paint(app: &mut InkforgeApp) {
    app.color = Color32::from_rgb(20, 80, 200);
    app.dab_at(pos2(16.0, 16.0));
}

#[test]
fn autosave_writes_a_recoverable_copy_of_unsaved_work() {
    let dir = scratch("write");
    let mut app = app_with(&dir);

    assert!(
        !app.autosave_now(),
        "a clean document has nothing worth autosaving"
    );
    assert!(recovery::inspect(&dir).is_none());

    paint(&mut app);
    assert!(app.canvas.modified(), "painting marks the document dirty");
    assert!(app.autosave_now(), "unsaved work is written");

    let info = recovery::inspect(&dir).expect("the slot is complete");
    assert_eq!(info.name, "Crash Test");
    assert_eq!((info.width, info.height), (32, 32));
    assert_eq!(info.project, None);
    assert!(info.age_secs < 60, "the copy is fresh");

    let (canvas, project) = recovery::load(&dir).expect("load the recovered copy");
    assert_eq!(project, None);
    let id = canvas.active_layer_id().expect("layer");
    let pixel = canvas.layer_surface(id).expect("surface").pixel(16, 16);
    assert_eq!(pixel.a(), 255, "the painted mark survived: {pixel:?}");
    assert!(pixel.b() > 150, "and kept its colour: {pixel:?}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_crash_leaves_work_the_next_launch_offers_to_recover() {
    let dir = scratch("crash");

    // First session: paint, autosave, then vanish without saving or quitting.
    {
        let mut app = app_with(&dir);
        paint(&mut app);
        assert!(app.autosave_now());
    }

    // Second session: the slot is found and offered.
    let mut app = app_with(&dir);
    app.start_session();
    assert!(
        matches!(app.dialog, Some(Dialog::Recovery)),
        "the artist is asked about the recovered work"
    );
    let info = app.recovery.clone().expect("recovery details");
    assert_eq!(info.name, "Crash Test");

    app.restore_recovery();
    assert!(app.dialog.is_none(), "the prompt closes after recovering");
    assert_eq!(app.canvas.name(), "Crash Test");
    assert!(
        app.canvas.modified(),
        "recovered pixels are still unsaved work"
    );
    let id = app.canvas.active_layer_id().expect("layer");
    let pixel = app.canvas.layer_surface(id).expect("surface").pixel(16, 16);
    assert_eq!(pixel.a(), 255, "the artwork came back: {pixel:?}");

    // A fresh copy is written straight away, so a second crash is also covered.
    assert!(
        recovery::inspect(&dir).is_some(),
        "the slot is refreshed after recovering"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn discarding_recovery_removes_the_autosaved_copy() {
    let dir = scratch("discard");
    {
        let mut app = app_with(&dir);
        paint(&mut app);
        assert!(app.autosave_now());
    }

    let mut app = app_with(&dir);
    app.start_session();
    assert!(app.recovery.is_some());

    app.discard_recovery();
    assert!(app.recovery.is_none());
    assert!(app.dialog.is_none());
    assert!(
        recovery::inspect(&dir).is_none(),
        "discarding really deletes the copy"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn keeping_the_recovery_for_later_leaves_it_on_disk() {
    let dir = scratch("keep");
    {
        let mut app = app_with(&dir);
        paint(&mut app);
        assert!(app.autosave_now());
    }

    let mut app = app_with(&dir);
    app.start_session();
    app.postpone_recovery();

    assert!(app.recovery.is_some(), "the details stay available");
    assert!(app.dialog.is_none());
    assert!(
        recovery::inspect(&dir).is_some(),
        "next launch can still offer it"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_damaged_autosave_is_reported_instead_of_crashing() {
    let dir = scratch("damaged");
    {
        let mut app = app_with(&dir);
        paint(&mut app);
        assert!(app.autosave_now());
    }

    // A crash mid-write could leave a broken project next to a valid sidecar.
    fs::write(dir.join(recovery::RECOVERY_FILE), b"half a file").expect("corrupt it");

    let mut app = app_with(&dir);
    app.start_session();
    assert!(app.recovery.is_some());

    app.restore_recovery();
    assert!(
        matches!(app.dialog, Some(Dialog::Notice(_))),
        "the failure is explained, not swallowed"
    );
    assert!(
        app.recovery.is_some(),
        "the slot is not deleted when it cannot be read"
    );
    assert_eq!(app.canvas.name(), "Crash Test", "the session is untouched");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_frame_tick_autosaves_dirty_work() {
    use inkforge::egui::{self, vec2, Rect};

    let dir = scratch("tick");
    let ctx = egui::Context::default();
    let mut app = app_with(&dir);
    paint(&mut app);
    assert!(recovery::inspect(&dir).is_none());

    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.frame(ui));
    output.textures_delta.clear();

    assert!(
        recovery::inspect(&dir).is_some(),
        "a frame tick with unsaved work writes the recovery copy"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn autosave_is_off_for_a_plain_session() {
    let app = InkforgeApp::with_defaults();
    let _ = Duration::from_secs(20);
    assert!(!app.autosave_enabled());
    assert_eq!(app.autosave_interval(), Duration::from_secs(20));
}

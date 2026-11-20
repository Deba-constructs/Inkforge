//! The recent-projects list: ordering, deduplication, pruning and reopening.

use std::fs;
use std::path::{Path, PathBuf};

use inkforge::app::InkforgeApp;
use inkforge::engine::{Background, Canvas};
use inkforge::io;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "inkforge-recent-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn project_file(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.inkforge"));
    let canvas = Canvas::try_new(name, 32, 24, Background::White).expect("canvas");
    io::save_project(&path, &canvas).expect("save project");
    path
}

#[test]
fn remembering_keeps_the_newest_first_and_deduplicates() {
    let dir = scratch("order");
    let a = project_file(&dir, "alpha");
    let b = project_file(&dir, "beta");

    let list = io::recent::remember(&dir, &a);
    assert_eq!(list, vec![a.clone()]);

    let list = io::recent::remember(&dir, &b);
    assert_eq!(list, vec![b.clone(), a.clone()]);

    // Re-opening an existing entry moves it to the top instead of repeating it.
    let list = io::recent::remember(&dir, &a);
    assert_eq!(list, vec![a.clone(), b.clone()]);
    assert_eq!(io::recent::load(&dir), list, "the list must be persisted");
}

#[test]
fn the_list_is_capped_at_eight_entries() {
    let dir = scratch("cap");
    let mut paths = Vec::new();
    for index in 0..10 {
        paths.push(project_file(&dir, &format!("doc-{index}")));
        io::recent::remember(&dir, &paths[index]);
    }

    let list = io::recent::load(&dir);
    assert_eq!(list.len(), io::recent::MAX_RECENT);
    assert_eq!(list[0], paths[9], "the newest project must be first");
    assert!(
        !list.contains(&paths[0]),
        "the oldest entries are dropped once the list is full"
    );
}

#[test]
fn deleted_projects_drop_out_instead_of_failing_later() {
    let dir = scratch("prune");
    let kept = project_file(&dir, "kept");
    let gone = project_file(&dir, "gone");
    io::recent::remember(&dir, &kept);
    io::recent::remember(&dir, &gone);
    fs::remove_file(&gone).expect("delete the project");

    assert_eq!(io::recent::load(&dir), vec![kept]);
}

#[test]
fn a_damaged_list_is_ignored_rather_than_reported() {
    let dir = scratch("damaged");
    fs::write(dir.join(io::recent::RECENT_FILE), b"not json").expect("write junk");

    assert!(io::recent::load(&dir).is_empty());
    let file = project_file(&dir, "fresh");
    assert_eq!(io::recent::remember(&dir, &file), vec![file]);
}

#[test]
fn forget_and_clear_remove_entries() {
    let dir = scratch("forget");
    let a = project_file(&dir, "a");
    let b = project_file(&dir, "b");
    io::recent::remember(&dir, &a);
    io::recent::remember(&dir, &b);

    assert_eq!(io::recent::forget(&dir, &b), vec![a.clone()]);
    io::recent::clear(&dir);
    assert!(io::recent::load(&dir).is_empty());
    assert!(!dir.join(io::recent::RECENT_FILE).is_file());
}

#[test]
fn opening_a_recent_project_through_the_app_loads_it() {
    let dir = scratch("app-open");
    let path = project_file(&dir, "Reopen me");

    let mut app = InkforgeApp::with_defaults();
    app.enable_recent(dir.clone());
    io::recent::remember(&dir, &path);
    app.enable_recent(dir);

    assert_eq!(app.recent_projects().len(), 1);
    app.open_recent(0);

    assert_eq!(app.canvas.name(), "Reopen me");
    assert_eq!(app.project_path.as_deref(), Some(path.as_path()));
    assert!(!app.canvas.modified());
    assert!(
        app.dialog.is_none(),
        "a clean document opened without a prompt"
    );
}

#[test]
fn a_missing_recent_project_is_removed_from_the_menu() {
    let dir = scratch("app-missing");
    let path = project_file(&dir, "vanishing");
    io::recent::remember(&dir, &path);

    let mut app = InkforgeApp::with_defaults();
    app.enable_recent(dir.clone());
    fs::remove_file(&path).expect("delete the project");

    app.open_recent(0);

    assert!(
        app.recent_projects().is_empty(),
        "a project that cannot be opened must leave the recent list"
    );
    assert!(matches!(app.dialog, Some(inkforge::app::Dialog::Notice(_))));
    assert_eq!(app.canvas.name(), "Untitled", "the open document is kept");
}

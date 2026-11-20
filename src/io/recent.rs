//! The recent-projects list shown in File ▸ Open recent.
//!
//! It is a small JSON file in INKFORGE's data folder, newest first. Every
//! entry is checked on load, so a project the artist has deleted or moved drops
//! out of the menu instead of failing when it is clicked. The list is a
//! convenience, never part of a document: a failed write here must not disturb
//! the artwork, so all of these functions are best-effort and return the list
//! the menu should show.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name inside the data folder.
pub const RECENT_FILE: &str = "recent.json";
/// How many projects the menu remembers.
pub const MAX_RECENT: usize = 8;

#[derive(Serialize, Deserialize)]
struct Stored {
    projects: Vec<PathBuf>,
}

/// Read the list, newest first, dropping anything that is no longer a file.
pub fn load(dir: &Path) -> Vec<PathBuf> {
    let Ok(bytes) = fs::read(dir.join(RECENT_FILE)) else {
        return Vec::new();
    };
    let Ok(stored) = serde_json::from_slice::<Stored>(&bytes) else {
        // A damaged list is not worth an error dialog; start a fresh one.
        return Vec::new();
    };
    stored
        .projects
        .into_iter()
        .filter(|path| path.is_file())
        .take(MAX_RECENT)
        .collect()
}

/// Put `path` at the top of the list and save it. Returns the new list.
pub fn remember(dir: &Path, path: &Path) -> Vec<PathBuf> {
    let mut projects = load(dir);
    projects.retain(|known| known != path);
    projects.insert(0, path.to_path_buf());
    projects.truncate(MAX_RECENT);
    write(dir, &projects);
    projects
}

/// Remove one entry, e.g. after the file could not be opened.
pub fn forget(dir: &Path, path: &Path) -> Vec<PathBuf> {
    let mut projects = load(dir);
    projects.retain(|known| known != path);
    write(dir, &projects);
    projects
}

/// Delete the list entirely.
pub fn clear(dir: &Path) {
    let _ = fs::remove_file(dir.join(RECENT_FILE));
}

fn write(dir: &Path, projects: &[PathBuf]) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let stored = Stored {
        projects: projects.to_vec(),
    };
    if let Ok(json) = serde_json::to_vec_pretty(&stored) {
        let _ = fs::write(dir.join(RECENT_FILE), json);
    }
}

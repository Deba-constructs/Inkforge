//! Autosave and crash recovery.
//!
//! While a document has unsaved changes INKFORGE periodically writes it to a
//! recovery slot in the user's data directory, using the same `.inkforge`
//! container as a normal save plus a small JSON sidecar with the original
//! project path and a timestamp. The sidecar is written last, so a half-written
//! recovery is never offered as restorable.
//!
//! On the next launch the slot is inspected; if it holds a complete project the
//! app offers to recover it. Saving the project, discarding the recovery, or
//! closing the app cleanly removes the slot.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::Canvas;

use super::project::{load_project, save_project};

/// The recovery project file inside the recovery directory.
pub const RECOVERY_FILE: &str = "autosave.inkforge";
/// The sidecar describing the recovery project.
pub const META_FILE: &str = "autosave.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Meta {
    name: String,
    width: u32,
    height: u32,
    /// The `.inkforge` file the document belongs to, when it has one.
    project: Option<PathBuf>,
    saved_at: u64,
}

/// What the recovery slot holds, without loading the pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryInfo {
    pub dir: PathBuf,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub project: Option<PathBuf>,
    /// Seconds between the autosave and the moment it was inspected.
    pub age_secs: u64,
}

impl RecoveryInfo {
    /// `"2 minutes ago"`, `"just now"`, …
    pub fn age_label(&self) -> String {
        match self.age_secs {
            0..=45 => String::from("just now"),
            46..=119 => String::from("1 minute ago"),
            seconds if seconds < 3_600 => format!("{} minutes ago", seconds / 60),
            seconds if seconds < 7_200 => String::from("1 hour ago"),
            seconds if seconds < 86_400 => format!("{} hours ago", seconds / 3_600),
            seconds => format!("{} days ago", seconds / 86_400),
        }
    }
}

/// INKFORGE's own folder, following the platform's usual data-directory rules.
///
/// Windows `%APPDATA%\INKFORGE`, macOS `~/Library/Application Support/INKFORGE`,
/// Linux `$XDG_DATA_HOME/inkforge` or `~/.local/share/inkforge`. Autosaves and
/// the recent-projects list live here, not next to the artist's artwork.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(appdata).join("INKFORGE"));
    }
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("INKFORGE"),
            );
        }
    }
    if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(data).join("inkforge"));
    }
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("inkforge")
    })
}

/// Where autosaves live.
pub fn default_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("autosave"))
}

/// Write the current document to the recovery slot.
pub fn save_recovery(dir: &Path, canvas: &Canvas, project: Option<&Path>) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("Could not create the autosave folder: {error}"))?;

    // Pixels first: the sidecar below is what makes the slot count as restorable.
    save_project(&dir.join(RECOVERY_FILE), canvas)?;

    let meta = Meta {
        name: canvas.name().to_owned(),
        width: canvas.width(),
        height: canvas.height(),
        project: project.map(Path::to_path_buf),
        saved_at: unix_now(),
    };
    let json = serde_json::to_vec(&meta)
        .map_err(|error| format!("Could not describe the autosave: {error}"))?;

    let temp = dir.join("autosave.json.part");
    fs::write(&temp, json)
        .map_err(|error| format!("Could not write the autosave description: {error}"))?;
    if fs::rename(&temp, dir.join(META_FILE)).is_err() {
        let _ = fs::remove_file(dir.join(META_FILE));
        fs::rename(&temp, dir.join(META_FILE))
            .map_err(|error| format!("Could not finish the autosave: {error}"))?;
    }
    Ok(())
}

/// Look at the recovery slot without loading the artwork.
pub fn inspect(dir: &Path) -> Option<RecoveryInfo> {
    if !dir.join(RECOVERY_FILE).is_file() {
        return None;
    }

    let meta: Meta = serde_json::from_slice(&fs::read(dir.join(META_FILE)).ok()?).ok()?;
    Some(RecoveryInfo {
        dir: dir.to_path_buf(),
        name: meta.name,
        width: meta.width,
        height: meta.height,
        project: meta.project,
        age_secs: unix_now().saturating_sub(meta.saved_at),
    })
}

/// Load the recovered document and the path it belonged to.
pub fn load(dir: &Path) -> Result<(Canvas, Option<PathBuf>), String> {
    let meta: Meta = serde_json::from_slice(
        &fs::read(dir.join(META_FILE))
            .map_err(|error| format!("Could not read the autosave details: {error}"))?,
    )
    .map_err(|error| format!("The autosave details are damaged: {error}"))?;

    let canvas = load_project(&dir.join(RECOVERY_FILE))?;
    Ok((canvas, meta.project))
}

/// Remove the recovery slot. Missing files are not an error.
pub fn clear(dir: &Path) {
    for name in [RECOVERY_FILE, META_FILE] {
        let _ = fs::remove_file(dir.join(name));
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

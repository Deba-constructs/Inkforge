//! Workspace preferences that persist between sessions.
//!
//! These are the settings that belong to the artist rather than to a document:
//! which panels are open, the ruler unit, which guides are drawn, and the grid
//! spacing. They live in `settings.json` in INKFORGE's data folder, next to the
//! recent-projects list and the canvas preset library.
//!
//! Preferences are a convenience, never part of the artwork: a missing, damaged
//! or read-only file must never block painting, so loading falls back to the
//! defaults and saving only reports a failure in the status line.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::canvas_presets::Unit;

/// File name inside the data folder.
pub const SETTINGS_FILE: &str = "settings.json";
/// Smallest grid spacing the UI offers, in canvas pixels.
pub const MIN_GRID_SPACING: f32 = 4.0;
/// Largest grid spacing the UI offers, in canvas pixels.
pub const MAX_GRID_SPACING: f32 = 1024.0;

fn yes() -> bool {
    true
}

fn default_grid_spacing() -> f32 {
    64.0
}

/// The workspace settings remembered between launches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default = "yes")]
    pub show_tool_rail: bool,
    #[serde(default = "yes")]
    pub show_sidebar: bool,
    #[serde(default)]
    pub units: Unit,
    #[serde(default)]
    pub show_rulers: bool,
    #[serde(default)]
    pub show_grid: bool,
    #[serde(default = "yes")]
    pub show_guides: bool,
    #[serde(default = "yes")]
    pub show_print_marks: bool,
    #[serde(default = "default_grid_spacing")]
    pub grid_spacing: f32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_tool_rail: true,
            show_sidebar: true,
            units: Unit::Pixels,
            show_rulers: false,
            show_grid: false,
            show_guides: true,
            show_print_marks: true,
            grid_spacing: default_grid_spacing(),
        }
    }
}

impl Preferences {
    /// Clamp anything a hand-edited file could carry into a usable setting.
    pub fn sanitised(mut self) -> Self {
        if !self.grid_spacing.is_finite() {
            self.grid_spacing = default_grid_spacing();
        }
        self.grid_spacing = self.grid_spacing.clamp(MIN_GRID_SPACING, MAX_GRID_SPACING);
        self
    }

    /// Read the settings, falling back to the defaults when there is no file or
    /// the file cannot be understood.
    pub fn load(dir: &Path) -> Self {
        let Ok(bytes) = fs::read(dir.join(SETTINGS_FILE)) else {
            return Self::default();
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(preferences) => preferences.sanitised(),
            Err(_) => Self::default(),
        }
    }

    /// Write the settings, creating the data folder if needed.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir)
            .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
        let json = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("could not serialise the settings: {error}"))?;
        fs::write(dir.join(SETTINGS_FILE), json).map_err(|error| {
            format!(
                "could not write {}: {error}",
                dir.join(SETTINGS_FILE).display()
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("inkforge-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = scratch("round-trip");
        let preferences = Preferences {
            show_tool_rail: false,
            units: Unit::Millimeters,
            show_rulers: true,
            show_grid: true,
            show_print_marks: false,
            grid_spacing: 128.0,
            ..Preferences::default()
        };

        preferences.save(&dir).expect("save");
        let loaded = Preferences::load(&dir);
        assert_eq!(loaded, preferences);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_or_damaged_file_gives_the_defaults() {
        let dir = scratch("damaged");
        assert_eq!(Preferences::load(&dir), Preferences::default());

        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join(SETTINGS_FILE), b"not json").expect("junk");
        assert_eq!(Preferences::load(&dir), Preferences::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_absurd_grid_spacing_is_clamped() {
        let dir = scratch("clamp");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(
            dir.join(SETTINGS_FILE),
            br#"{ "grid_spacing": 1.0e12, "units": "inches" }"#,
        )
        .expect("write");
        let loaded = Preferences::load(&dir);
        assert_eq!(loaded.grid_spacing, MAX_GRID_SPACING);
        assert_eq!(loaded.units, Unit::Inches);
        assert!(loaded.show_sidebar, "missing fields keep their defaults");
        let _ = fs::remove_dir_all(&dir);
    }
}

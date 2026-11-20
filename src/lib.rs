//! INKFORGE application library.
//!
//! The binary (`src/main.rs`) only boots the window; everything else lives here
//! so the painting engine, UI and project code can be unit tested from `tests/`.
//!
//! Module map (see DEVELOPMENT.md for what is implemented per milestone):
//!
//! * [`app`]    — application state, tools, shortcuts.
//! * [`engine`] — canvas document model, brush parameters, viewport maths.
//! * [`input`]  — tablet pressure and tilt from the platform (WM_POINTER on Windows).
//! * [`io`]     — `.inkforge` projects and PNG/JPEG import and export.
//! * [`ui`]     — egui panels: top bar, tool rail, sidebar, canvas view, status bar.

// Allow internal modules to refer to the crate by its own name, so `crate::`
// paths and `inkforge::` paths agree with what `tests/` and downstream users see.
extern crate self as inkforge;

pub mod app;
pub mod engine;
pub mod history;
pub mod input;
pub mod io;
pub mod layers;
pub mod ui;

/// Re-export of the GUI toolkit so integration tests and downstream code do not
/// need their own `egui` dependency pinned to exactly the same version.
pub use eframe::egui;

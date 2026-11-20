//! Platform pen input.
//!
//! egui/winit deliver pointer *positions* only, so a drawing tablet's pressure
//! and tilt are read from the window system directly. On Windows the app's
//! window is subclassed to listen for `WM_POINTER` messages, which carry the pen
//! info (`GetPointerPenInfo`); everywhere else [`latest`] is a mouse-equivalent
//! sample so the canvas has one code path for pointers and pens alike.
//!
//! The platform side only publishes a reading — one pressure, two tilt angles —
//! and never touches the brush. The mapping from that reading to a dab lives in
//! [`crate::engine::pen`], which is plain, testable maths.

// Only the Windows pen path reads these layouts, but their tests run on every
// platform, so the module stays compiled everywhere.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod pointer;

#[cfg(target_os = "windows")]
mod windows;

use crate::engine::PenSample;

/// The latest tablet reading.
///
/// Returns [`PenSample::MOUSE`] when the pen is out of contact, when the device
/// reports no pressure, and on platforms without a pen source.
#[cfg(target_os = "windows")]
pub fn latest() -> PenSample {
    windows::latest()
}

/// The latest tablet reading; always a mouse-equivalent sample off Windows.
#[cfg(not(target_os = "windows"))]
pub fn latest() -> PenSample {
    PenSample::MOUSE
}

/// Start listening for tablet input on the native window.
///
/// Harmless to call every frame: the listener is installed once. Returns true
/// when a pen source is in place, which is only ever possible on Windows.
#[cfg(target_os = "windows")]
pub fn attach(window: &impl raw_window_handle::HasWindowHandle) -> bool {
    windows::attach(window)
}

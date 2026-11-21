//! File input and output: the `.inkforge` project format and PNG/JPEG import and
//! export.
//!
//! Everything here works on plain [`Canvas`] values, so the UI layer only has to
//! choose a path and show the status line. Failures come back as human-readable
//! strings — never a panic — because a broken file must not take the app (and
//! the artist's other work) down with it.
//!
//! [`Canvas`]: crate::engine::Canvas

pub mod canvas_presets;
pub mod ora;
pub mod png;
pub mod project;
pub mod raster;
pub mod recent;
pub mod recovery;

pub use canvas_presets::{
    builtin_presets, px_to_unit, unit_to_px, BackgroundSpec, Browse, CanvasPreset, CanvasSetup,
    Category, PresetLibrary, Scope, Sort, Unit,
};
pub use ora::export_ora;
pub use project::{load_project, save_project};
pub use raster::{export_image, open_image, read_image, ImageFormat};
pub use recovery::RecoveryInfo;

use crate::egui::Color32;

/// Convert a premultiplied [`Color32`] (the internal surface layout) to the
/// straight-alpha RGBA bytes PNG and JPEG expect.
pub fn straight_rgba(pixel: Color32) -> [u8; 4] {
    let alpha = pixel.a();
    if alpha == 0 {
        return [0, 0, 0, 0];
    }
    if alpha == 255 {
        return [pixel.r(), pixel.g(), pixel.b(), 255];
    }

    let channel = |value: u8| {
        ((value as u32 * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8
    };
    [channel(pixel.r()), channel(pixel.g()), channel(pixel.b()), alpha]
}

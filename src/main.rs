//! INKFORGE — digital painting application.
//!
//! Entry point: configures the native window and hands control to
//! [`inkforge::app::InkforgeApp`].

// Release builds are windows-subsystem so no console window flashes up next to
// the painting UI. Debug builds keep the console for `println!` diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use inkforge::app::InkforgeApp;
use inkforge::egui;

fn main() -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("INKFORGE")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([900.0, 560.0]);

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "INKFORGE",
        native_options,
        Box::new(|cc| Ok(Box::new(InkforgeApp::new(cc)))),
    )
}

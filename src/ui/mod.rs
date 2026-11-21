//! UI layer: one module per region of the workspace plus shared theming.

pub mod canvas_view;
pub mod dialogs;
pub mod icons;
pub mod new_canvas;
pub mod sidebar;
pub mod status_bar;
pub mod theme;
pub mod tool_rail;
pub mod top_bar;

use inkforge::egui;

use crate::app::InkforgeApp;

/// Height of the top bar, so panels can align their contents to it.
pub const TOP_BAR_HEIGHT: f32 = 56.0;
/// Height of the bottom status strip.
pub const STATUS_BAR_HEIGHT: f32 = 30.0;
/// Width of the left tool rail.
pub const TOOL_RAIL_WIDTH: f32 = 58.0;
/// Default and allowed widths of the right sidebar.
pub const SIDEBAR_WIDTH: f32 = 300.0;
pub const SIDEBAR_MIN_WIDTH: f32 = 250.0;
pub const SIDEBAR_MAX_WIDTH: f32 = 460.0;

/// Draw the whole workspace.
///
/// Panel order matters in egui: side panels take space first, the central panel
/// gets what is left.
pub fn show(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    top_bar::show(app, ui);
    status_bar::show(app, ui);

    if app.show_tool_rail {
        tool_rail::show(app, ui);
    }
    if app.show_sidebar {
        sidebar::show(app, ui);
    }

    canvas_view::show(app, ui);

    // Floating dialogs are context-level windows, not panels.
    let ctx = ui.ctx().clone();
    dialogs::show(app, &ctx);
}

//! Left tool rail.

use inkforge::egui::{self, CornerRadius, Margin, Sense, Stroke, Vec2};

use super::{icons, theme, TOOL_RAIL_WIDTH};
use crate::app::{InkforgeApp, Tool};

pub fn show(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    egui::Panel::left("tool_rail")
        .resizable(false)
        .exact_size(TOOL_RAIL_WIDTH)
        .show_separator_line(false)
        .frame(
            egui::Frame::NONE
                .fill(theme::BAR)
                .inner_margin(Margin::symmetric(8, 10)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;

            for tool in Tool::ALL {
                tool_button(app, ui, tool);
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                current_colour_chip(app, ui);
            });
        });
}

fn tool_button(app: &mut InkforgeApp, ui: &mut egui::Ui, tool: Tool) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(42.0), Sense::click());
    let selected = app.tool == tool;

    if selected {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(9),
            theme::with_alpha(theme::ACCENT, 38),
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(9),
            Stroke::new(1.0, theme::with_alpha(theme::ACCENT, 170)),
            egui::StrokeKind::Inside,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(9), theme::HOVER);
    }

    let colour = if selected {
        theme::ACCENT
    } else if response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };

    icons::paint(
        ui.painter(),
        icons::Icon::for_tool(tool),
        rect.shrink(11.0),
        colour,
    );

    if response.clicked() {
        app.select_tool(tool);
    }

    response.on_hover_text(format!(
        "{} ({})\n{}",
        tool.label(),
        tool.shortcut_label(),
        tool.hint()
    ));
}

/// Compact display of the active colour at the bottom of the rail.
fn current_colour_chip(app: &InkforgeApp, ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(7), app.color);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(7),
        Stroke::new(1.0, theme::LINE),
        egui::StrokeKind::Inside,
    );

    response.on_hover_text(format!(
        "Current colour #{:02X}{:02X}{:02X} — change it in the Colour panel",
        app.color.r(),
        app.color.g(),
        app.color.b()
    ));
}

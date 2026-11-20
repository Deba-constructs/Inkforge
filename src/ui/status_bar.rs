//! Bottom status bar: tool, message and zoom controls.

use inkforge::egui::{
    self, Align, Button, Layout, Margin, Response, RichText, Sense, Stroke, Vec2,
};

use super::{icons, theme, STATUS_BAR_HEIGHT};
use crate::app::InkforgeApp;

pub fn show(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    egui::Panel::bottom("status_bar")
        .exact_size(STATUS_BAR_HEIGHT)
        .show_separator_line(false)
        .frame(
            egui::Frame::NONE
                .fill(theme::BAR)
                .inner_margin(Margin::symmetric(12, 0)),
        )
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter()
                .hline(rect.x_range(), rect.min.y + 0.5, Stroke::new(1.0, theme::LINE));

            ui.horizontal_centered(|ui| {
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter()
                    .circle_filled(dot_rect.center(), 3.0, theme::ACCENT);

                ui.label(
                    RichText::new(app.tool.label())
                        .size(11.0)
                        .color(theme::TEXT),
                );
                ui.label(RichText::new("·").size(11.0).color(theme::TEXT_MUTED));
                ui.label(
                    RichText::new(app.status())
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(Button::new(RichText::new("Fit").size(11.0)).frame(false))
                        .on_hover_text("Fit the canvas to the window (0)")
                        .clicked()
                    {
                        app.view.request_fit();
                    }

                    if icon_button(ui, icons::Icon::ZoomIn, "Zoom in (+)").clicked() {
                        app.view.zoom_in();
                    }

                    let zoom = app.view.zoom_label();
                    if ui
                        .add(
                            Button::new(
                                RichText::new(zoom)
                                    .size(11.0)
                                    .monospace()
                                    .color(theme::TEXT),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Reset to 100% (1)")
                        .clicked()
                    {
                        app.view.request_actual_size();
                    }

                    if icon_button(ui, icons::Icon::ZoomOut, "Zoom out (−)").clicked() {
                        app.view.zoom_out();
                    }

                    ui.label(RichText::new("·").size(11.0).color(theme::TEXT_MUTED));
                    ui.label(
                        RichText::new(app.canvas.dimensions_label())
                            .size(11.0)
                            .monospace()
                            .color(theme::TEXT_MUTED),
                    );

                    // A turned canvas shows its angle here, and the angle is the
                    // button that puts it back upright: the quickest way out of
                    // a view the artist has lost track of.
                    if app.view.rotated() {
                        let angle = app.view.rotation_label();
                        if ui
                            .add(
                                Button::new(
                                    RichText::new(angle)
                                        .size(11.0)
                                        .monospace()
                                        .color(theme::ACCENT),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Canvas rotation — click to turn the canvas upright")
                            .clicked()
                        {
                            app.view.reset_rotation();
                            app.set_status("Canvas turned upright");
                        }
                    }
                });
            });
        });
}

fn icon_button(ui: &mut egui::Ui, icon: icons::Icon, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(5), theme::HOVER);
    }
    icons::paint(
        ui.painter(),
        icon,
        rect.shrink(4.0),
        if response.hovered() {
            theme::TEXT
        } else {
            theme::TEXT_MUTED
        },
    );
    response.on_hover_text(tooltip)
}

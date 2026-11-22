//! The Canvas size window: page size, nine anchors and print setup, for a
//! document that already exists.
//!
//! It stays deliberately small — two size fields, three print fields and the
//! anchor grid — because the common case is nudging a page by a few pixels, not
//! rebuilding it. Cropping and trimming live in the Image menu, where the
//! selection is already in mind.

use inkforge::egui::{self, Align, Align2, DragValue, Layout, RichText, Vec2};

use super::{theme, top_bar};
use crate::app::InkforgeApp;
use crate::engine::{Anchor, MAX_CANVAS_DIM, MAX_DPI, MIN_CANVAS_DIM};

pub fn show(app: &mut InkforgeApp, ctx: &egui::Context) {
    let mut draft = app.canvas_size.clone();
    let mut window_open = true;
    let mut apply = false;
    let mut cancel = false;
    let (current_width, current_height) = (app.canvas.width(), app.canvas.height());

    egui::Window::new("Canvas size")
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(430.0);

            ui.label(
                RichText::new(format!(
                    "Page now: {current_width} × {current_height} px · {:.0} dpi",
                    app.canvas.dpi()
                ))
                .size(11.5)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(10.0);

            egui::Grid::new("canvas-size-fields")
                .num_columns(2)
                .spacing([14.0, 7.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Width").size(12.0).color(theme::TEXT));
                    ui.add(
                        DragValue::new(&mut draft.width)
                            .range(MIN_CANVAS_DIM..=MAX_CANVAS_DIM)
                            .speed(1.0)
                            .suffix(" px"),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Height").size(12.0).color(theme::TEXT));
                    ui.add(
                        DragValue::new(&mut draft.height)
                            .range(MIN_CANVAS_DIM..=MAX_CANVAS_DIM)
                            .speed(1.0)
                            .suffix(" px"),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Resolution").size(12.0).color(theme::TEXT));
                    ui.add(
                        DragValue::new(&mut draft.dpi)
                            .range(1.0..=MAX_DPI)
                            .speed(1.0)
                            .suffix(" dpi"),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Bleed").size(12.0).color(theme::TEXT))
                        .on_hover_text("Extra image on every side, kept for the printer to trim.");
                    ui.add(
                        DragValue::new(&mut draft.bleed_mm)
                            .range(0.0..=50.0)
                            .speed(0.1)
                            .max_decimals(1)
                            .suffix(" mm"),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Safe area").size(12.0).color(theme::TEXT))
                        .on_hover_text("Margin inside the trim edge that stays clear of text.");
                    ui.add(
                        DragValue::new(&mut draft.safe_mm)
                            .range(0.0..=50.0)
                            .speed(0.1)
                            .max_decimals(1)
                            .suffix(" mm"),
                    );
                    ui.end_row();
                });

            ui.add_space(10.0);
            ui.label(theme::section_title("Keep anchored"));
            for row in Anchor::ALL {
                ui.horizontal(|ui| {
                    for anchor in row {
                        let selected = draft.anchor == anchor;
                        if ui
                            .selectable_label(
                                selected,
                                RichText::new(short_label(anchor)).size(12.0),
                            )
                            .on_hover_text(anchor.label())
                            .clicked()
                        {
                            draft.anchor = anchor;
                        }
                    }
                });
            }

            if let Some(error) = &draft.error {
                ui.add_space(6.0);
                ui.label(RichText::new(error).size(11.5).color(theme::ACCENT));
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(top_bar::accent_button("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });

    if apply {
        app.canvas_size = draft;
        app.apply_canvas_size();
    } else if cancel || !window_open {
        app.close_dialog();
    } else {
        // Keep the inline edits, including the error message, between frames.
        app.canvas_size = draft;
    }
}

/// Compact label for one anchor button; the tooltip spells it out.
fn short_label(anchor: Anchor) -> &'static str {
    match anchor {
        Anchor::TopLeft => "TL",
        Anchor::Top => "T",
        Anchor::TopRight => "TR",
        Anchor::Left => "L",
        Anchor::Center => "·",
        Anchor::Right => "R",
        Anchor::BottomLeft => "BL",
        Anchor::Bottom => "B",
        Anchor::BottomRight => "BR",
    }
}

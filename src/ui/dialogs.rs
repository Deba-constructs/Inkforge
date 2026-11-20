//! Floating dialogs: New canvas, About, Keyboard shortcuts.

use inkforge::egui::{
    self, Align, Align2, CornerRadius, DragValue, Layout, Margin, RichText, Sense, Stroke, Vec2,
};

use super::{theme, top_bar};
use crate::app::{Dialog, InkforgeApp, PendingAction};
use crate::engine::Background;

/// Size presets offered in the New canvas dialog.
const PRESETS: [(&str, u32, u32); 4] = [
    ("1920 × 1080", 1920, 1080),
    ("2480 × 3508 · A4 300 dpi", 2480, 3508),
    ("3840 × 2160", 3840, 2160),
    ("2048 × 2048", 2048, 2048),
];

const SHORTCUTS: [(&str, &str); 17] = [
    ("B / E / I / G / M / U / H", "Brush, eraser, colour picker, fill, selection, shapes, pan"),
    ("Mouse wheel", "Zoom at the cursor"),
    ("Middle drag", "Pan the canvas (also Hand tool + left drag)"),
    ("Shift + drag", "Turn the canvas with the Hand tool; it snaps upright"),
    ("0", "Fit canvas to window"),
    ("1", "Actual size (100%)"),
    ("+ / −", "Zoom in / out"),
    ("[ / ]", "Decrease / increase brush size"),
    ("Tab", "Show / hide side panels"),
    ("Ctrl+N", "New canvas"),
    ("Ctrl+Z / Ctrl+Y", "Undo / redo the last edit (also Ctrl+Shift+Z)"),
    ("G · M · U", "Fill, rectangle selection, shapes"),
    ("Ctrl+A / Ctrl+D", "Select the whole canvas / deselect"),
    ("Q", "Show or hide the Quick mask wash over the selection"),
    ("Ctrl+S / Ctrl+Shift+S", "Save the project / save it under a new name"),
    ("Ctrl+O", "Open a .inkforge project"),
    ("Esc · Ctrl+Q", "Close dialog or deselect · quit"),
];

pub fn show(app: &mut InkforgeApp, ctx: &egui::Context) {
    match app.dialog.clone() {
        Some(Dialog::NewCanvas) => new_canvas(app, ctx),
        Some(Dialog::About) => about(app, ctx),
        Some(Dialog::Shortcuts) => shortcuts(app, ctx),
        Some(Dialog::Unsaved(action)) => unsaved(app, ctx, action),
        Some(Dialog::Notice(message)) => notice(app, ctx, &message),
        Some(Dialog::Recovery) => recovery(app, ctx),
        None => {}
    }
}

/// Offer work that was autosaved before an unclean exit.
fn recovery(app: &mut InkforgeApp, ctx: &egui::Context) {
    let mut window_open = true;
    let mut restore = false;
    let mut discard = false;
    let mut keep = false;
    let info = app.recovery.clone();

    egui::Window::new("Recover unsaved work")
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(470.0);

            match &info {
                Some(info) => {
                    ui.label(
                        RichText::new(
                            "INKFORGE found work that was not saved when the app last closed.",
                        )
                        .size(12.0)
                        .color(theme::TEXT),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "\"{}\" · {} × {} px",
                            info.name, info.width, info.height
                        ))
                        .size(12.5)
                        .color(theme::ACCENT),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!("Autosaved {}.", info.age_label()))
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                    );
                    if let Some(project) = &info.project {
                        ui.label(
                            RichText::new(format!("Project file: {}", project.display()))
                                .size(11.0)
                                .color(theme::TEXT_MUTED),
                        );
                    }
                    ui.add_space(6.0);
                    ui.label(theme::pending_note(
                        "Recovering restores every layer as it was; save it with Ctrl+S. \
                         Keeping it leaves the autosave on disk for next time.",
                    ));
                }
                None => {
                    ui.label(theme::pending_note("There is no autosaved work to recover."));
                }
            }

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(top_bar::accent_button("Recover it")).clicked() {
                        restore = true;
                    }
                    if ui.button("Discard").clicked() {
                        discard = true;
                    }
                    if ui.button("Keep for later").clicked() {
                        keep = true;
                    }
                });
            });
        });

    if restore {
        app.restore_recovery();
    } else if discard {
        app.discard_recovery();
    } else if keep || !window_open {
        app.postpone_recovery();
    }
}

/// Ask before throwing away unsaved work.
fn unsaved(app: &mut InkforgeApp, ctx: &egui::Context, action: PendingAction) {
    let mut window_open = true;
    let mut save = false;
    let mut discard = false;
    let mut cancel = false;

    egui::Window::new("Unsaved changes")
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(
                RichText::new(format!(
                    "\"{}\" has changes that are not saved to a project.",
                    app.canvas.name()
                ))
                .size(12.0)
                .color(theme::TEXT),
            );
            ui.add_space(4.0);
            ui.label(theme::pending_note(match action {
                PendingAction::NewCanvas => "Creating a new canvas replaces this document.",
                PendingAction::OpenProject => "Opening a project replaces this document.",
                PendingAction::OpenImage => "Opening an image replaces this document.",
                PendingAction::OpenRecent(_) => "Opening a recent project replaces this document.",
                PendingAction::Quit => "Quitting closes this document.",
            }));

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(top_bar::accent_button("Save and continue")).clicked() {
                        save = true;
                    }
                    if ui.button("Discard changes").clicked() {
                        discard = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });

    if save {
        // A cancelled or failed save keeps the question on screen rather than
        // silently continuing and losing the work.
        if app.save_project() {
            app.proceed(action);
        }
    } else if discard {
        app.proceed(action);
    } else if cancel || !window_open {
        app.close_dialog();
    }
}

/// A file operation failed; the summary lives here, the details in the text.
fn notice(app: &mut InkforgeApp, ctx: &egui::Context, message: &str) {
    let mut window_open = true;
    let mut close = false;

    egui::Window::new("Something went wrong")
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new(message).size(12.0).color(theme::TEXT));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(top_bar::accent_button("Close")).clicked() {
                        close = true;
                    }
                });
            });
        });

    if close || !window_open {
        app.close_dialog();
    }
}

fn new_canvas(app: &mut InkforgeApp, ctx: &egui::Context) {
    let mut draft = app.new_canvas.clone();
    let mut window_open = true;
    let mut create = false;
    let mut cancel = false;

    egui::Window::new("New canvas")
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(430.0);

            ui.label(theme::section_title("Name"));
            ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .hint_text("Untitled")
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(10.0);
            ui.label(theme::section_title("Presets"));
            ui.horizontal_wrapped(|ui| {
                for (label, width, height) in PRESETS {
                    if ui.button(label).clicked() {
                        draft.width = width;
                        draft.height = height;
                    }
                }
            });

            ui.add_space(10.0);
            ui.label(theme::section_title("Pixel size"));
            ui.horizontal(|ui| {
                ui.label(RichText::new("W").size(11.0).color(theme::TEXT_MUTED));
                ui.add(
                    DragValue::new(&mut draft.width)
                        .range(1..=20_000)
                        .speed(8.0)
                        .suffix(" px"),
                );
                ui.label(RichText::new("H").size(11.0).color(theme::TEXT_MUTED));
                ui.add(
                    DragValue::new(&mut draft.height)
                        .range(1..=20_000)
                        .speed(8.0)
                        .suffix(" px"),
                );
                if ui.button("Swap").clicked() {
                    std::mem::swap(&mut draft.width, &mut draft.height);
                }
            });

            let megapixels = (draft.width as f32 * draft.height as f32) / 1_000_000.0;
            let megabytes = megapixels * 4.0;
            let heavy = megabytes > 512.0;
            ui.label(
                RichText::new(format!(
                    "≈ {megapixels:.1} MP · ≈ {megabytes:.0} MB of memory per layer"
                ))
                .size(11.0)
                .color(if heavy { theme::ACCENT } else { theme::TEXT_MUTED }),
            );
            if heavy {
                ui.label(theme::pending_note(
                    "Large canvases work but need plenty of free RAM.",
                ));
            }

            ui.add_space(10.0);
            ui.label(theme::section_title("Background"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut draft.background, Background::White, "White");
                ui.radio_value(
                    &mut draft.background,
                    Background::Transparent,
                    "Transparent",
                );
                let custom = Background::Custom(draft.custom_color);
                ui.radio_value(&mut draft.background, custom, "Custom");

                if matches!(draft.background, Background::Custom(_)) {
                    let mut colour = match draft.background {
                        Background::Custom(colour) => colour,
                        _ => draft.custom_color,
                    };
                    if egui::color_picker::color_edit_button_srgba(
                        ui,
                        &mut colour,
                        egui::color_picker::Alpha::Opaque,
                    )
                    .changed()
                    {
                        draft.custom_color = colour;
                        draft.background = Background::Custom(colour);
                    }
                }
            });

            if let Some(error) = &draft.error {
                ui.add_space(8.0);
                egui::Frame::NONE
                    .fill(theme::with_alpha(theme::ACCENT, 26))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.label(RichText::new(error).size(11.0).color(theme::ACCENT));
                    });
            }

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(top_bar::accent_button("Create canvas")).clicked() {
                        create = true;
                    }
                    if ui.button("Cancel").clicked() {
                        // Applied after the closure: `window_open` is borrowed by
                        // `Window::open` for the duration of `show`.
                        cancel = true;
                    }
                });
            });
        });

    app.new_canvas = draft;
    if create {
        app.create_canvas();
    } else if cancel || !window_open {
        app.close_dialog();
    }
}

fn about(app: &mut InkforgeApp, ctx: &egui::Context) {
    let mut window_open = true;
    let mut close = false;

    egui::Window::new("About INKFORGE")
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(440.0);

            let (rect, _) = ui.allocate_exact_size(Vec2::new(190.0, 26.0), Sense::hover());
            super::icons::paint_logo(ui.painter(), rect, theme::ACCENT, theme::PANEL, theme::TEXT);
            ui.label(
                RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "A desktop painting application for artists, written in Rust. \
                     Rendering runs on the GPU through egui + wgpu.",
                )
                .size(11.5)
                .color(theme::TEXT_MUTED),
            );

            ui.add_space(10.0);
            ui.label(theme::section_title("Build progress"));
            milestone(ui, "1 · Workspace shell, canvas viewport, theme", true);
            milestone(ui, "2 · Painting engine: strokes, eraser", true);
            milestone(ui, "3 · Brush presets, spacing, grain, stabiliser", true);
            milestone(ui, "4 · Layers, visibility, opacity, blending", true);
            milestone(ui, "5 · Selection, fill, shapes (undo/redo landed early)", true);
            milestone(ui, "6 · PNG/JPEG I/O and the .inkforge format", true);
            milestone(ui, "7 · Interface polish and performance", false);
            milestone(ui, "8 · Windows release build and packaging", false);

            ui.add_space(8.0);
            ui.label(theme::pending_note(
                "Builds are honest about what they can do: nothing in the menus is a placeholder.",
            ));

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(top_bar::accent_button("Close")).clicked() {
                        close = true;
                    }
                });
            });
        });

    if close || !window_open {
        app.close_dialog();
    }
}

fn milestone(ui: &mut egui::Ui, text: &str, done: bool) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        if done {
            ui.painter().circle_filled(rect.center(), 3.5, theme::ACCENT);
        } else {
            ui.painter().circle_stroke(
                rect.center(),
                3.0,
                Stroke::new(1.0, theme::TEXT_MUTED),
            );
        }
        ui.label(
            RichText::new(text)
                .size(11.5)
                .color(if done { theme::TEXT } else { theme::TEXT_MUTED }),
        );
    });
}

fn shortcuts(app: &mut InkforgeApp, ctx: &egui::Context) {
    let mut window_open = true;
    let mut close = false;

    egui::Window::new("Keyboard shortcuts")
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(520.0);
            egui::Grid::new("shortcut_grid")
                .num_columns(2)
                .spacing([22.0, 6.0])
                .show(ui, |ui| {
                    for (keys, action) in SHORTCUTS {
                        ui.label(
                            RichText::new(keys)
                                .monospace()
                                .size(11.0)
                                .color(theme::ACCENT),
                        );
                        ui.label(RichText::new(action).size(11.5).color(theme::TEXT));
                        ui.end_row();
                    }
                });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(top_bar::accent_button("Close")).clicked() {
                        close = true;
                    }
                });
            });
        });

    if close || !window_open {
        app.close_dialog();
    }
}

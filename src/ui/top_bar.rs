//! Top bar: brand, menus, brush controls, colour, history buttons, document label.

use inkforge::egui::{
    self, Align, Button, Color32, CornerRadius, DragValue, Layout, Margin, Response, RichText, Sense,
    Stroke, Vec2,
};

use super::{icons, theme, TOP_BAR_HEIGHT};
use crate::app::{Dialog, InkforgeApp};

pub fn show(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    egui::Panel::top("top_bar")
        .exact_size(TOP_BAR_HEIGHT)
        .show_separator_line(false)
        .frame(
            egui::Frame::NONE
                .fill(theme::BAR)
                .inner_margin(Margin::symmetric(14, 0)),
        )
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter()
                .hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, theme::LINE));

            ui.horizontal_centered(|ui| {
                brand(app, ui);
                ui.add_space(10.0);
                menus(app, ui);
                ui.add_space(4.0);
                divider(ui);
                brush_controls(app, ui);
                divider(ui);
                colour_control(app, ui);
                divider(ui);
                history_controls(app, ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    document_badge(app, ui);
                });
            });
        });
}

/// Brand mark + wordmark. Also acts as a subtle "about" shortcut.
fn brand(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(150.0, 28.0), Sense::click());
    icons::paint_logo(ui.painter(), rect, theme::ACCENT, theme::BAR, theme::TEXT);
    if response.clicked() {
        app.open_dialog(Dialog::About);
    }
    response.on_hover_text("INKFORGE — click for version information");
}

fn menus(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| file_menu(app, ui));
        ui.menu_button("Edit", |ui| edit_menu(app, ui));
        ui.menu_button("Image", |ui| image_menu(app, ui));
        ui.menu_button("View", |ui| view_menu(app, ui));
        ui.menu_button("Help", |ui| help_menu(app, ui));
    });
}

fn file_menu(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    if ui
        .add(Button::new("New canvas…").shortcut_text("Ctrl+N"))
        .clicked()
    {
        app.request_new_canvas();
        ui.close();
    }

    if ui
        .add(Button::new("Open project…").shortcut_text("Ctrl+O"))
        .clicked()
    {
        app.open_project();
        ui.close();
    }
    ui.menu_button("Open recent", |ui| recent_menu(app, ui));
    if ui.button("Open image as document…").clicked() {
        app.open_image_file();
        ui.close();
    }
    if ui.button("Import image as layer…").clicked() {
        app.import_image_layer();
        ui.close();
    }

    ui.separator();
    if ui
        .add(Button::new("Save project").shortcut_text("Ctrl+S"))
        .clicked()
    {
        app.save_project();
        ui.close();
    }
    if ui
        .add(Button::new("Save project as…").shortcut_text("Ctrl+Shift+S"))
        .clicked()
    {
        app.save_project_as();
        ui.close();
    }

    ui.separator();
    if ui.button("Export PNG…").clicked() {
        app.export_image(crate::io::ImageFormat::Png);
        ui.close();
    }
    if ui.button("Export JPEG…").clicked() {
        app.export_image(crate::io::ImageFormat::Jpeg);
        ui.close();
    }
    ui.separator();
    if ui
        .add(Button::new("Quit").shortcut_text("Ctrl+Q"))
        .clicked()
    {
        app.request_quit();
        ui.close();
    }
}

/// Recently opened projects, newest first, with the full path in the tooltip.
fn recent_menu(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    let projects = app.recent_projects().to_vec();
    if projects.is_empty() {
        ui.add_enabled(false, Button::new("No recent projects"));
        return;
    }

    for (index, path) in projects.iter().enumerate() {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        if ui
            .add(Button::new(name))
            .on_hover_text(path.display().to_string())
            .clicked()
        {
            app.open_recent(index);
            ui.close();
        }
    }

    ui.separator();
    if ui.button("Clear recent projects").clicked() {
        app.clear_recent();
        ui.close();
    }
}

fn edit_menu(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    let can_undo = app.history.can_undo();
    if ui
        .add_enabled(can_undo, Button::new("Undo").shortcut_text("Ctrl+Z"))
        .clicked()
    {
        app.undo();
        ui.close();
    }

    let can_redo = app.history.can_redo();
    if ui
        .add_enabled(can_redo, Button::new("Redo").shortcut_text("Ctrl+Y"))
        .clicked()
    {
        app.redo();
        ui.close();
    }

    ui.separator();
    let can_flatten = app.canvas.layer_count() > 1;
    if ui
        .add_enabled(can_flatten, Button::new("Flatten image"))
        .on_disabled_hover_text("The image is already a single layer.")
        .clicked()
    {
        app.flatten_image();
        ui.close();
    }

    ui.separator();
    if ui
        .add(Button::new("Select all").shortcut_text("Ctrl+A"))
        .clicked()
    {
        app.select_all();
        ui.close();
    }

    let has_selection = app.canvas.selection().is_some();
    if ui
        .add_enabled(has_selection, Button::new("Deselect").shortcut_text("Esc"))
        .clicked()
    {
        app.deselect();
        ui.close();
    }

    if ui
        .add_enabled(has_selection, Button::new("Fill selection"))
        .clicked()
    {
        app.fill_selection();
        ui.close();
    }

    ui.separator();
    if ui.button("Deselect / reset view").clicked() {
        app.view.request_fit();
        app.set_status("View reset to fit");
        ui.close();
    }
}

/// Flips and rotations of the artwork itself (as opposed to the view).
fn image_menu(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    for transform in crate::engine::Transform::ALL {
        if ui.button(transform.label()).clicked() {
            app.transform_document(transform);
            ui.close();
        }
    }

    ui.separator();
    ui.label(
        RichText::new(format!(
            "Artwork: {} × {} px. These change the pixels, not the view.",
            app.canvas.width(),
            app.canvas.height()
        ))
        .size(11.0)
        .color(theme::TEXT_MUTED),
    );
}

/// Degrees one "Turn canvas" menu click rotates the view by.
const TURN_STEP: f32 = 0.261_799_39;

fn view_menu(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    if ui
        .add(Button::new("Fit canvas").shortcut_text("0"))
        .clicked()
    {
        app.view.request_fit();
        ui.close();
    }
    if ui
        .add(Button::new("Actual size (100%)").shortcut_text("1"))
        .clicked()
    {
        app.view.request_actual_size();
        ui.close();
    }
    if ui
        .add(Button::new("Zoom in").shortcut_text("+"))
        .clicked()
    {
        app.view.zoom_in();
    }
    if ui
        .add(Button::new("Zoom out").shortcut_text("−"))
        .clicked()
    {
        app.view.zoom_out();
    }

    ui.separator();
    if ui
        .add(Button::new("Turn canvas left").shortcut_text("15°"))
        .clicked()
    {
        app.view.rotate_by(-TURN_STEP);
        app.set_status(&format!("Canvas turned to {}", app.view.rotation_label()));
    }
    if ui
        .add(Button::new("Turn canvas right").shortcut_text("15°"))
        .clicked()
    {
        app.view.rotate_by(TURN_STEP);
        app.set_status(&format!("Canvas turned to {}", app.view.rotation_label()));
    }
    if ui
        .add_enabled(
            app.view.rotated(),
            Button::new("Straighten canvas"),
        )
        .clicked()
    {
        app.view.reset_rotation();
        app.set_status("Canvas turned upright");
        ui.close();
    }

    ui.separator();
    ui.checkbox(&mut app.show_tool_rail, "Tool rail");
    ui.checkbox(&mut app.show_sidebar, "Side panel");
    ui.separator();
    if ui
        .add(Button::new("Toggle panels").shortcut_text("Tab"))
        .clicked()
    {
        let visible = app.show_tool_rail || app.show_sidebar;
        app.show_tool_rail = !visible;
        app.show_sidebar = !visible;
    }
}

fn help_menu(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    if ui.button("Keyboard shortcuts").clicked() {
        app.open_dialog(Dialog::Shortcuts);
        ui.close();
    }
    if ui.button("About INKFORGE").clicked() {
        app.open_dialog(Dialog::About);
        ui.close();
    }
}

fn divider(ui: &mut egui::Ui) {
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(1.0, 22.0), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0, theme::LINE),
    );
}

fn small_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(theme::TEXT_MUTED));
}

fn brush_controls(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    small_label(ui, "Size");
    let mut size = app.brush.size();
    if ui
        .add(
            DragValue::new(&mut size)
                .range(1.0..=500.0)
                .speed(0.5)
                .fixed_decimals(0)
                .suffix(" px"),
        )
        .changed()
    {
        app.brush.set_size(size);
    }

    small_label(ui, "Opacity");
    let mut opacity = app.brush.opacity_percent();
    if ui
        .add(
            DragValue::new(&mut opacity)
                .range(0.0..=100.0)
                .speed(0.5)
                .fixed_decimals(0)
                .suffix("%"),
        )
        .changed()
    {
        app.brush.set_opacity(opacity / 100.0);
    }
}

fn colour_control(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    small_label(ui, "Colour");
    egui::color_picker::color_edit_button_srgba(
        ui,
        &mut app.color,
        egui::color_picker::Alpha::Opaque,
    )
    .on_hover_text("Primary colour");
}

fn history_controls(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    let can_undo = app.history.can_undo();
    let undo = icon_button(
        ui,
        icons::Icon::Undo,
        can_undo,
        if can_undo {
            "Undo (Ctrl+Z)"
        } else {
            "Nothing to undo"
        },
    );
    if undo.clicked() {
        app.undo();
    }

    let can_redo = app.history.can_redo();
    let redo = icon_button(
        ui,
        icons::Icon::Redo,
        can_redo,
        if can_redo {
            "Redo (Ctrl+Y)"
        } else {
            "Nothing to redo"
        },
    );
    if redo.clicked() {
        app.redo();
    }
}

/// Square icon button painted with the vector icons.
fn icon_button(ui: &mut egui::Ui, icon: icons::Icon, enabled: bool, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());

    if enabled && response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::HOVER);
    }

    let colour = if enabled {
        theme::TEXT
    } else {
        theme::with_alpha(theme::TEXT_MUTED, 120)
    };
    icons::paint(ui.painter(), icon, rect.shrink(5.0), colour);

    response.on_hover_text(tooltip)
}

fn document_badge(app: &InkforgeApp, ui: &mut egui::Ui) {
    ui.label(
        RichText::new(app.canvas.dimensions_label())
            .size(11.0)
            .color(theme::TEXT_MUTED),
    );

    let name = if app.canvas.modified() {
        format!("{} •", app.canvas.name())
    } else {
        app.canvas.name().to_owned()
    };
    ui.label(RichText::new(name).size(12.0).color(theme::TEXT))
        .on_hover_text(match (&app.project_path, app.canvas.modified()) {
            (Some(path), true) => format!("Unsaved changes — Ctrl+S saves to {}", path.display()),
            (Some(path), false) => format!("Saved to {}", path.display()),
            (None, true) => String::from("Unsaved changes — Ctrl+S saves a .inkforge project"),
            (None, false) => String::from("Not saved as a project yet — Ctrl+S to save"),
        });
}

/// Amber call-to-action button used by dialogs.
pub fn accent_button(text: &str) -> Button<'static> {
    Button::new(RichText::new(text).color(Color32::from_rgb(0x14, 0x12, 0x0C))).fill(theme::ACCENT)
}

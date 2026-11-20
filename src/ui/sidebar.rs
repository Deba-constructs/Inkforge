//! Right sidebar: colour, brush, canvas and layer panels.

use inkforge::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, RichText, Sense,
    Slider, Stroke, Vec2,
};

use super::icons::{self, Icon};
use super::{theme, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH, SIDEBAR_WIDTH};
use crate::app::{InkforgeApp, Tool};
use crate::engine::{
    presets::PRESETS, Background, SelectionMode, ShapeKind, MAX_FEATHER, MAX_GROW,
};
use crate::layers::{BlendMode, Entry, LayerId};

/// Quick-pick palette shown under the colour picker.
const SWATCHES: [Color32; 8] = [
    Color32::from_rgb(0x1B, 0x1B, 0x1E),
    Color32::from_rgb(0xF5, 0xF3, 0xEC),
    Color32::from_rgb(0x8A, 0x80, 0x78),
    Color32::from_rgb(0x2C, 0x4A, 0x7C),
    Color32::from_rgb(0xB5, 0x47, 0x3C),
    Color32::from_rgb(0xD6, 0x9A, 0x3C),
    Color32::from_rgb(0x4C, 0x6B, 0x4F),
    Color32::from_rgb(0x6A, 0x4C, 0x8C),
];

pub fn show(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    egui::Panel::right("sidebar")
        .resizable(true)
        .default_size(SIDEBAR_WIDTH)
        .size_range(egui::Rangef::new(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH))
        .frame(
            egui::Frame::NONE
                .fill(theme::PANEL)
                .inner_margin(Margin::symmetric(14, 12)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    colour_section(app, ui);
                    ui.add_space(14.0);
                    presets_section(app, ui);
                    ui.add_space(14.0);
                    brush_section(app, ui);
                    ui.add_space(14.0);
                    pen_section(app, ui);
                    ui.add_space(14.0);
                    tool_options(app, ui);
                    ui.add_space(14.0);
                    canvas_section(app, ui);
                    ui.add_space(14.0);
                    layers_section(app, ui);
                    ui.add_space(8.0);
                });
        });
}

fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.label(theme::section_title(title));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.x_range(),
                rect.center().y,
                Stroke::new(1.0, theme::LINE_SOFT),
            );
        });
    });
    ui.add_space(4.0);
}

fn colour_section(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    section_header(ui, "Colour");

    egui::Frame::NONE
        .fill(theme::INPUT)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            egui::color_picker::color_picker_color32(
                ui,
                &mut app.color,
                egui::color_picker::Alpha::Opaque,
            );

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                swatch(ui, app.color, Vec2::splat(26.0));
                ui.label(
                    RichText::new(format!(
                        "#{:02X}{:02X}{:02X}",
                        app.color.r(),
                        app.color.g(),
                        app.color.b()
                    ))
                    .monospace()
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
                );
            });
        });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        for colour in SWATCHES {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(5), colour);
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(5),
                Stroke::new(1.0, theme::LINE),
                egui::StrokeKind::Inside,
            );
            if response.clicked() {
                app.color = colour;
                app.set_status(format!(
                    "Colour set to #{:02X}{:02X}{:02X}",
                    colour.r(),
                    colour.g(),
                    colour.b()
                ));
            }
            response.on_hover_text("Use this colour");
        }
    });
}

fn swatch(ui: &mut egui::Ui, colour: Color32, size: Vec2) {
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), colour);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, theme::LINE),
        egui::StrokeKind::Inside,
    );
}

fn brush_section(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    section_header(ui, "Brush");

    let mut size = app.brush.size();
    slider_row(ui, "Size", &format!("{size:.0} px"));
    let response = ui.add(Slider::new(&mut size, 1.0..=500.0).show_value(false));
    if response.changed() {
        app.brush.set_size(size);
    }
    response.on_hover_text("Brush diameter in pixels — [ and ] step it by one");

    let mut opacity = app.brush.opacity_percent();
    slider_row(ui, "Opacity", &format!("{opacity:.0}%"));
    let response = ui.add(Slider::new(&mut opacity, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.brush.set_opacity(opacity / 100.0);
    }
    response.on_hover_text("How solid each dab is; low values build up gradually");

    let mut hardness = app.brush.hardness_percent();
    slider_row(ui, "Hardness", &format!("{hardness:.0}%"));
    let response = ui.add(Slider::new(&mut hardness, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.brush.set_hardness(hardness / 100.0);
    }
    response.on_hover_text("Edge softness of the tip: 100% is crisp, 0% is airbrush-soft");

    let mut spacing = app.brush.spacing_percent();
    slider_row(ui, "Spacing", &format!("{spacing:.0}%"));
    let response = ui.add(Slider::new(&mut spacing, 2.0..=100.0).show_value(false));
    if response.changed() {
        app.brush.set_spacing(spacing / 100.0);
    }
    response.on_hover_text("Distance between dabs as a share of the diameter — wide spacing looks broken");

    let mut grain = app.brush.grain_percent();
    slider_row(ui, "Grain", &format!("{grain:.0}%"));
    let response = ui.add(Slider::new(&mut grain, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.brush.set_grain(grain / 100.0);
    }
    response.on_hover_text("Paper-tooth noise in the tip — it stays in place when you paint over it");

    let mut smoothing = app.brush.smoothing_percent();
    slider_row(ui, "Stabiliser", &format!("{smoothing:.0}%"));
    let response = ui.add(Slider::new(&mut smoothing, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.brush.set_smoothing(smoothing / 100.0);
    }
    response.on_hover_text("Smooths shaky strokes; 0% follows the pointer exactly");

    ui.add_space(4.0);
    ui.label(theme::pending_note(
        "Changing a slider gives a custom brush; pick a preset to start from a known tip.",
    ));
}

/// Tablet panel: what the pen's pressure and tilt drive, and what the pen is
/// reporting right now — so a reading can be seen arriving before painting.
fn pen_section(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    section_header(ui, "Drawing tablet");

    let mut enabled = app.pen.enabled();
    let response = ui.checkbox(&mut enabled, RichText::new("Pressure & tilt").size(12.0));
    if response.changed() {
        app.pen.set_enabled(enabled);
        app.set_status(if enabled {
            "Tablet pressure and tilt shape the brush."
        } else {
            "Tablet input off — strokes paint at the set size and opacity."
        });
    }
    response.on_hover_text(
        "Pen pressure sets how wide and how solid each dab is; leaning the pen fans the tip out",
    );

    let mut size = app.pen.size_from_pressure_percent();
    slider_row(ui, "Size from pressure", &format!("{size:.0}%"));
    let response = ui.add(Slider::new(&mut size, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.pen.set_size_from_pressure(size / 100.0);
    }
    response.on_hover_text(
        "How much a light touch thins the line: 0% keeps one width, 100% starts from a point",
    );

    let mut opacity = app.pen.opacity_from_pressure_percent();
    slider_row(ui, "Opacity from pressure", &format!("{opacity:.0}%"));
    let response = ui.add(Slider::new(&mut opacity, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.pen.set_opacity_from_pressure(opacity / 100.0);
    }
    response.on_hover_text(
        "How much a light touch fades the paint: 0% keeps the set opacity, 100% starts nearly clear",
    );

    let mut tilt = app.pen.size_from_tilt_percent();
    slider_row(ui, "Size from tilt", &format!("{tilt:.0}%"));
    let response = ui.add(Slider::new(&mut tilt, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.pen.set_size_from_tilt(tilt / 100.0);
    }
    response.on_hover_text(
        "Extra width when the pen is laid over: 0% ignores tilt, 100% doubles the mark at a full lean",
    );

    let sample = app.pen_sample();
    let readout = if sample.is_pen() {
        format!(
            "Pen: {:.0}% pressure · {:.0}° tilt",
            sample.pressure() * 100.0,
            sample.tilt_degrees()
        )
    } else if app.pen.enabled() {
        String::from("No pen in contact — a mouse paints at full size and opacity.")
    } else {
        String::from("Tablet input is off.")
    };
    ui.label(theme::pending_note(&readout));
}

fn presets_section(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    section_header(ui, "Presets");

    let active = app.brush.matching_preset();
    for (index, preset) in PRESETS.iter().enumerate() {
        let selected = active == Some(index);
        let text = RichText::new(preset.name)
            .size(12.0)
            .color(if selected { theme::ACCENT } else { theme::TEXT });

        let button = egui::Button::new(text)
            .min_size(Vec2::new(ui.available_width(), 26.0))
            .fill(if selected {
                theme::with_alpha(theme::ACCENT, 28)
            } else {
                theme::INPUT
            })
            .stroke(Stroke::new(
                1.0,
                if selected {
                    theme::with_alpha(theme::ACCENT, 150)
                } else {
                    theme::LINE_SOFT
                },
            ));

        let response = ui.add(button);
        if response.clicked() {
            app.brush.apply_preset(preset);
            app.set_status(format!("Brush preset: {}", preset.name));
        }
        response.on_hover_text(preset.hint);
    }
}

fn slider_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.5).color(theme::TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(11.0).color(theme::TEXT_MUTED));
        });
    });
}

/// Options for the tool that is currently selected.
fn tool_options(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    match app.tool {
        Tool::Shape => {
            section_header(ui, "Shape");
            ui.horizontal_wrapped(|ui| {
                for kind in ShapeKind::ALL {
                    if ui.selectable_label(app.shape == kind, kind.label()).clicked() {
                        app.set_shape(kind);
                    }
                }
            });
            ui.label(theme::pending_note(
                "Drag on the canvas to draw; the current brush tip sets the stroke.",
            ));
        }
        Tool::Selection => {
            section_header(ui, "Selection");
            ui.horizontal_wrapped(|ui| {
                for mode in SelectionMode::ALL {
                    if ui
                        .selectable_label(app.selection_mode == mode, mode.label())
                        .clicked()
                    {
                        app.set_selection_mode(mode);
                    }
                }
            });

            if app.selection_mode == SelectionMode::MagicWand {
                let mut tolerance = app.wand_tolerance as f32;
                slider_row(ui, "Tolerance", &format!("{tolerance:.0}"));
                let response =
                    ui.add(Slider::new(&mut tolerance, 0.0..=128.0).show_value(false));
                if response.changed() {
                    app.set_wand_tolerance(tolerance.round() as u8);
                }
                response.on_hover_text(
                    "How different a neighbouring colour may be and still be selected (0–128)",
                );
            }

            let mut feather = app.feather;
            slider_row(ui, "Feather", &format!("{feather:.0} px"));
            let response = ui.add(Slider::new(&mut feather, 0.0..=MAX_FEATHER).show_value(false));
            if response.changed() {
                app.set_feather(feather);
            }
            response
                .on_hover_text("Softens new selections inward — it applies when the selection is made");
            let mut offset = app.grow_offset;
            slider_row(ui, "Grow / shrink", &format!("{offset:+.0} px"));
            let response = ui.add(
                Slider::new(&mut offset, -(MAX_GROW as f32)..=(MAX_GROW as f32)).show_value(false),
            );
            if response.drag_started() {
                app.begin_grow();
            }
            if response.changed() {
                app.preview_grow(offset);
            }
            if response.drag_stopped() || response.lost_focus() {
                app.finish_grow();
            }
            response.on_hover_text(
                "Drag right to grow the selection, left to shrink it — it springs back to the middle",
            );

            ui.horizontal(|ui| {
                if ui
                    .button("Invert")
                    .on_hover_text("Select everything that is not selected")
                    .clicked()
                {
                    app.invert_selection();
                }
                if ui
                    .selectable_label(app.quick_mask, "Quick mask")
                    .on_hover_text(
                        "Show the whole selection as a wash you can paint (Q) — the brush adds, the eraser removes",
                    )
                    .clicked()
                {
                    app.toggle_quick_mask();
                }
            });

            let hint = if app.quick_mask {
                "Quick mask: paint with the brush to add to the selection, the eraser to remove from it. Q turns it off."
            } else if app.selection_mode == SelectionMode::MagicWand {
                "Click a colour: the connected area of similar colour is selected. Feather softens its edge."
            } else {
                "Drag to select; every edit is clipped to the selection. Feather softens the edge."
            };
            ui.label(theme::pending_note(hint));
        }
        Tool::Fill => {
            section_header(ui, "Fill");
            let mut tolerance = app.fill_tolerance as f32;
            slider_row(ui, "Tolerance", &format!("{tolerance:.0}"));
            let response = ui.add(Slider::new(&mut tolerance, 0.0..=128.0).show_value(false));
            if response.changed() {
                app.set_fill_tolerance(tolerance.round() as u8);
            }
            response.on_hover_text("How different a neighbouring colour may be and still fill (0–128)");
            ui.label(theme::pending_note(
                "Fills the area you click, sampling the visible image and stopping at the selection.",
            ));
        }
        _ => {}
    }
}

fn canvas_section(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    section_header(ui, "Canvas");

    info_row(ui, "Size", &app.canvas.dimensions_label());
    info_row(ui, "Memory / layer", &format!("{:.1} MB", app.canvas.layer_memory_mb()));

    ui.add_space(6.0);
    ui.label(RichText::new("Background").size(11.5).color(theme::TEXT));
    let current = app.canvas.background();
    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(current == Background::White, "White")
            .clicked()
        {
            app.canvas.set_background(Background::White);
            app.set_status("Canvas background: white");
        }
        if ui
            .selectable_label(current == Background::Transparent, "Transparent")
            .clicked()
        {
            app.canvas.set_background(Background::Transparent);
            app.set_status("Canvas background: transparent");
        }

        let mut custom = match current {
            Background::Custom(colour) => colour,
            _ => Color32::from_rgb(0xF2, 0xEF, 0xE6),
        };
        if egui::color_picker::color_edit_button_srgba(
            ui,
            &mut custom,
            egui::color_picker::Alpha::Opaque,
        )
        .changed()
        {
            app.canvas.set_background(Background::Custom(custom));
            app.set_status("Canvas background: custom colour");
        }
    });
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.5).color(theme::TEXT_MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(11.5).color(theme::TEXT));
        });
    });
}

fn layers_section(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    section_header(ui, "Layers");

    let active = app.canvas.active_layer_index();
    let active_entry = app.canvas.active_entry().clone();
    let active_is_group = active_entry.is_group();
    let active_is_top_level = active_entry.parent().is_none();

    ui.horizontal(|ui| {
        if layer_action(ui, Icon::Plus, "Add a layer above the active row", true) {
            app.add_layer();
        }
        if layer_action(
            ui,
            Icon::FolderPlus,
            if active_is_group {
                "Select a layer to group"
            } else {
                "Group the active layer"
            },
            !active_is_group && active_is_top_level,
        ) {
            app.add_group();
        }

        let can_delete = app.canvas.layer_count() > 1;
        let hint = if can_delete {
            "Delete the active row (a group takes its layers with it)"
        } else {
            "A document keeps at least one layer"
        };
        if layer_action(ui, Icon::Trash, hint, can_delete) {
            app.delete_active_layer();
        }

        if layer_action(ui, Icon::ArrowUp, "Move the active row up", true) {
            app.move_active_layer(1);
        }
        if layer_action(ui, Icon::ArrowDown, "Move the active row down", true) {
            app.move_active_layer(-1);
        }
        if layer_action(
            ui,
            Icon::Duplicate,
            if active_is_group {
                "Groups cannot be duplicated yet"
            } else {
                "Duplicate the active layer"
            },
            !active_is_group,
        ) {
            app.duplicate_layer();
        }

        let can_merge = can_merge_down(app);
        let merge_hint = if can_merge {
            "Merge the active layer into the one below"
        } else {
            "There is no layer below at this level to merge into"
        };
        if layer_action(ui, Icon::MergeDown, merge_hint, can_merge) {
            app.merge_layer_down();
        }
    });

    ui.add_space(2.0);
    ui.label(
        RichText::new(format!(
            "{} layers · {} rows · {:.0} MB",
            app.canvas.layer_count(),
            app.canvas.entry_count(),
            app.canvas.total_memory_mb()
        ))
        .size(10.0)
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(4.0);

    let rows = snapshot_rows(app);

    // Thumbnails for layers that no longer exist would leak textures.
    let alive: Vec<u64> = rows
        .iter()
        .filter(|row| !row.is_group)
        .map(|row| row.id)
        .collect();
    app.thumbnails.prune(&alive);

    let display: Vec<RowSnap> = rows.iter().rev().cloned().collect();

    let mut row_rects: Vec<(usize, Rect)> = Vec::new();
    let list = egui::Frame::NONE
        .fill(theme::INPUT)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(190.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    // Top row first, the way image editors show a stack.
                    for row in &display {
                        let rect = layer_row(app, ui, row, row.index == active);
                        row_rects.push((row.index, rect));
                    }
                });
        });

    layer_drag_marker(app, ui, &row_rects, list.response.rect);

    ui.add_space(3.0);
    ui.label(
        RichText::new("Drag a row onto a folder to move it inside.")
            .size(10.0)
            .color(theme::TEXT_MUTED),
    );

    ui.add_space(8.0);
    if active_is_group {
        group_properties(app, ui, active_entry.id());
    } else {
        layer_properties(app, ui);
    }
}

/// A row as the list draws it, captured before the rows are rendered so the
/// loop can mutate the document.
#[derive(Clone)]
struct RowSnap {
    index: usize,
    /// 1 for a layer inside a group.
    depth: u8,
    is_group: bool,
    id: u64,
    name: String,
    visible: bool,
    opacity: f32,
    /// "Lock transparent pixels" is on for this layer.
    alpha_locked: bool,
    collapsed: bool,
    /// A line drawn in place of the children of a group that has none.
    placeholder: bool,
}

/// Rows in stack order (bottom first), groups followed by their children.
/// Collapsed groups leave their children out.
fn snapshot_rows(app: &InkforgeApp) -> Vec<RowSnap> {
    let entries = app.canvas.entries();
    let mut rows = Vec::new();
    let mut index = 0;

    while index < entries.len() {
        match &entries[index] {
            Entry::Group(group) => {
                rows.push(RowSnap {
                    index,
                    depth: 0,
                    is_group: true,
                    id: group.id(),
                    name: group.name().to_owned(),
                    visible: group.visible(),
                    opacity: group.opacity(),
                    alpha_locked: false,
                    collapsed: group.collapsed(),
                    placeholder: false,
                });

                let block_end = index + app.canvas.block_len(index);
                // An empty folder always shows its hint, even if it was
                // collapsed before it was emptied: there is nothing to hide.
                if !group.collapsed() || block_end == index + 1 {
                    let mut child = index + 1;
                    while child < block_end {
                        if let Some(entry) = entries.get(child) {
                            if entry.as_layer().is_some() {
                                rows.push(RowSnap {
                                    index: child,
                                    depth: 1,
                                    is_group: false,
                                    id: entry.id(),
                                    name: entry.name().to_owned(),
                                    visible: entry.visible(),
                                    opacity: entry.opacity(),
                                    alpha_locked: entry
                                        .as_layer()
                                        .is_some_and(|layer| layer.alpha_locked()),
                                    collapsed: false,
                                    placeholder: false,
                                });
                            }
                        }
                        child += 1;
                    }

                    // An empty folder gets a line of its own, where its layers
                    // would be: a bare header reads as a broken row.
                    if block_end == index + 1 {
                        rows.push(RowSnap {
                            index,
                            depth: 1,
                            is_group: true,
                            id: group.id(),
                            name: String::from("Drag a layer onto this folder"),
                            visible: group.visible(),
                            opacity: group.opacity(),
                            alpha_locked: false,
                            collapsed: false,
                            placeholder: true,
                        });
                    }
                }
                index = block_end;
            }
            Entry::Layer(layer) => {
                rows.push(RowSnap {
                    index,
                    depth: 0,
                    is_group: false,
                    id: layer.id(),
                    name: layer.name().to_owned(),
                    visible: layer.visible(),
                    opacity: layer.opacity(),
                    alpha_locked: layer.alpha_locked(),
                    collapsed: false,
                    placeholder: false,
                });
                index += 1;
            }
        }
    }

    rows
}

/// The name a row paints: the properties-panel draft while it is being typed
/// on that row, so the list echoes the field live without the document seeing a
/// keystroke. A blank draft shows the name the entry still has.
fn row_display_name<'a>(app: &'a InkforgeApp, row: &'a RowSnap) -> &'a str {
    match app.name_field.as_ref().filter(|field| field.id == row.id) {
        Some(field) if !field.text.trim().is_empty() => field.text.trim(),
        _ => &row.name,
    }
}

/// One row in the layer list: caret/folder for groups, thumbnail and eye for
/// layers, name and opacity for both.
fn layer_row(app: &mut InkforgeApp, ui: &mut egui::Ui, row: &RowSnap, selected: bool) -> Rect {
    if row.placeholder {
        // Not a row of the document: no widgets, no drag, no selection.
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 20.0),
            Sense::hover(),
        );
        ui.painter().text(
            Pos2::new(rect.left() + 27.0 + row.depth as f32 * 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            &row.name,
            FontId::proportional(10.5),
            theme::with_alpha(theme::TEXT_MUTED, 210),
        );
        return rect;
    }

    // A row being renamed is a text field first: it must not also swallow the
    // clicks and drags meant for the field inside it.
    let renaming = app
        .rename
        .as_ref()
        .is_some_and(|state| state.id == row.id);
    let sense = if renaming {
        Sense::hover()
    } else {
        Sense::click_and_drag()
    };

    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), sense);

    let indent = row.depth as f32 * 14.0;
    let eye_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 15.0 + indent, rect.center().y),
        Vec2::splat(18.0),
    );
    let eye = ui.interact(eye_rect, ui.scope_id().with(("layer-eye", row.id)), Sense::click());

    // A folder with nothing in it has nothing to collapse, so it gets no
    // caret until a layer lands inside: a click that does nothing is worse than
    // no affordance at all.
    let empty_folder = row.is_group && app.canvas.block_len(row.index) <= 1;

    let caret = if row.is_group && !empty_folder {
        let caret_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 12.0 + indent, rect.center().y),
            Vec2::splat(16.0),
        );
        Some(ui.interact(
            caret_rect,
            ui.scope_id().with(("layer-caret", row.id)),
            Sense::click(),
        ))
    } else {
        None
    };

    let thumbnail = if row.is_group {
        None
    } else {
        match app.canvas.layer_surface(row.id) {
            Some(surface) => Some(app.thumbnails.texture_id(ui.ctx(), row.id, surface)),
            None => None,
        }
    };
    let aspect = app.canvas.aspect_ratio();

    let painter = ui.painter();
    let radius = CornerRadius::same(5);
    if selected {
        painter.rect_filled(rect, radius, theme::with_alpha(theme::ACCENT, 34));
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, theme::with_alpha(theme::ACCENT, 120)),
            egui::StrokeKind::Inside,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, radius, theme::HOVER);
    }

    let name_x;

    if row.is_group {
        if !empty_folder {
            let icon = if row.collapsed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            };
            icons::paint(
                painter,
                icon,
                Rect::from_center_size(
                    Pos2::new(rect.left() + 12.0 + indent, rect.center().y),
                    Vec2::splat(14.0),
                ),
                theme::TEXT_MUTED,
            );
        }
        icons::paint(
            painter,
            Icon::Folder,
            Rect::from_center_size(
                Pos2::new(rect.left() + 26.0 + indent, rect.center().y),
                Vec2::splat(15.0),
            ),
            // A folder holding nothing reads quieter than one with layers in it.
            theme::with_alpha(theme::ACCENT, if empty_folder { 110 } else { 200 }),
        );
        name_x = rect.left() + 38.0 + indent;
    } else {
        let (eye_icon, eye_colour) = if row.visible {
            (Icon::Eye, theme::TEXT)
        } else {
            (Icon::EyeOff, theme::TEXT_MUTED)
        };
        icons::paint(painter, eye_icon, eye_rect.shrink(2.0), eye_colour);

        // Thumbnail: a light well so transparent areas read as empty, not black.
        let thumb_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 27.0 + indent, rect.center().y - 9.0),
            Vec2::splat(18.0),
        );
        painter.rect_filled(
            thumb_rect,
            CornerRadius::same(3),
            theme::with_alpha(Color32::WHITE, 20),
        );
        if let Some(texture) = thumbnail {
            painter.image(
                texture,
                fit_rect(thumb_rect, aspect),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        painter.rect_stroke(
            thumb_rect,
            CornerRadius::same(3),
            Stroke::new(1.0, theme::LINE_SOFT),
            egui::StrokeKind::Inside,
        );
        name_x = rect.left() + 51.0 + indent;
    }

    if !renaming {
        painter.text(
            Pos2::new(name_x, rect.center().y),
            Align2::LEFT_CENTER,
            row_display_name(app, row),
            FontId::proportional(11.5),
            if row.visible { theme::TEXT } else { theme::TEXT_MUTED },
        );
    }
    // A locked layer wears a small padlock, so the state is visible without
    // selecting the row.
    if row.alpha_locked {
        icons::paint(
            painter,
            Icon::Lock,
            Rect::from_center_size(
                Pos2::new(rect.right() - 46.0, rect.center().y),
                Vec2::splat(12.0),
            ),
            theme::with_alpha(theme::ACCENT, 220),
        );
    }

    painter.text(
        Pos2::new(rect.right() - 6.0, rect.center().y),
        Align2::RIGHT_CENTER,
        format!("{:.0}%", row.opacity * 100.0),
        FontId::monospace(10.0),
        theme::TEXT_MUTED,
    );

    if renaming {
        let field_rect = Rect::from_min_max(
            Pos2::new(name_x - 3.0, rect.top() + 4.0),
            Pos2::new(rect.right() - 42.0, rect.bottom() - 4.0),
        );

        let (close, commit) = {
            let state = app.rename.as_mut().expect("renaming state");
            let field_id = egui::Id::unique(("layer-rename", row.id));
            let mut text = state.text.clone();

            let mut output = ui
                .scope_builder(egui::UiBuilder::new().max_rect(field_rect), |ui| {
                    egui::TextEdit::singleline(&mut text)
                        .id(field_id)
                        .font(egui::TextStyle::Small)
                        .desired_width(f32::INFINITY)
                        .show(ui)
                })
                .inner;
            state.text = text;

            if state.focus {
                if output.response.has_focus() {
                    // Take focus first, then select the whole name on the next
                    // frame: a field that gains focus puts the caret at the end
                    // by itself, and typing should replace the name instead.
                    output.state.cursor.set_char_range(Some(
                        egui::text::CCursorRange::select_all_str(&state.text),
                    ));
                    output.state.store(ui.ctx(), field_id);
                    state.focus = false;
                } else {
                    output.response.request_focus();
                }
            }

            // Enter and clicking elsewhere commit; Escape drops the edit.
            let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
            (output.response.lost_focus() || escaped, !escaped)
        };

        if close {
            app.finish_rename(commit);
        }
        return rect;
    }

    if row.is_group {
        let children = app.canvas.block_len(row.index).saturating_sub(1);
        let hint = match children {
            0 => String::from(
                "Empty folder — drag a layer onto it to put it inside. Double-click to rename.",
            ),
            1 => String::from("1 layer in this folder. Double-click to rename."),
            n => format!("{n} layers in this folder. Double-click to rename."),
        };
        response = response.on_hover_text(hint);
    } else {
        response = response.on_hover_text("Double-click the name to rename this layer.");
    }

    if let Some(caret) = caret {
        if caret.clicked() {
            app.toggle_group_collapsed(row.id);
            return rect;
        }
    }

    if eye.clicked() {
        if row.is_group {
            app.set_group_visibility(row.id, !row.visible);
        } else {
            app.set_layer_visibility(row.id, !row.visible);
        }
    } else if response.drag_started() {
        app.begin_layer_drag(row.index);
    } else if response.clicked() {
        app.select_entry(row.index);
    }

    // Double-clicking a row opens its name for editing.
    if response.double_clicked() {
        app.begin_rename_entry(row.id);
    }

    rect
}

/// Is there a layer directly below the active one in the same group?
fn can_merge_down(app: &InkforgeApp) -> bool {
    let entries = app.canvas.entries();
    let index = app.canvas.active_layer_index();
    let Some(active) = entries.get(index).and_then(Entry::as_layer) else {
        return false;
    };
    let Some(below) = index
        .checked_sub(1)
        .and_then(|i| entries.get(i))
        .and_then(Entry::as_layer)
    else {
        return false;
    };
    active.group() == below.group()
}

/// Insertion index for a pointer at `pointer_y` over `rows` (visual order).
///
/// `rows` holds only the siblings of the dragged entry, so a drop can never
/// land in the middle of a group's block.
pub fn drop_entry_index(
    rows: &[(usize, Rect)],
    pointer_y: f32,
    block_len: impl Fn(usize) -> usize,
) -> usize {
    if rows.is_empty() {
        return 0;
    }

    for (index, rect) in rows {
        if pointer_y < rect.center().y {
            return *index;
        }
    }

    let (last, _) = rows[rows.len() - 1];
    last + block_len(last)
}

/// Screen y of the insertion line for `slot`.
fn insertion_y(rows: &[(usize, Rect)], slot: usize) -> Option<f32> {
    if rows.is_empty() {
        return None;
    }

    if let Some((_, rect)) = rows.iter().find(|(index, _)| *index == slot) {
        return Some(rect.top());
    }

    // The slot is past the last row, or inside a collapsed group: the line sits
    // at the top edge of the row just below it.
    rows.iter()
        .filter(|(index, _)| *index < slot)
        .max_by_key(|(index, _)| *index)
        .map(|(_, rect)| rect.top())
}

/// Insertion slot and destination group for the pointer over the row list.
///
/// Hovering a folder row puts the layer inside that group; otherwise the flat
/// row order picks the slot, and the slot's group (if any) is where it lands.
fn drop_target(
    app: &InkforgeApp,
    rows: &[(usize, Rect)],
    pointer: Pos2,
    dragged_is_group: bool,
) -> (usize, Option<LayerId>) {
    let slot = drop_entry_index(rows, pointer.y, |index| app.canvas.block_len(index));
    if dragged_is_group {
        // Groups only trade places with other top-level rows.
        return (slot, None);
    }

    if let Some((index, _)) = rows.iter().find(|(_, rect)| rect.contains(pointer)) {
        if let Some(entry) = app.canvas.entry(*index) {
            if entry.is_group() {
                return (index + 1, Some(entry.id()));
            }
            // A row inside a group keeps the drop in that group.
            return (slot, entry.parent());
        }
    }

    (slot, app.canvas.group_at_slot(slot))
}

/// Update the drop slot and draw the insertion line (or the target group)
/// while a row is dragged.
fn layer_drag_marker(app: &mut InkforgeApp, ui: &egui::Ui, rows: &[(usize, Rect)], list: Rect) {
    let Some(drag) = app.layer_drag else {
        return;
    };
    let dragged_is_group = app
        .canvas
        .entry(drag.from)
        .map(Entry::is_group)
        .unwrap_or(false);

    if let Some(pointer) = ui.ctx().pointer_interact_pos() {
        let top_level: Vec<(usize, Rect)>;
        let candidates: &[(usize, Rect)] = if dragged_is_group {
            top_level = rows
                .iter()
                .filter(|(index, _)| {
                    app.canvas
                        .entry(*index)
                        .and_then(Entry::parent)
                        .is_none()
                })
                .cloned()
                .collect();
            &top_level
        } else {
            rows
        };

        let (slot, parent) = drop_target(app, candidates, pointer, dragged_is_group);
        if let Some(active) = app.layer_drag.as_mut() {
            active.to = slot;
            active.parent = parent;
        }
    }

    let Some(drag) = app.layer_drag else {
        return;
    };

    match drag.parent {
        Some(group) => {
            // Outline the folder the row will join, so the target is obvious
            // even when the drop is between its children.
            if let Some((_, rect)) = rows.iter().find(|(index, _)| {
                app.canvas.entry(*index).map(Entry::id) == Some(group)
            }) {
                let rect = *rect;
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(5),
                    theme::with_alpha(theme::ACCENT, 30),
                );
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(5),
                    Stroke::new(1.5, theme::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
        }
        None => {
            if let Some(y) = insertion_y(rows, drag.to) {
                ui.painter().line_segment(
                    [
                        Pos2::new(list.left() + 4.0, y),
                        Pos2::new(list.right() - 4.0, y),
                    ],
                    Stroke::new(2.0, theme::ACCENT),
                );
            }
        }
    }

    if ui.input(|i| i.pointer.any_released()) {
        app.finish_layer_drag();
    }
}

/// Name, opacity and blend mode of the active layer.
fn layer_properties(app: &mut InkforgeApp, ui: &mut egui::Ui) {
    let Some(id) = app.canvas.active_layer_id() else {
        return;
    };
    let Some(props) = app.canvas.layer_props(id) else {
        return;
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new("Name").size(11.5).color(theme::TEXT_MUTED));
        name_field(app, ui, id, &props.name);
    });

    let mut opacity = props.opacity * 100.0;
    slider_row(ui, "Layer opacity", &format!("{opacity:.0}%"));
    let response = ui.add(Slider::new(&mut opacity, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.set_layer_opacity(id, opacity / 100.0);
    }
    response.on_hover_text("Fades this layer only — the artwork underneath shows through");

    ui.add_space(4.0);
    ui.label(RichText::new("Blend mode").size(11.5).color(theme::TEXT_MUTED));
    ui.horizontal_wrapped(|ui| {
        for mode in BlendMode::ALL {
            if ui
                .selectable_label(props.blend == mode, mode.label())
                .clicked()
            {
                app.set_layer_blend(id, mode);
            }
        }
    });

    ui.add_space(6.0);
    let mut locked = props.alpha_locked;
    if ui
        .checkbox(
            &mut locked,
            RichText::new("Lock transparent pixels").size(11.5),
        )
        .on_hover_text(
            "Paint recolours what is already on this layer but cannot add new pixels, \
             and the eraser does nothing: the layer's shape is frozen",
        )
        .changed()
    {
        app.set_layer_alpha_lock(id, locked);
    }
}

/// Name and opacity of the active group.
fn group_properties(app: &mut InkforgeApp, ui: &mut egui::Ui, id: u64) {
    let Some(props) = app.canvas.group_props(id) else {
        return;
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new("Group").size(11.5).color(theme::TEXT_MUTED));
        name_field(app, ui, id, &props.name);
    });

    let mut opacity = props.opacity * 100.0;
    slider_row(ui, "Group opacity", &format!("{opacity:.0}%"));
    let response = ui.add(Slider::new(&mut opacity, 0.0..=100.0).show_value(false));
    if response.changed() {
        app.set_group_opacity(id, opacity / 100.0);
    }
    response.on_hover_text("Fades every layer inside the group together");

    ui.add_space(4.0);
    ui.label(theme::pending_note(
        "Group opacity and visibility apply to every layer inside the group.",
    ));
}

/// Name field of the properties panel for entry `id`.
///
/// The text is buffered in [`InkforgeApp::name_field`] while it is being typed,
/// so nothing reaches the document per keystroke: Enter or clicking away
/// commits the name once (one undo step, like inline renaming), Escape drops it.
fn name_field(app: &mut InkforgeApp, ui: &mut egui::Ui, id: u64, current: &str) {
    let mut name = app
        .name_field
        .as_ref()
        .filter(|field| field.id == id)
        .map(|field| field.text.clone())
        .unwrap_or_else(|| current.to_owned());

    let response = ui.add(
        egui::TextEdit::singleline(&mut name)
            .desired_width(f32::INFINITY)
            .font(egui::TextStyle::Small),
    );
    if response.changed() {
        app.set_name_draft(id, name);
    }

    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escaped && app.name_field.is_some() {
        app.finish_name_draft(false);
    } else if response.lost_focus() {
        app.finish_name_draft(true);
    }

    response.on_hover_text("Type a new name, then press Enter or click away to apply it.");
}

/// Largest rectangle of `aspect` that fits inside `outer`, centred.
fn fit_rect(outer: Rect, aspect: f32) -> Rect {
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    let mut size = outer.size();
    if aspect >= 1.0 {
        size.y = (size.x / aspect).max(1.0);
    } else {
        size.x = (size.y * aspect).max(1.0);
    }
    Rect::from_center_size(outer.center(), size)
}

/// Small square icon button for the layer toolbar.
fn layer_action(ui: &mut egui::Ui, icon: Icon, tooltip: &str, enabled: bool) -> bool {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(24.0), sense);

    let painter = ui.painter();
    if enabled && response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), theme::HOVER);
    }
    let colour = if enabled {
        theme::TEXT
    } else {
        theme::with_alpha(theme::TEXT_MUTED, 90)
    };
    icons::paint(painter, icon, rect.shrink(5.0), colour);

    let response = response.on_hover_text(tooltip);
    enabled && response.clicked()
}

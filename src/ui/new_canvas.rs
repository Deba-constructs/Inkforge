//! The New canvas window: preset browser, canvas configuration and creation.
//!
//! Layout follows the usual professional shape — categories on the left, preset
//! cards in the middle, every setting on the right, Create/Cancel at the bottom —
//! and collapses to two stacked columns when the window is narrow.
//!
//! Nothing here is painted decoration: a card click applies the preset to the
//! setup model, the size fields convert through [`crate::io::canvas_presets`],
//! the star writes the favourite to the library on disk and Create builds a real
//! [`crate::engine::Canvas`] through the same validation the engine uses.

use inkforge::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, DragValue, FontId, Key, Layout, Margin,
    Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Vec2,
};

use super::{theme, top_bar};
use crate::app::{InkforgeApp, NewCanvasState};
use crate::engine::{MAX_CANVAS_DIM, MAX_DPI, MAX_PIXEL_ASPECT, MIN_CANVAS_DIM, MIN_PIXEL_ASPECT};
use crate::io::{BackgroundSpec, CanvasPreset, Category, Scope, Sort, Unit};

use crate::io::canvas_presets::px_to_unit;

/// Card metrics, shared by the grid and the hit-testing.
const CARD_WIDTH: f32 = 212.0;
const CARD_HEIGHT: f32 = 86.0;
const SIDEBAR_WIDTH: f32 = 156.0;
const CONFIG_WIDTH: f32 = 334.0;

/// Something the window asks the app to do after it has finished drawing, so
/// the library is never mutated while it is being read for the card list.
enum Action {
    Save,
    Update,
    Delete,
    Favorite(String),
    Restore,
    Import,
    Export,
}

pub fn show(app: &mut InkforgeApp, ctx: &egui::Context) {
    let mut draft = app.new_canvas.clone();
    let cards = app.presets.browse(&draft.browse());
    let favorites: Vec<bool> = cards
        .iter()
        .map(|preset| app.presets.is_favorite(&preset.id))
        .collect();
    let counts: Vec<usize> = Category::ALL
        .iter()
        .map(|category| app.presets.count(*category))
        .collect();

    let mut window_open = true;
    let mut create = false;
    let mut cancel = false;
    let mut actions: Vec<Action> = Vec::new();

    egui::Window::new("New canvas")
        .open(&mut window_open)
        .collapsible(false)
        .resizable(true)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .default_size(Vec2::new(1060.0, 660.0))
        .min_width(720.0)
        .show(ctx, |ui| {
            ui.set_min_width(700.0);

            // Keyboard: arrows walk the preset list, Enter creates, Esc cancels.
            // A focused text field keeps its own keys.
            let typing = ui.ctx().egui_wants_keyboard_input();
            let mut moved = false;
            if !typing {
                let (down, up) = ui.input(|input| {
                    (
                        input.key_pressed(Key::ArrowDown) || input.key_pressed(Key::ArrowRight),
                        input.key_pressed(Key::ArrowUp) || input.key_pressed(Key::ArrowLeft),
                    )
                });
                if (down || up) && !cards.is_empty() {
                    let current = draft
                        .selected
                        .as_deref()
                        .and_then(|id| cards.iter().position(|preset| preset.id == id));
                    let next = match (current, down, up) {
                        (None, _, _) => 0,
                        (Some(index), true, _) => (index + 1).min(cards.len() - 1),
                        (Some(index), _, true) => index.saturating_sub(1),
                        (Some(index), _, _) => index,
                    };
                    if current != Some(next) {
                        apply_preset(&mut draft, &cards[next]);
                        moved = true;
                    }
                }
                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    create = true;
                }
            }

            let available = ui.available_width();
            let body_height = (ui.available_height() - 52.0).max(220.0);
            let wide = available >= 940.0;

            if wide {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(SIDEBAR_WIDTH, body_height),
                        Layout::top_down(Align::Min),
                        |ui| sidebar(ui, &mut draft, &counts),
                    );
                    ui.add_space(4.0);
                    let centre = (available - SIDEBAR_WIDTH - CONFIG_WIDTH - 26.0).max(240.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(centre, body_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(centre);
                            cards_panel(ui, &mut draft, &cards, &favorites, &mut actions, moved);
                        },
                    );
                    ui.add_space(4.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(CONFIG_WIDTH, body_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(CONFIG_WIDTH);
                            config_panel(ui, &mut draft, &favorites, &cards, &mut actions);
                        },
                    );
                });
            } else {
                // Narrow window: categories become a chip row, the settings sit
                // under the preset grid.
                compact_filters(ui, &mut draft, &counts);
                ui.add_space(4.0);
                ScrollArea::vertical()
                    .id_salt("compact-presets")
                    .max_height(body_height * 0.5)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        cards_grid(ui, &mut draft, &cards, &favorites, &mut actions, moved);
                    });
                ui.separator();
                ScrollArea::vertical()
                    .id_salt("compact-config")
                    .max_height(body_height * 0.5)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        config_panel(ui, &mut draft, &favorites, &cards, &mut actions);
                    });
            }

            footer(ui, &mut draft, &mut create, &mut cancel);
        });

    app.new_canvas = draft;

    // Preset actions run after the window is drawn, against the live library.
    for action in actions {
        let result = match action {
            Action::Save => app.save_canvas_preset(),
            Action::Update => app.update_canvas_preset(),
            Action::Delete => app.delete_canvas_preset(),
            Action::Favorite(id) => {
                app.toggle_preset_favorite(&id);
                Ok(())
            }
            Action::Restore => {
                app.restore_canvas_presets();
                Ok(())
            }
            Action::Import => app.import_canvas_presets(),
            Action::Export => app.export_canvas_presets(),
        };
        if let Err(error) = result {
            app.new_canvas.error = Some(error);
        }
    }

    if create {
        app.create_canvas();
    } else if cancel || !window_open {
        app.dialog = None;
    }
}

/// Adopt a preset's size, resolution, background and margins.
fn apply_preset(draft: &mut NewCanvasState, preset: &CanvasPreset) {
    draft.setup.apply_preset(preset);
    draft.selected = Some(preset.id.clone());
    draft.preset_name = preset.name.clone();
    draft.editing = if preset.category == Category::Custom {
        Some(preset.id.clone())
    } else {
        None
    };
    draft.error = None;
}

fn sidebar(ui: &mut egui::Ui, draft: &mut NewCanvasState, counts: &[usize]) {
    ui.set_width(SIDEBAR_WIDTH);
    ui.label(theme::section_title("Show"));
    for scope in Scope::ALL {
        let selected = draft.scope == scope;
        if ui
            .selectable_label(selected, RichText::new(scope.label()).size(12.0))
            .clicked()
        {
            draft.scope = scope;
            draft.category = None;
        }
    }

    ui.add_space(8.0);
    ui.label(theme::section_title("Categories"));
    let mut all_selected = draft.category.is_none();
    if ui
        .selectable_label(all_selected, RichText::new("All categories").size(12.0))
        .clicked()
    {
        draft.category = None;
        all_selected = true;
    }
    let _ = all_selected;

    for (category, count) in Category::ALL.iter().zip(counts) {
        let selected = draft.category == Some(*category);
        let label = format!("{}  ·  {count}", category.label());
        if ui
            .selectable_label(selected, RichText::new(label).size(12.0))
            .clicked()
        {
            draft.category = Some(*category);
        }
    }
}

fn compact_filters(ui: &mut egui::Ui, draft: &mut NewCanvasState, counts: &[usize]) {
    ui.horizontal_wrapped(|ui| {
        for scope in Scope::ALL {
            if ui
                .selectable_label(
                    draft.scope == scope,
                    RichText::new(scope.label()).size(11.5),
                )
                .clicked()
            {
                draft.scope = scope;
                draft.category = None;
            }
        }
        ui.separator();
        if ui
            .selectable_label(draft.category.is_none(), RichText::new("All").size(11.5))
            .clicked()
        {
            draft.category = None;
        }
        for (category, count) in Category::ALL.iter().zip(counts) {
            let label = format!("{} ({count})", category.label());
            if ui
                .selectable_label(
                    draft.category == Some(*category),
                    RichText::new(label).size(11.5),
                )
                .clicked()
            {
                draft.category = Some(*category);
            }
        }
    });
}

fn cards_panel(
    ui: &mut egui::Ui,
    draft: &mut NewCanvasState,
    cards: &[CanvasPreset],
    favorites: &[bool],
    actions: &mut Vec<Action>,
    moved: bool,
) {
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 132.0).max(120.0);
        ui.add(
            egui::TextEdit::singleline(&mut draft.search)
                .hint_text("Search presets…")
                .desired_width(width),
        );
        egui::ComboBox::from_id_salt("preset_sort")
            .width(124.0)
            .selected_text(draft.sort.label())
            .show_ui(ui, |ui| {
                for sort in Sort::ALL {
                    ui.selectable_value(&mut draft.sort, sort, sort.label());
                }
            });
    });

    ui.add_space(2.0);
    if let Some(category) = draft.category {
        ui.label(
            RichText::new(category.hint())
                .size(10.5)
                .color(theme::TEXT_MUTED),
        );
    } else {
        ui.label(
            RichText::new(format!("{} presets · click one to apply it", cards.len()))
                .size(10.5)
                .color(theme::TEXT_MUTED),
        );
    }
    ui.add_space(4.0);

    ScrollArea::vertical()
        .id_salt("preset_cards")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            cards_grid(ui, draft, cards, favorites, actions, moved);
        });
}

fn cards_grid(
    ui: &mut egui::Ui,
    draft: &mut NewCanvasState,
    cards: &[CanvasPreset],
    favorites: &[bool],
    actions: &mut Vec<Action>,
    moved: bool,
) {
    if cards.is_empty() {
        ui.add_space(12.0);
        ui.label(theme::pending_note(if draft.search.trim().is_empty() {
            "No presets here yet. Save the current settings as a preset."
        } else {
            "No preset matches that search."
        }));
        return;
    }

    ui.horizontal_wrapped(|ui| {
        for (index, preset) in cards.iter().enumerate() {
            let favorite = favorites.get(index).copied().unwrap_or(false);
            let selected = draft.selected.as_deref() == Some(preset.id.as_str());

            let (clicked, starred) = preset_card(ui, preset, favorite, selected);
            if starred {
                actions.push(Action::Favorite(preset.id.clone()));
            } else if clicked {
                apply_preset(draft, preset);
            }

            if moved && selected {
                // Keep a keyboard-selected card visible inside the list.
                let rect = ui.min_rect();
                ui.scroll_to_rect(rect, Some(Align::Center));
            }
        }
    });
}

/// One preset card, painted by hand so the thumbnail has the preset's real
/// proportions. Returns `(clicked, star clicked)`.
fn preset_card(
    ui: &mut egui::Ui,
    preset: &CanvasPreset,
    favorite: bool,
    selected: bool,
) -> (bool, bool) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(CARD_WIDTH, CARD_HEIGHT), Sense::click());
    let hovered = response.hovered();

    let fill = if selected {
        theme::with_alpha(theme::ACCENT, 26)
    } else if hovered {
        theme::HOVER
    } else {
        theme::INPUT
    };
    let border = if selected {
        theme::ACCENT
    } else if hovered {
        theme::with_alpha(theme::ACCENT, 90)
    } else {
        theme::LINE_SOFT
    };

    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, CornerRadius::same(8), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    // Thumbnail: a sheet with the preset's aspect ratio.
    let well = Rect::from_min_size(rect.min + Vec2::new(9.0, 10.0), Vec2::new(52.0, 66.0));
    let scale = (well.width() / preset.width.max(1) as f32)
        .min(well.height() / preset.height.max(1) as f32);
    let size = Vec2::new(
        (preset.width.max(1) as f32 * scale).max(3.0),
        (preset.height.max(1) as f32 * scale).max(3.0),
    );
    let sheet = Rect::from_center_size(well.center(), size);

    let paper = preset.background.to_background().fill();
    match paper {
        Some(colour) => {
            painter.rect_filled(sheet, CornerRadius::same(2), colour);
        }
        None => {
            painter.rect_filled(sheet, CornerRadius::same(2), Color32::from_gray(0xC8));
            painter.rect_filled(
                Rect::from_min_size(
                    sheet.min,
                    Vec2::new(sheet.width() / 2.0, sheet.height() / 2.0),
                ),
                CornerRadius::ZERO,
                Color32::from_gray(0xEC),
            );
            painter.rect_filled(
                Rect::from_min_size(
                    sheet.min + Vec2::new(sheet.width() / 2.0, sheet.height() / 2.0),
                    Vec2::new(sheet.width() / 2.0, sheet.height() / 2.0),
                ),
                CornerRadius::ZERO,
                Color32::from_gray(0xEC),
            );
        }
    }
    painter.rect_stroke(
        sheet,
        CornerRadius::same(2),
        Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 70)),
        StrokeKind::Inside,
    );

    let text_left = rect.left() + 70.0;
    let max_width = rect.width() - 92.0;
    let mut name = preset.name.clone();
    // A crude but reliable truncation: drop characters until the name fits.
    while ui
        .painter()
        .layout_no_wrap(name.clone(), FontId::proportional(11.5), theme::TEXT)
        .size()
        .x
        > max_width
        && name.chars().count() > 4
    {
        name.pop();
    }

    painter.text(
        egui::pos2(text_left, rect.top() + 12.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(11.5),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(text_left, rect.top() + 33.0),
        Align2::LEFT_TOP,
        preset.dimensions_label(),
        FontId::proportional(10.5),
        theme::TEXT_MUTED,
    );
    painter.text(
        egui::pos2(text_left, rect.top() + 51.0),
        Align2::LEFT_TOP,
        preset.detail_label(),
        FontId::proportional(10.0),
        theme::TEXT_MUTED,
    );

    // Favourite star, hit-tested separately from the card.
    let star_rect = Rect::from_min_size(
        egui::pos2(rect.right() - 24.0, rect.top() + 7.0),
        Vec2::splat(17.0),
    );
    let star = ui.interact(
        star_rect,
        ui.auto_id_with(("preset_star", preset.id.as_str())),
        Sense::click(),
    );
    painter.text(
        star_rect.center(),
        Align2::CENTER_CENTER,
        if favorite { "★" } else { "☆" },
        FontId::proportional(13.0),
        if favorite {
            theme::ACCENT
        } else {
            theme::TEXT_MUTED
        },
    );

    let clickable = response.clone().on_hover_text(format!(
        "{} — {} ({})",
        preset.name,
        preset.dimensions_label(),
        preset.detail_label()
    ));

    (clickable.clicked() && !star.clicked(), star.clicked())
}

fn config_panel(
    ui: &mut egui::Ui,
    draft: &mut NewCanvasState,
    favorites: &[bool],
    cards: &[CanvasPreset],
    actions: &mut Vec<Action>,
) {
    ui.set_width(CONFIG_WIDTH);

    ui.label(theme::section_title("Document"));
    ui.add(
        egui::TextEdit::singleline(&mut draft.setup.name)
            .hint_text("Untitled")
            .desired_width(f32::INFINITY),
    );

    ui.add_space(8.0);
    ui.label(theme::section_title("Size"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("canvas_units")
            .width(112.0)
            .selected_text(draft.setup.unit.label())
            .show_ui(ui, |ui| {
                for unit in Unit::ALL {
                    ui.selectable_value(&mut draft.setup.unit, unit, unit.label());
                }
            });
        let unit = draft.setup.unit;
        if unit.is_physical() {
            ui.label(
                RichText::new(format!("at {:.0} dpi", draft.setup.dpi))
                    .size(10.0)
                    .color(theme::TEXT_MUTED),
            );
        }
    });

    let unit = draft.setup.unit;
    let max_value = px_to_unit(MAX_CANVAS_DIM, unit, draft.setup.dpi) as f64;
    let min_value = px_to_unit(MIN_CANVAS_DIM, unit, draft.setup.dpi) as f64;
    let suffix = unit.suffix();

    ui.horizontal(|ui| {
        ui.label(RichText::new("W").size(10.5).color(theme::TEXT_MUTED));
        let mut width = draft.setup.width_value();
        if ui
            .add(
                DragValue::new(&mut width)
                    .range(min_value..=max_value)
                    .speed(unit.step())
                    .max_decimals(unit.decimals())
                    .suffix(format!(" {suffix}")),
            )
            .changed()
        {
            draft.setup.set_width_value(width);
            draft.setup.preset_id = None;
            draft.error = None;
        }
        ui.label(RichText::new("H").size(10.5).color(theme::TEXT_MUTED));
        let mut height = draft.setup.height_value();
        if ui
            .add(
                DragValue::new(&mut height)
                    .range(min_value..=max_value)
                    .speed(unit.step())
                    .max_decimals(unit.decimals())
                    .suffix(format!(" {suffix}")),
            )
            .changed()
        {
            draft.setup.set_height_value(height);
            draft.setup.preset_id = None;
            draft.error = None;
        }
    });

    ui.horizontal(|ui| {
        let portrait = draft.setup.is_portrait();
        if ui
            .selectable_label(portrait, RichText::new("Portrait").size(11.0))
            .clicked()
        {
            if !portrait {
                draft.setup.toggle_orientation();
            }
        }
        if ui
            .selectable_label(!portrait, RichText::new("Landscape").size(11.0))
            .clicked()
        {
            if portrait {
                draft.setup.toggle_orientation();
            }
        }
        if ui
            .button(RichText::new("Swap").size(11.0))
            .on_hover_text("Exchange width and height without changing the size")
            .clicked()
        {
            draft.setup.toggle_orientation();
        }
    });

    // Aspect lock and quick ratios.
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut draft.setup.aspect_lock,
            RichText::new("Lock ratio").size(11.0),
        );
        ui.label(
            RichText::new(draft.setup.aspect_label())
                .size(11.0)
                .monospace()
                .color(theme::TEXT_MUTED),
        );
    });
    ui.horizontal_wrapped(|ui| {
        for (label, numerator, denominator) in [
            ("1:1", 1, 1),
            ("4:5", 4, 5),
            ("3:4", 3, 4),
            ("2:3", 2, 3),
            ("16:9", 16, 9),
            ("9:16", 9, 16),
        ] {
            if ui.small_button(label).clicked() {
                draft.setup.set_aspect(numerator, denominator);
                draft.setup.preset_id = None;
            }
        }
    });

    ui.add_space(8.0);
    ui.label(theme::section_title("Resolution"));
    ui.horizontal(|ui| {
        ui.label(RichText::new("DPI").size(10.5).color(theme::TEXT_MUTED));
        let mut dpi = draft.setup.dpi;
        if ui
            .add(
                DragValue::new(&mut dpi)
                    .range(1.0..=MAX_DPI)
                    .speed(1.0)
                    .fixed_decimals(0)
                    .suffix(" dpi"),
            )
            .changed()
        {
            draft.setup.dpi = dpi;
            draft.setup.preset_id = None;
            draft.error = None;
        }
        ui.label(RichText::new("PPI").size(10.0).color(theme::TEXT_MUTED));
    });
    let (mm_w, mm_h) = draft.setup.trim_size_mm();
    ui.label(
        RichText::new(format!(
            "{mm_w:.1} × {mm_h:.1} mm  ·  {:.2} × {:.2} in",
            mm_w / 25.4,
            mm_h / 25.4
        ))
        .size(10.5)
        .color(theme::TEXT_MUTED),
    );
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Pixel aspect")
                .size(10.5)
                .color(theme::TEXT_MUTED),
        );
        let mut aspect = draft.setup.pixel_aspect;
        if ui
            .add(
                DragValue::new(&mut aspect)
                    .range(MIN_PIXEL_ASPECT..=MAX_PIXEL_ASPECT)
                    .speed(0.01)
                    .max_decimals(2),
            )
            .changed()
        {
            draft.setup.pixel_aspect = aspect;
            draft.setup.preset_id = None;
        }
    });
    ui.label(theme::pending_note(
        "1.00 is square. Other values mark anamorphic video pixels.",
    ));

    ui.add_space(8.0);
    ui.label(theme::section_title("Colour"));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mode").size(10.5).color(theme::TEXT_MUTED));
        ui.label(
            RichText::new("RGB · 8-bit · sRGB")
                .size(11.0)
                .color(theme::TEXT),
        );
    });
    ui.label(theme::pending_note(
        "INKFORGE composites 8-bit sRGB, so those presets are the only ones it \
         can honour. CMYK, Grayscale and 16/32-bit depth need a colour-managed \
         high-depth engine and are not offered.",
    ));

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.radio_value(&mut draft.setup.background, BackgroundSpec::White, "White");
        ui.radio_value(&mut draft.setup.background, BackgroundSpec::Black, "Black");
        ui.radio_value(
            &mut draft.setup.background,
            BackgroundSpec::Transparent,
            "Transparent",
        );
    });
    ui.horizontal(|ui| {
        let custom = matches!(draft.setup.background, BackgroundSpec::Custom { .. });
        if ui
            .selectable_label(custom, RichText::new("Custom colour").size(11.0))
            .clicked()
        {
            if let BackgroundSpec::Custom { .. } = draft.setup.background {
                // Already custom: keep the colour.
            } else {
                draft.setup.background = BackgroundSpec::Custom {
                    r: 0xF2,
                    g: 0xEF,
                    b: 0xE6,
                };
            }
        }
        if let BackgroundSpec::Custom { r, g, b } = draft.setup.background {
            let mut colour = Color32::from_rgb(r, g, b);
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut colour,
                egui::color_picker::Alpha::Opaque,
            )
            .changed()
            {
                draft.setup.background = BackgroundSpec::Custom {
                    r: colour.r(),
                    g: colour.g(),
                    b: colour.b(),
                };
            }
        }
    });

    ui.add_space(8.0);
    ui.label(theme::section_title("Print guides"));
    let mut bleed_mm = draft.setup.bleed_mm();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Bleed").size(10.5).color(theme::TEXT_MUTED));
        if ui
            .add(
                DragValue::new(&mut bleed_mm)
                    .range(0.0..=50.0)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(" mm"),
            )
            .changed()
        {
            draft.setup.set_bleed_mm(bleed_mm);
            draft.error = None;
        }
        for quick in [3.0_f32, 5.0] {
            if ui.small_button(format!("{quick:.0}")).clicked() {
                draft.setup.set_bleed_mm(quick);
                draft.error = None;
            }
        }
    });
    let mut safe_mm = draft.setup.safe_mm();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Safe area")
                .size(10.5)
                .color(theme::TEXT_MUTED),
        );
        if ui
            .add(
                DragValue::new(&mut safe_mm)
                    .range(0.0..=80.0)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(" mm"),
            )
            .changed()
        {
            draft.setup.set_safe_mm(safe_mm);
            draft.error = None;
        }
        for quick in [5.0_f32, 10.0] {
            if ui.small_button(format!("{quick:.0}")).clicked() {
                draft.setup.set_safe_mm(quick);
                draft.error = None;
            }
        }
    });
    if draft.setup.bleed_px > 0.0 {
        let (width, height) = draft.setup.canvas_size_px();
        ui.label(
            RichText::new(format!(
                "Bleed is included: the canvas becomes {width} × {height} px, with the \
                 trim line drawn {:.0} px in from the edge.",
                draft.setup.bleed_px
            ))
            .size(10.5)
            .color(theme::TEXT_MUTED),
        );
    }

    ui.add_space(6.0);
    ui.checkbox(
        &mut draft.setup.sketch_layer,
        RichText::new("Start with a sketch layer").size(11.0),
    );

    ui.add_space(10.0);
    ui.separator();
    ui.label(theme::section_title("Presets"));
    ui.add(
        egui::TextEdit::singleline(&mut draft.preset_name)
            .hint_text("Preset name")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        if ui
            .button(RichText::new("Save as preset").size(11.0))
            .on_hover_text("Store the current settings in My presets")
            .clicked()
        {
            actions.push(Action::Save);
        }
        if ui
            .add_enabled(
                draft.editing.is_some(),
                Button::new(RichText::new("Update").size(11.0)),
            )
            .on_disabled_hover_text("Select one of your own presets to change it")
            .clicked()
        {
            actions.push(Action::Update);
        }
        if ui
            .add_enabled(
                draft.editing.is_some(),
                Button::new(RichText::new("Delete").size(11.0)),
            )
            .clicked()
        {
            actions.push(Action::Delete);
        }
    });
    ui.horizontal(|ui| {
        let selected = draft.selected.clone();
        match selected {
            Some(id) => {
                let favorite = cards
                    .iter()
                    .position(|preset| preset.id == id)
                    .and_then(|index| favorites.get(index).copied())
                    .unwrap_or(false);
                if ui
                    .button(
                        RichText::new(if favorite {
                            "★ Favourited"
                        } else {
                            "☆ Favourite"
                        })
                        .size(11.0),
                    )
                    .clicked()
                {
                    actions.push(Action::Favorite(id));
                }
            }
            None => {
                ui.add_enabled(false, Button::new(RichText::new("☆ Favourite").size(11.0)));
            }
        }
        if ui
            .button(RichText::new("Restore defaults").size(11.0))
            .on_hover_text("Remove your own presets, favourites and recents")
            .clicked()
        {
            actions.push(Action::Restore);
        }
    });
    ui.horizontal(|ui| {
        if ui
            .button(RichText::new("Import…").size(11.0))
            .on_hover_text("Add presets from a JSON file")
            .clicked()
        {
            actions.push(Action::Import);
        }
        if ui
            .button(RichText::new("Export…").size(11.0))
            .on_hover_text("Write your presets to a JSON file")
            .clicked()
        {
            actions.push(Action::Export);
        }
    });

    if let Some(notice) = &draft.notice {
        ui.add_space(4.0);
        ui.label(RichText::new(notice).size(10.5).color(theme::TEXT_MUTED));
    }
}

fn footer(ui: &mut egui::Ui, draft: &mut NewCanvasState, create: &mut bool, cancel: &mut bool) {
    ui.add_space(6.0);

    if let Some(error) = &draft.error {
        egui::Frame::NONE
            .fill(theme::with_alpha(theme::ACCENT, 26))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.label(RichText::new(error).size(11.0).color(theme::ACCENT));
            });
    }

    ui.horizontal(|ui| {
        let (width, height) = draft.setup.canvas_size_px();
        let megapixels = width as f64 * height as f64 / 1_000_000.0;
        let memory = draft.setup.memory_mb();
        let heavy = memory > 512.0;
        ui.label(
            RichText::new(format!(
                "{width} × {height} px · {megapixels:.1} MP · ≈{memory:.0} MB (layer + view)"
            ))
            .size(10.5)
            .color(if heavy {
                theme::ACCENT
            } else {
                theme::TEXT_MUTED
            }),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(top_bar::accent_button("Create canvas")).clicked() {
                *create = true;
            }
            if ui.button("Cancel").clicked() {
                *cancel = true;
            }
        });
    });
}

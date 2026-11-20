//! Dark charcoal theme: one place for every colour and metric so the whole
//! workspace stays consistent.

use inkforge::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Vec2,
};

/// Backdrop behind the page.
pub const CANVAS_SURROUND: Color32 = Color32::from_rgb(0x12, 0x13, 0x16);
/// Top bar / status bar.
pub const BAR: Color32 = Color32::from_rgb(0x1B, 0x1D, 0x21);
/// Side panels.
pub const PANEL: Color32 = Color32::from_rgb(0x1F, 0x22, 0x26);
/// Inset fields, sliders troughs, swatch wells.
pub const INPUT: Color32 = Color32::from_rgb(0x15, 0x16, 0x19);
/// Hover fill.
pub const HOVER: Color32 = Color32::from_rgb(0x2A, 0x2E, 0x34);
/// Pressed fill.
pub const ACTIVE: Color32 = Color32::from_rgb(0x33, 0x38, 0x40);
/// Panel edges.
pub const LINE: Color32 = Color32::from_rgb(0x2E, 0x32, 0x38);
/// Very soft edges inside panels.
pub const LINE_SOFT: Color32 = Color32::from_rgb(0x26, 0x29, 0x2E);
/// Primary text.
pub const TEXT: Color32 = Color32::from_rgb(0xD8, 0xDB, 0xE1);
/// Secondary text / inactive icons.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8A, 0x90, 0x99);
/// Brand accent (amber ink).
pub const ACCENT: Color32 = Color32::from_rgb(0xE9, 0xA1, 0x3B);

/// `color` with a replaced alpha channel.
pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Install the theme on a context. Called once at startup.
///
/// egui 0.37 keeps a dark and a light style; the workspace only defines the
/// dark one and pins the app to it, so the look never depends on the OS theme.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);

    ctx.all_styles_mut(|style| {
        style.visuals = visuals();

        style.spacing.item_spacing = Vec2::new(8.0, 7.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.spacing.interact_size = Vec2::new(28.0, 22.0);
        style.spacing.slider_width = 150.0;
        style.spacing.slider_rail_height = 4.0;
        style.spacing.indent = 14.0;

        style.text_styles = [
            (TextStyle::Small, FontId::new(10.5, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        ]
        .into();
    });
}

fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();

    visuals.dark_mode = true;
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = INPUT;
    visuals.faint_bg_color = Color32::from_rgb(0x23, 0x26, 0x2B);
    visuals.override_text_color = Some(TEXT);
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(160),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(140),
    };
    visuals.selection.bg_fill = with_alpha(ACCENT, 70);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;

    let corner = CornerRadius::same(6);

    let mut inactive = visuals.widgets.inactive;
    inactive.bg_fill = INPUT;
    inactive.weak_bg_fill = Color32::from_rgb(0x25, 0x28, 0x2D);
    inactive.bg_stroke = Stroke::new(1.0, LINE_SOFT);
    inactive.fg_stroke = Stroke::new(1.0, TEXT);
    inactive.corner_radius = corner;
    inactive.expansion = 0.0;

    let mut hovered = inactive;
    hovered.weak_bg_fill = HOVER;
    hovered.bg_stroke = Stroke::new(1.0, with_alpha(ACCENT, 120));

    let mut active = inactive;
    active.weak_bg_fill = ACTIVE;
    active.bg_stroke = Stroke::new(1.0, ACCENT);
    active.fg_stroke = Stroke::new(1.0, ACCENT);

    let mut noninteractive = inactive;
    noninteractive.bg_fill = PANEL;
    noninteractive.weak_bg_fill = PANEL;
    noninteractive.bg_stroke = Stroke::new(1.0, LINE_SOFT);
    noninteractive.fg_stroke = Stroke::new(1.0, TEXT);

    let mut open = inactive;
    open.bg_fill = PANEL;

    visuals.widgets.noninteractive = noninteractive;
    visuals.widgets.inactive = inactive;
    visuals.widgets.hovered = hovered;
    visuals.widgets.active = active;
    visuals.widgets.open = open;

    visuals
}

/// Small uppercase heading used to title sidebar sections.
pub fn section_title(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase())
        .size(10.0)
        .color(TEXT_MUTED)
}

/// Muted explanatory line for features that are not wired up yet.
pub fn pending_note(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(11.0).color(TEXT_MUTED)
}

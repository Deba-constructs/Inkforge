//! Built-in brush presets.
//!
//! Each preset is a complete set of tip parameters, chosen so the presets are
//! visibly different from one another rather than just differently labelled:
//! the pen is thin and hard, the airbrush is huge and faint, the charcoal is
//! grainy with wide spacing, and so on.

use super::BrushSettings;

/// One named set of brush parameters.
#[derive(Clone, Copy, Debug)]
pub struct BrushPreset {
    pub name: &'static str,
    /// One-line description shown in a tooltip.
    pub hint: &'static str,
    pub size: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub spacing: f32,
    pub grain: f32,
    pub smoothing: f32,
}

/// `PRESETS[0]` must match [`BrushSettings::default`]; a test enforces it.
pub const PRESETS: [BrushPreset; 6] = [
    BrushPreset {
        name: "Round",
        hint: "Clean all-purpose round brush",
        size: 24.0,
        opacity: 1.0,
        hardness: 0.85,
        spacing: 0.15,
        grain: 0.0,
        smoothing: 0.35,
    },
    BrushPreset {
        name: "Ink Pen",
        hint: "Thin, hard edge for line art",
        size: 5.0,
        opacity: 1.0,
        hardness: 0.95,
        spacing: 0.07,
        grain: 0.0,
        smoothing: 0.55,
    },
    BrushPreset {
        name: "Pencil",
        hint: "Fine, grainy and slightly transparent",
        size: 6.0,
        opacity: 0.7,
        hardness: 0.8,
        spacing: 0.05,
        grain: 0.5,
        smoothing: 0.2,
    },
    BrushPreset {
        name: "Airbrush",
        hint: "Very soft and faint; builds up with repeated passes",
        size: 90.0,
        opacity: 0.15,
        hardness: 0.0,
        spacing: 0.05,
        grain: 0.0,
        smoothing: 0.6,
    },
    BrushPreset {
        name: "Marker",
        hint: "Flat, semi-transparent colour that stacks",
        size: 30.0,
        opacity: 0.55,
        hardness: 0.75,
        spacing: 0.1,
        grain: 0.06,
        smoothing: 0.45,
    },
    BrushPreset {
        name: "Charcoal",
        hint: "Grainy, broken edges for sketching",
        size: 22.0,
        opacity: 0.85,
        hardness: 0.45,
        spacing: 0.3,
        grain: 0.7,
        smoothing: 0.15,
    },
];

impl BrushSettings {
    /// Replace every parameter with the preset's values.
    pub fn apply_preset(&mut self, preset: &BrushPreset) {
        self.set_size(preset.size);
        self.set_opacity(preset.opacity);
        self.set_hardness(preset.hardness);
        self.set_spacing(preset.spacing);
        self.set_grain(preset.grain);
        self.set_smoothing(preset.smoothing);
    }

    /// Index of the preset these settings are exactly, if any.
    ///
    /// Derived rather than stored, so the UI can never claim a preset is active
    /// after the artist has moved a slider.
    pub fn matching_preset(&self) -> Option<usize> {
        PRESETS
            .iter()
            .position(|preset| self.matches(preset))
    }

    fn matches(&self, preset: &BrushPreset) -> bool {
        let close = |a: f32, b: f32| (a - b).abs() < 0.0005;
        close(self.size(), preset.size)
            && close(self.opacity(), preset.opacity)
            && close(self.hardness(), preset.hardness)
            && close(self.spacing(), preset.spacing)
            && close(self.grain(), preset.grain)
            && close(self.smoothing(), preset.smoothing)
    }
}

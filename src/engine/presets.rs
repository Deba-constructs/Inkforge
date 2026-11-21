//! Built-in brush presets.
//!
//! Each preset is a complete set of tip parameters, chosen so the presets are
//! visibly different from one another rather than just differently labelled:
//! the pen is thin and hard, the airbrush is huge and faint, the charcoal is
//! grainy with wide spacing, and so on.
//!
//! The library is grouped the way a paint application groups its brushes:
//! general-purpose first, then inking, sketching, painting, watercolour and
//! effects. The group only decides where a preset is listed (see
//! [`BrushGroup`]); every preset is a plain parameter set the rasteriser already
//! understands, so there are no brush types that only exist in the menu.

use super::BrushSettings;

/// Shelf a preset is listed under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushGroup {
    General,
    Inking,
    Sketching,
    Painting,
    Watercolour,
    Effects,
}

impl BrushGroup {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Inking,
        Self::Sketching,
        Self::Painting,
        Self::Watercolour,
        Self::Effects,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Inking => "Inking",
            Self::Sketching => "Sketching",
            Self::Painting => "Painting",
            Self::Watercolour => "Watercolour",
            Self::Effects => "Effects",
        }
    }
}

/// One named set of brush parameters.
#[derive(Clone, Copy, Debug)]
pub struct BrushPreset {
    pub name: &'static str,
    /// One-line description shown in a tooltip.
    pub hint: &'static str,
    pub group: BrushGroup,
    pub size: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub spacing: f32,
    pub grain: f32,
    pub smoothing: f32,
}

/// `PRESETS[0]` must match [`BrushSettings::default`]; a test enforces it.
pub const PRESETS: &[BrushPreset] = &[
    // ---- General ------------------------------------------------------
    BrushPreset {
        name: "Round",
        hint: "Clean all-purpose round brush",
        group: BrushGroup::General,
        size: 24.0,
        opacity: 1.0,
        hardness: 0.85,
        spacing: 0.15,
        grain: 0.0,
        smoothing: 0.35,
    },
    BrushPreset {
        name: "Soft Round",
        hint: "Soft-edged general brush that blends into what is underneath",
        group: BrushGroup::General,
        size: 48.0,
        opacity: 0.8,
        hardness: 0.25,
        spacing: 0.06,
        grain: 0.0,
        smoothing: 0.5,
    },
    BrushPreset {
        name: "Hard Round",
        hint: "Crisp edge for blocking shapes in",
        group: BrushGroup::General,
        size: 18.0,
        opacity: 1.0,
        hardness: 1.0,
        spacing: 0.08,
        grain: 0.0,
        smoothing: 0.3,
    },
    BrushPreset {
        name: "Eraser",
        hint: "Broad soft eraser; use the Eraser tool to erase with any preset",
        group: BrushGroup::General,
        size: 40.0,
        opacity: 1.0,
        hardness: 0.6,
        spacing: 0.1,
        grain: 0.0,
        smoothing: 0.4,
    },
    // ---- Inking -------------------------------------------------------
    BrushPreset {
        name: "Ink Pen",
        hint: "Thin, hard edge for line art",
        group: BrushGroup::Inking,
        size: 5.0,
        opacity: 1.0,
        hardness: 0.95,
        spacing: 0.07,
        grain: 0.0,
        smoothing: 0.55,
    },
    BrushPreset {
        name: "Technical Pen",
        hint: "Constant-width hairline, like a 0.3 mm pen",
        group: BrushGroup::Inking,
        size: 3.0,
        opacity: 1.0,
        hardness: 1.0,
        spacing: 0.03,
        grain: 0.0,
        smoothing: 0.5,
    },
    BrushPreset {
        name: "Fineliner",
        hint: "Slightly soft hairline for detail work",
        group: BrushGroup::Inking,
        size: 2.5,
        opacity: 0.95,
        hardness: 0.98,
        spacing: 0.04,
        grain: 0.03,
        smoothing: 0.6,
    },
    BrushPreset {
        name: "G-Pen",
        hint: "The classic comic line: springy, heavy, very smooth",
        group: BrushGroup::Inking,
        size: 9.0,
        opacity: 1.0,
        hardness: 0.92,
        spacing: 0.06,
        grain: 0.0,
        smoothing: 0.7,
    },
    BrushPreset {
        name: "Manga Line",
        hint: "Even-weight line for panel borders and hatching",
        group: BrushGroup::Inking,
        size: 6.0,
        opacity: 1.0,
        hardness: 0.9,
        spacing: 0.05,
        grain: 0.02,
        smoothing: 0.5,
    },
    BrushPreset {
        name: "Brush Pen",
        hint: "Thick, tapering stroke; pressure does most of the work",
        group: BrushGroup::Inking,
        size: 14.0,
        opacity: 0.9,
        hardness: 0.8,
        spacing: 0.08,
        grain: 0.08,
        smoothing: 0.55,
    },
    BrushPreset {
        name: "Speed Ink",
        hint: "Almost no smoothing, so fast strokes keep their snap",
        group: BrushGroup::Inking,
        size: 4.0,
        opacity: 1.0,
        hardness: 0.95,
        spacing: 0.05,
        grain: 0.0,
        smoothing: 0.1,
    },
    BrushPreset {
        name: "Dry Ink",
        hint: "Ink that skips across the paper",
        group: BrushGroup::Inking,
        size: 7.0,
        opacity: 0.9,
        hardness: 0.85,
        spacing: 0.12,
        grain: 0.35,
        smoothing: 0.3,
    },
    BrushPreset {
        name: "Calligraphy",
        hint: "Wide, flat ribbon with a broken edge",
        group: BrushGroup::Inking,
        size: 16.0,
        opacity: 1.0,
        hardness: 0.88,
        spacing: 0.09,
        grain: 0.12,
        smoothing: 0.45,
    },
    // ---- Sketching ----------------------------------------------------
    BrushPreset {
        name: "Pencil",
        hint: "Fine, grainy and slightly transparent",
        group: BrushGroup::Sketching,
        size: 6.0,
        opacity: 0.7,
        hardness: 0.8,
        spacing: 0.05,
        grain: 0.5,
        smoothing: 0.2,
    },
    BrushPreset {
        name: "2H Pencil",
        hint: "Faint hard pencil for construction lines",
        group: BrushGroup::Sketching,
        size: 4.0,
        opacity: 0.35,
        hardness: 0.9,
        spacing: 0.04,
        grain: 0.3,
        smoothing: 0.2,
    },
    BrushPreset {
        name: "6B Pencil",
        hint: "Dark, soft pencil for shading",
        group: BrushGroup::Sketching,
        size: 9.0,
        opacity: 0.9,
        hardness: 0.7,
        spacing: 0.06,
        grain: 0.6,
        smoothing: 0.15,
    },
    BrushPreset {
        name: "Blue Pencil",
        hint: "Light sketch pencil for underdrawing",
        group: BrushGroup::Sketching,
        size: 5.0,
        opacity: 0.5,
        hardness: 0.85,
        spacing: 0.05,
        grain: 0.25,
        smoothing: 0.25,
    },
    BrushPreset {
        name: "Charcoal",
        hint: "Grainy, broken edges for sketching",
        group: BrushGroup::Sketching,
        size: 22.0,
        opacity: 0.85,
        hardness: 0.45,
        spacing: 0.3,
        grain: 0.7,
        smoothing: 0.15,
    },
    BrushPreset {
        name: "Vine Charcoal",
        hint: "Wide, dusty and very grainy",
        group: BrushGroup::Sketching,
        size: 30.0,
        opacity: 0.6,
        hardness: 0.3,
        spacing: 0.28,
        grain: 0.85,
        smoothing: 0.12,
    },
    BrushPreset {
        name: "Chalk",
        hint: "Dry, chalky texture that leaves the paper showing",
        group: BrushGroup::Sketching,
        size: 18.0,
        opacity: 0.8,
        hardness: 0.5,
        spacing: 0.18,
        grain: 0.8,
        smoothing: 0.1,
    },
    BrushPreset {
        name: "Pastel",
        hint: "Soft, dusty colour sticks",
        group: BrushGroup::Sketching,
        size: 26.0,
        opacity: 0.75,
        hardness: 0.4,
        spacing: 0.14,
        grain: 0.65,
        smoothing: 0.2,
    },
    BrushPreset {
        name: "Crayon",
        hint: "Waxy, uneven line with visible grain",
        group: BrushGroup::Sketching,
        size: 12.0,
        opacity: 0.85,
        hardness: 0.6,
        spacing: 0.16,
        grain: 0.55,
        smoothing: 0.25,
    },
    BrushPreset {
        name: "Marker",
        hint: "Flat, semi-transparent colour that stacks",
        group: BrushGroup::Sketching,
        size: 30.0,
        opacity: 0.55,
        hardness: 0.75,
        spacing: 0.1,
        grain: 0.06,
        smoothing: 0.45,
    },
    // ---- Painting -----------------------------------------------------
    BrushPreset {
        name: "Flat Bristle",
        hint: "Square-ish bristle brush for blocking in colour",
        group: BrushGroup::Painting,
        size: 34.0,
        opacity: 0.95,
        hardness: 0.8,
        spacing: 0.07,
        grain: 0.2,
        smoothing: 0.35,
    },
    BrushPreset {
        name: "Filbert",
        hint: "Rounded bristle brush with a soft belly",
        group: BrushGroup::Painting,
        size: 40.0,
        opacity: 0.9,
        hardness: 0.65,
        spacing: 0.09,
        grain: 0.15,
        smoothing: 0.4,
    },
    BrushPreset {
        name: "Rough Bristle",
        hint: "Wide, streaky brush with visible bristle gaps",
        group: BrushGroup::Painting,
        size: 44.0,
        opacity: 0.85,
        hardness: 0.55,
        spacing: 0.2,
        grain: 0.5,
        smoothing: 0.3,
    },
    BrushPreset {
        name: "Dry Brush",
        hint: "Broken, scumbled strokes for texture",
        group: BrushGroup::Painting,
        size: 28.0,
        opacity: 0.8,
        hardness: 0.6,
        spacing: 0.35,
        grain: 0.75,
        smoothing: 0.2,
    },
    BrushPreset {
        name: "Impasto",
        hint: "Thick, opaque paint that stays where it is put",
        group: BrushGroup::Painting,
        size: 38.0,
        opacity: 1.0,
        hardness: 0.85,
        spacing: 0.12,
        grain: 0.3,
        smoothing: 0.3,
    },
    BrushPreset {
        name: "Palette Knife",
        hint: "Hard, wide smear of flat colour",
        group: BrushGroup::Painting,
        size: 52.0,
        opacity: 1.0,
        hardness: 0.95,
        spacing: 0.22,
        grain: 0.2,
        smoothing: 0.35,
    },
    BrushPreset {
        name: "Oil Bristle",
        hint: "Opaque, loaded brush with a little drag",
        group: BrushGroup::Painting,
        size: 30.0,
        opacity: 0.95,
        hardness: 0.7,
        spacing: 0.15,
        grain: 0.35,
        smoothing: 0.3,
    },
    BrushPreset {
        name: "Gouache",
        hint: "Matte, creamy and mostly opaque",
        group: BrushGroup::Painting,
        size: 24.0,
        opacity: 0.9,
        hardness: 0.8,
        spacing: 0.08,
        grain: 0.18,
        smoothing: 0.3,
    },
    BrushPreset {
        name: "Airbrush",
        hint: "Very soft and faint; builds up with repeated passes",
        group: BrushGroup::Painting,
        size: 90.0,
        opacity: 0.15,
        hardness: 0.0,
        spacing: 0.05,
        grain: 0.0,
        smoothing: 0.6,
    },
    BrushPreset {
        name: "Soft Airbrush",
        hint: "Wider, softer spray for gradients",
        group: BrushGroup::Painting,
        size: 140.0,
        opacity: 0.1,
        hardness: 0.05,
        spacing: 0.04,
        grain: 0.08,
        smoothing: 0.65,
    },
    // ---- Watercolour ---------------------------------------------------
    BrushPreset {
        name: "Watercolour",
        hint: "Translucent wash that layers into depth",
        group: BrushGroup::Watercolour,
        size: 70.0,
        opacity: 0.3,
        hardness: 0.2,
        spacing: 0.09,
        grain: 0.3,
        smoothing: 0.45,
    },
    BrushPreset {
        name: "Wet Wash",
        hint: "Very pale, very wide wash for skies and backgrounds",
        group: BrushGroup::Watercolour,
        size: 120.0,
        opacity: 0.12,
        hardness: 0.05,
        spacing: 0.04,
        grain: 0.05,
        smoothing: 0.55,
    },
    BrushPreset {
        name: "Wet Edge",
        hint: "Denser wash with the look of a drying edge",
        group: BrushGroup::Watercolour,
        size: 48.0,
        opacity: 0.45,
        hardness: 0.35,
        spacing: 0.1,
        grain: 0.4,
        smoothing: 0.4,
    },
    BrushPreset {
        name: "Salt Bloom",
        hint: "Loose, mottled wash with heavy texture",
        group: BrushGroup::Watercolour,
        size: 64.0,
        opacity: 0.25,
        hardness: 0.3,
        spacing: 0.25,
        grain: 0.9,
        smoothing: 0.35,
    },
    BrushPreset {
        name: "Ink Wash",
        hint: "Broad, thin grey wash for tonal studies",
        group: BrushGroup::Watercolour,
        size: 90.0,
        opacity: 0.2,
        hardness: 0.15,
        spacing: 0.06,
        grain: 0.12,
        smoothing: 0.5,
    },
    // ---- Effects -------------------------------------------------------
    BrushPreset {
        name: "Spray",
        hint: "Scattering spray can",
        group: BrushGroup::Effects,
        size: 100.0,
        opacity: 0.1,
        hardness: 0.1,
        spacing: 0.03,
        grain: 0.55,
        smoothing: 0.4,
    },
    BrushPreset {
        name: "Splatter",
        hint: "Thrown droplets with wide gaps between dabs",
        group: BrushGroup::Effects,
        size: 60.0,
        opacity: 0.9,
        hardness: 0.8,
        spacing: 0.85,
        grain: 0.9,
        smoothing: 0.05,
    },
    BrushPreset {
        name: "Stipple",
        hint: "Even dots, for texture and dithering",
        group: BrushGroup::Effects,
        size: 10.0,
        opacity: 0.8,
        hardness: 0.9,
        spacing: 0.75,
        grain: 0.7,
        smoothing: 0.05,
    },
    BrushPreset {
        name: "Noise Texture",
        hint: "Full-strength grain for adding tooth to flat colour",
        group: BrushGroup::Effects,
        size: 80.0,
        opacity: 0.5,
        hardness: 0.6,
        spacing: 0.2,
        grain: 1.0,
        smoothing: 0.3,
    },
    BrushPreset {
        name: "Smoke",
        hint: "A very faint haze that builds up slowly",
        group: BrushGroup::Effects,
        size: 160.0,
        opacity: 0.06,
        hardness: 0.1,
        spacing: 0.07,
        grain: 0.4,
        smoothing: 0.65,
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

/// Presets belonging to one group, in library order.
pub fn presets_in(group: BrushGroup) -> impl Iterator<Item = &'static BrushPreset> {
    PRESETS.iter().filter(move |preset| preset.group == group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_group_is_populated() {
        for group in BrushGroup::ALL {
            assert!(
                presets_in(group).next().is_some(),
                "brush group '{}' is empty",
                group.label()
            );
        }
    }

    #[test]
    fn preset_names_are_unique() {
        for (index, preset) in PRESETS.iter().enumerate() {
            for other in &PRESETS[index + 1..] {
                assert_ne!(preset.name, other.name, "duplicate preset name");
            }
        }
    }

    #[test]
    fn preset_parameters_are_unique() {
        let key = |preset: &BrushPreset| {
            (
                preset.size.to_bits(),
                preset.opacity.to_bits(),
                preset.hardness.to_bits(),
                preset.spacing.to_bits(),
                preset.grain.to_bits(),
                preset.smoothing.to_bits(),
            )
        };
        for (index, preset) in PRESETS.iter().enumerate() {
            for other in &PRESETS[index + 1..] {
                assert_ne!(
                    key(preset),
                    key(other),
                    "'{}' and '{}' have identical parameters",
                    preset.name,
                    other.name
                );
            }
        }
    }
}

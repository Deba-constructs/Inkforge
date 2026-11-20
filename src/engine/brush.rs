//! Brush parameters.
//!
//! These values are the single source of truth shared by the top bar, the
//! sidebar, the presets and the stroke rasteriser. Every setter sanitises its
//! input so a bad value can never reach the renderer.

use crate::egui::Color32;

use super::pen::PenTip;

/// Smallest brush diameter, in canvas pixels.
pub const MIN_SIZE: f32 = 1.0;
/// Largest brush diameter, in canvas pixels.
pub const MAX_SIZE: f32 = 500.0;
/// Smallest dab radius, in canvas pixels, once pen pressure has squeezed it.
/// A feather touch still leaves a hairline instead of a negative radius.
pub const MIN_DAB_RADIUS: f32 = 0.35;
/// Smallest dab spacing, as a fraction of the brush diameter.
pub const MIN_SPACING: f32 = 0.02;
/// Largest dab spacing, as a fraction of the brush diameter.
pub const MAX_SPACING: f32 = 1.0;

/// Brush configuration used for freehand strokes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushSettings {
    size: f32,
    opacity: f32,
    hardness: f32,
    spacing: f32,
    grain: f32,
    smoothing: f32,
}

impl Default for BrushSettings {
    /// The values of [`crate::engine::presets::PRESETS`]`[0]` ("Round").
    fn default() -> Self {
        Self {
            size: 24.0,
            opacity: 1.0,
            hardness: 0.85,
            spacing: 0.15,
            grain: 0.0,
            smoothing: 0.35,
        }
    }
}

/// A resolved brush tip, ready for the rasteriser.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushDab {
    /// Radius in canvas pixels (half the brush size).
    pub radius: f32,
    /// `0.0` = fully soft edge, `1.0` = hard edge.
    pub hardness: f32,
    /// Paint opacity in `0.0..=1.0`.
    pub opacity: f32,
    /// Distance between dabs as a fraction of the diameter. Small values give a
    /// smooth line, large values give a broken, textured one.
    pub spacing: f32,
    /// Per-pixel alpha noise in `0.0..=1.0`; 0 is a clean tip, 1 is very grainy.
    pub grain: f32,
    pub colour: Color32,
    /// Erase instead of paint.
    pub erase: bool,
}

impl BrushSettings {
    /// Resolve these settings into a dab for one stroke.
    pub fn dab(&self, colour: Color32, erase: bool) -> BrushDab {
        self.dab_scaled(colour, erase, PenTip::IDENTITY)
    }

    /// Resolve these settings into a dab with a pen tip applied.
    ///
    /// `tip` carries the size and opacity multipliers that pressure and tilt
    /// produce ([`crate::engine::PenSettings::tip`]). Radius and opacity are
    /// clamped here, so neither a zero-pressure reading nor a wild driver value
    /// can reach the rasteriser as a zero, negative or NaN tip.
    pub fn dab_scaled(&self, colour: Color32, erase: bool, tip: PenTip) -> BrushDab {
        BrushDab {
            radius: (self.size * 0.5 * tip.size).clamp(MIN_DAB_RADIUS, MAX_SIZE * 0.5),
            hardness: self.hardness,
            opacity: (self.opacity * tip.opacity).clamp(0.0, 1.0),
            spacing: self.spacing,
            grain: self.grain,
            colour,
            erase,
        }
    }

    pub fn size(&self) -> f32 {
        self.size
    }

    /// Set the diameter in canvas pixels, clamped to a sane range.
    pub fn set_size(&mut self, px: f32) {
        self.size = if px.is_finite() {
            px.clamp(MIN_SIZE, MAX_SIZE)
        } else {
            MIN_SIZE
        };
    }

    /// Paint opacity in `0.0..=1.0`.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn set_opacity(&mut self, value: f32) {
        self.opacity = unit(value).unwrap_or(self.opacity);
    }

    /// Edge softness in `0.0..=1.0` (1.0 = hard edge).
    pub fn hardness(&self) -> f32 {
        self.hardness
    }

    pub fn set_hardness(&mut self, value: f32) {
        self.hardness = unit(value).unwrap_or(self.hardness);
    }

    /// Dab spacing as a fraction of the diameter.
    pub fn spacing(&self) -> f32 {
        self.spacing
    }

    pub fn set_spacing(&mut self, value: f32) {
        if value.is_finite() {
            self.spacing = value.clamp(MIN_SPACING, MAX_SPACING);
        }
    }

    /// Per-pixel alpha noise in `0.0..=1.0`.
    pub fn grain(&self) -> f32 {
        self.grain
    }

    pub fn set_grain(&mut self, value: f32) {
        self.grain = unit(value).unwrap_or(self.grain);
    }

    /// Stroke stabilisation in `0.0..=1.0` (0.0 = raw pointer input).
    pub fn smoothing(&self) -> f32 {
        self.smoothing
    }

    pub fn set_smoothing(&mut self, value: f32) {
        self.smoothing = unit(value).unwrap_or(self.smoothing);
    }

    /// Grow/shrink the brush by a multiplier, used by the `[` and `]` keys.
    pub fn scale_size(&mut self, factor: f32) {
        self.set_size(self.size * factor);
    }

    pub fn size_label(&self) -> String {
        format!("{:.0} px", self.size)
    }

    pub fn opacity_percent(&self) -> f32 {
        self.opacity * 100.0
    }

    pub fn hardness_percent(&self) -> f32 {
        self.hardness * 100.0
    }

    pub fn spacing_percent(&self) -> f32 {
        self.spacing * 100.0
    }

    pub fn grain_percent(&self) -> f32 {
        self.grain * 100.0
    }

    pub fn smoothing_percent(&self) -> f32 {
        self.smoothing * 100.0
    }
}

/// Clamp to `0.0..=1.0`, rejecting NaN/infinity.
fn unit(value: f32) -> Option<f32> {
    if value.is_finite() {
        Some(value.clamp(0.0, 1.0))
    } else {
        None
    }
}

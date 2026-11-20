//! Drawing-tablet pressure and tilt.
//!
//! egui reports pointer positions only: a pen and a mouse arrive as the same
//! events, which is why a stroke laid down with a tablet used to be one uniform
//! width from start to finish. [`crate::input`] reads the two channels a tablet
//! adds — how hard the tip is pressed, and which way it leans — and this module
//! turns them into the two brush properties they drive: dab size and dab
//! opacity.
//!
//! The mapping is deliberately plain and is visible in the sidebar. Each amount
//! is a percentage: 0% leaves the brush exactly as configured, 100% takes the
//! dab from its smallest to its full value across the pressure range. Tilt
//! widens the tip, the way a leaned pen lays down a broader mark. A reading with
//! no pressure information ([`PenSample::MOUSE`]) is full pressure and no tilt,
//! so a mouse paints exactly the brush that is configured.
//!
//! Everything that reaches the rasteriser passes through [`PenSettings::tip`],
//! where the numbers are sanitised, so a bad value from a driver can never
//! produce a NaN or negative radius.

use crate::egui::Vec2;

/// Tilt magnitude, in degrees, that counts as fully tilted for the size boost.
pub const FULL_TILT: f32 = 60.0;

/// One reading from the tablet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenSample {
    pressure: f32,
    /// Tilt in degrees: `x` leans along the drawing surface's x axis, `y` along
    /// its y axis. Windows reports `-90..=90` for each.
    tilt: Vec2,
    from_pen: bool,
}

impl Default for PenSample {
    fn default() -> Self {
        Self::MOUSE
    }
}

impl PenSample {
    /// A mouse, or a platform with no tablet support: full pressure, no tilt.
    pub const MOUSE: Self = Self {
        pressure: 1.0,
        tilt: Vec2::ZERO,
        from_pen: false,
    };

    /// A reading from a pen, in `0.0..=1.0` pressure and degrees of tilt.
    pub fn pen(pressure: f32, tilt: Vec2) -> Self {
        Self {
            pressure: unit(pressure).unwrap_or(1.0),
            tilt: Vec2::new(degrees(tilt.x), degrees(tilt.y)),
            from_pen: true,
        }
    }

    /// `0.0` is a feather touch, `1.0` is fully pressed.
    pub fn pressure(&self) -> f32 {
        self.pressure
    }

    /// Tilt in degrees, per axis.
    pub fn tilt(&self) -> Vec2 {
        self.tilt
    }

    /// How far the pen leans from upright, `0.0..=~90.0` degrees.
    pub fn tilt_degrees(&self) -> f32 {
        self.tilt.length()
    }

    /// True when the reading came from a pen rather than a mouse (or a pen
    /// hovering out of contact, which reports full pressure).
    pub fn is_pen(&self) -> bool {
        self.from_pen
    }
}

/// How a tablet reading scales one dab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenTip {
    /// Multiplier for the dab radius.
    pub size: f32,
    /// Multiplier for the dab opacity.
    pub opacity: f32,
}

impl PenTip {
    /// The tip a mouse gets: the brush exactly as configured.
    pub const IDENTITY: Self = Self {
        size: 1.0,
        opacity: 1.0,
    };
}

/// What the artist's tablet controls, and how strongly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenSettings {
    enabled: bool,
    size_from_pressure: f32,
    opacity_from_pressure: f32,
    size_from_tilt: f32,
}

impl Default for PenSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            size_from_pressure: 0.85,
            opacity_from_pressure: 0.55,
            size_from_tilt: 0.30,
        }
    }
}

impl PenSettings {
    /// Whether pressure and tilt shape the brush at all.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Share of the diameter that pressure takes away at a feather touch.
    pub fn size_from_pressure(&self) -> f32 {
        self.size_from_pressure
    }

    pub fn set_size_from_pressure(&mut self, value: f32) {
        self.size_from_pressure = unit(value).unwrap_or(self.size_from_pressure);
    }

    /// Share of the opacity that pressure takes away at a feather touch.
    pub fn opacity_from_pressure(&self) -> f32 {
        self.opacity_from_pressure
    }

    pub fn set_opacity_from_pressure(&mut self, value: f32) {
        self.opacity_from_pressure = unit(value).unwrap_or(self.opacity_from_pressure);
    }

    /// Extra width at full tilt, as a share of the diameter.
    pub fn size_from_tilt(&self) -> f32 {
        self.size_from_tilt
    }

    pub fn set_size_from_tilt(&mut self, value: f32) {
        self.size_from_tilt = unit(value).unwrap_or(self.size_from_tilt);
    }

    pub fn size_from_pressure_percent(&self) -> f32 {
        self.size_from_pressure * 100.0
    }

    pub fn opacity_from_pressure_percent(&self) -> f32 {
        self.opacity_from_pressure * 100.0
    }

    pub fn size_from_tilt_percent(&self) -> f32 {
        self.size_from_tilt * 100.0
    }

    /// Scale one dab for a tablet reading.
    ///
    /// Disabled, or a mouse reading, returns [`PenTip::IDENTITY`] untouched.
    pub fn tip(&self, sample: PenSample) -> PenTip {
        if !self.enabled {
            return PenTip::IDENTITY;
        }

        let pressure = sample.pressure;
        let size = 1.0 - self.size_from_pressure * (1.0 - pressure);
        let opacity = 1.0 - self.opacity_from_pressure * (1.0 - pressure);

        // A leaned pen fans its tip out, so the mark widens without getting
        // any more opaque.
        let lean = (sample.tilt_degrees() / FULL_TILT).clamp(0.0, 1.0);
        let size = size * (1.0 + self.size_from_tilt * lean);

        PenTip {
            size: size.max(0.0),
            opacity: opacity.clamp(0.0, 1.0),
        }
    }
}

/// Clamp to `0.0..=1.0`, rejecting NaN and infinity.
fn unit(value: f32) -> Option<f32> {
    if value.is_finite() {
        Some(value.clamp(0.0, 1.0))
    } else {
        None
    }
}

/// Clamp one tilt axis to the physically possible range.
fn degrees(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-90.0, 90.0)
    } else {
        0.0
    }
}

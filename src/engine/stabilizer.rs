//! Stroke stabilisation.
//!
//! A raw pointer path is jittery, especially on cheap tablets and with a mouse.
//! The stabiliser turns it into a smooth path by easing a virtual "stamp point"
//! toward the pointer.
//!
//! Two refinements over a plain exponential filter:
//!
//! * **Speed adaptation** — fast movement is smoothed less, so quick strokes
//!   stay responsive instead of lagging behind the cursor.
//! * **Bounded lag** — the stamp point is never allowed to fall further than
//!   [`MAX_LAG`] behind the pointer, so a fast flick cannot leave a long,
//!   straightened trail.

use crate::egui::Pos2;

/// Maximum distance the stamp point may trail the pointer, in canvas pixels.
pub const MAX_LAG: f32 = 26.0;
/// Pointer speed (canvas pixels per update) at which smoothing fully eases off.
const FAST_SPEED: f32 = 45.0;
/// How much of the smoothing strength fast movement removes.
const SPEED_RELIEF: f32 = 0.7;
/// Overall smoothing ceiling within one update.
const MAX_STRENGTH: f32 = 0.92;

/// Eases the stamp point toward the pointer.
#[derive(Clone, Copy, Debug)]
pub struct Stabilizer {
    current: Pos2,
    last_raw: Pos2,
}

impl Stabilizer {
    /// Start a stroke at `at`.
    pub fn begin(at: Pos2) -> Self {
        Self {
            current: at,
            last_raw: at,
        }
    }

    /// Current stamp position.
    pub fn position(&self) -> Pos2 {
        self.current
    }

    /// Feed a pointer position and get the position to stamp.
    ///
    /// `smoothing` is `0.0..=1.0`; `0.0` returns the raw position exactly.
    pub fn push(&mut self, raw: Pos2, smoothing: f32) -> Pos2 {
        if !(raw.x.is_finite() && raw.y.is_finite()) {
            return self.current;
        }

        let smoothing = smoothing.clamp(0.0, 1.0);
        if smoothing <= 0.0 {
            self.current = raw;
            self.last_raw = raw;
            return self.current;
        }

        let speed = (raw - self.last_raw).length();
        let fast = (speed / FAST_SPEED).clamp(0.0, 1.0);
        let strength = smoothing * (1.0 - SPEED_RELIEF * fast);
        let factor = 1.0 - strength * MAX_STRENGTH;

        self.current += (raw - self.current) * factor;

        let lag = raw - self.current;
        if lag.length() > MAX_LAG {
            self.current = raw - lag.normalized() * MAX_LAG;
        }

        self.last_raw = raw;
        self.current
    }
}

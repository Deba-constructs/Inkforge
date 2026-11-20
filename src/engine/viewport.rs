//! Canvas viewport transform: zoom, pan and rotation.
//!
//! The page is drawn centred on the view rect plus a pan offset and turned by
//! the view rotation, which makes "fit to window", cursor-anchored zooming and
//! turning the sheet like a drawing board cheap and exact. All of the maths
//! lives here so the painting code and the canvas view can never disagree
//! about where a screen point lands on the page.

use inkforge::egui::{Pos2, Rect, Vec2};

/// Minimum zoom (2%).
pub const MIN_ZOOM: f32 = 0.02;
/// Maximum zoom (3200%).
pub const MAX_ZOOM: f32 = 32.0;
/// Multiplier applied by the zoom in/out buttons.
const ZOOM_STEP: f32 = 1.25;
/// Fraction of the view rect the page may occupy when fitted.
const FIT_MARGIN: f32 = 0.88;
/// Angles this close to a quarter turn snap to it (1.5°).
const ROTATION_SNAP: f32 = 0.0262;

/// Rotate a vector by `angle` radians (positive turns clockwise on screen,
/// whose y axis points down).
fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}

/// Wrap an angle into `(-π, π]`, so a view that has been turned all the way
/// round still reports a small angle and the readout never counts up.
fn normalise(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut wrapped = angle % tau;
    if wrapped <= -std::f32::consts::PI {
        wrapped += tau;
    } else if wrapped > std::f32::consts::PI {
        wrapped -= tau;
    }
    wrapped
}

/// Zoom / pan / rotation state for the canvas view.
#[derive(Clone, Debug)]
pub struct Viewport {
    zoom: f32,
    pan: Vec2,
    /// View rotation in radians. Canvas content and pointer mapping both use
    /// it, so a stroke still lands under the brush at any angle.
    rotation: f32,
    needs_fit: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            rotation: 0.0,
            needs_fit: true,
        }
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn zoom_percent(&self) -> f32 {
        self.zoom * 100.0
    }

    pub fn zoom_label(&self) -> String {
        format!("{:.0}%", self.zoom_percent())
    }

    pub fn pan(&self) -> Vec2 {
        self.pan
    }

    pub fn pan_by(&mut self, delta: Vec2) {
        if delta.x.is_finite() && delta.y.is_finite() {
            self.pan += delta;
        }
    }

    /// View rotation in radians, in `(-π, π]`.
    pub fn rotation(&self) -> f32 {
        self.rotation
    }

    /// True when the view is turned enough to matter.
    pub fn rotated(&self) -> bool {
        self.rotation.abs() > 1.0e-4
    }

    pub fn rotation_degrees(&self) -> f32 {
        self.rotation.to_degrees()
    }

    /// Angle for the readout, with a whole degree while it is close to one.
    pub fn rotation_label(&self) -> String {
        let degrees = self.rotation_degrees();
        if degrees.abs() < 0.05 {
            return String::from("0°");
        }
        format!("{degrees:.1}°")
    }

    /// Turn the view, snapping to the nearest quarter turn when it is close.
    ///
    /// Snapping is what makes "upright" reachable by hand: a drag never lands
    /// on exactly 0° or 90°, so without it the artist is left with a canvas
    /// that is a third of a degree crooked.
    pub fn set_rotation(&mut self, radians: f32) {
        if !radians.is_finite() {
            return;
        }
        let quarter = std::f32::consts::FRAC_PI_2;
        let nearest = (radians / quarter).round() * quarter;
        let snapped = if (radians - nearest).abs() <= ROTATION_SNAP {
            nearest
        } else {
            radians
        };
        self.rotation = normalise(snapped);
    }

    pub fn rotate_by(&mut self, radians: f32) {
        self.set_rotation(self.rotation + radians);
    }

    pub fn reset_rotation(&mut self) {
        self.rotation = 0.0;
    }

    /// True on the first frame (and after "Fit" is requested); the canvas view
    /// performs the fit because only it knows the available rect.
    pub fn needs_fit(&self) -> bool {
        self.needs_fit
    }

    pub fn request_fit(&mut self) {
        self.needs_fit = true;
    }

    /// Jump to 100% zoom, keeping the page where it is.
    pub fn request_actual_size(&mut self) {
        self.zoom = 1.0;
        self.needs_fit = false;
    }

    /// Scale the page down so it fits the view rect with a small margin.
    ///
    /// A turned page is wider and taller than its upright self, so the fit
    /// uses the size of the rotated bounding box; otherwise a 90° view of a
    /// landscape page would hang off the top and bottom of the window.
    pub fn fit(&mut self, view: Rect, doc: Vec2) {
        if doc.x <= 0.0 || doc.y <= 0.0 || view.width() <= 1.0 || view.height() <= 1.0 {
            return;
        }
        let (sin, cos) = self.rotation.sin_cos();
        let width = doc.x * cos.abs() + doc.y * sin.abs();
        let height = doc.x * sin.abs() + doc.y * cos.abs();
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let zoom =
            ((view.width() * FIT_MARGIN) / width).min((view.height() * FIT_MARGIN) / height);
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan = Vec2::ZERO;
        self.needs_fit = false;
    }

    /// The page rectangle in screen coordinates, unrotated.
    ///
    /// This is the bounding rectangle of the page as if it were upright; the
    /// drawing code turns its corners through [`Self::canvas_to_screen`].
    pub fn page_rect(&self, view: Rect, doc: Vec2) -> Rect {
        Rect::from_center_size(view.center() + self.pan, doc * self.zoom)
    }

    /// Zoom around `anchor` so the canvas point under it stays put.
    pub fn zoom_at(&mut self, factor: f32, anchor: Pos2, view: Rect) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let old = self.zoom;
        let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if (new - old).abs() <= f32::EPSILON {
            self.zoom = new;
            return;
        }
        let center = view.center();
        // Offset of the anchor from the page centre, in canvas pixels. Turning
        // the offset back by the view angle is what keeps the canvas point
        // under the cursor on a rotated view.
        let offset = rotate(anchor - (center + self.pan), -self.rotation) / old;
        self.pan = anchor - center - rotate(offset * new, self.rotation);
        self.zoom = new;
        self.needs_fit = false;
    }

    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom * ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM);
        self.needs_fit = false;
    }

    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom / ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM);
        self.needs_fit = false;
    }

    /// Screen point → canvas pixel coordinate (may fall outside the page).
    pub fn screen_to_canvas(&self, view: Rect, doc: Vec2, screen: Pos2) -> Pos2 {
        let offset = rotate(screen - (view.center() + self.pan), -self.rotation);
        let canvas = offset / self.zoom + doc * 0.5;
        Pos2::new(canvas.x, canvas.y)
    }

    /// Canvas pixel coordinate → screen point.
    pub fn canvas_to_screen(&self, view: Rect, doc: Vec2, canvas: Pos2) -> Pos2 {
        let offset = (canvas.to_vec2() - doc * 0.5) * self.zoom;
        (view.center() + self.pan) + rotate(offset, self.rotation)
    }
}

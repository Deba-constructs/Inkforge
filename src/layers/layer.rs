//! A single paint layer and the blending maths used to composite layers.

use crate::egui::Color32;
use crate::engine::Surface;

/// Stable identifier for a layer, so history and the UI can refer to it even
/// after reordering.
pub type LayerId = u64;

/// Basic blending modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
}

impl BlendMode {
    pub const ALL: [Self; 5] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Add,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Overlay => "Overlay",
            Self::Add => "Add",
        }
    }

    /// Blend one straight-alpha channel (`0.0..=1.0`).
    fn channel(self, dst: f32, src: f32) -> f32 {
        match self {
            Self::Normal => src,
            Self::Multiply => dst * src,
            Self::Screen => dst + src - dst * src,
            Self::Overlay => {
                if dst <= 0.5 {
                    2.0 * dst * src
                } else {
                    1.0 - 2.0 * (1.0 - dst) * (1.0 - src)
                }
            }
            Self::Add => (dst + src).min(1.0),
        }
    }
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

/// Everything about a layer except its pixels. Cheap to clone, which is what
/// makes property undo entries small.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerProps {
    pub name: String,
    pub visible: bool,
    /// `0.0..=1.0`.
    pub opacity: f32,
    pub blend: BlendMode,
    /// When set, the layer's alpha channel is frozen: paint can recolour the
    /// pixels that are already there, transparent pixels stay empty, and the
    /// eraser does nothing. This is Clip Studio Paint's "Lock transparent
    /// pixels".
    pub alpha_locked: bool,
}

/// One paint layer: its properties plus an RGBA surface.
#[derive(Clone, Debug)]
pub struct Layer {
    id: LayerId,
    props: LayerProps,
    /// Group this layer belongs to, if any.
    group: Option<LayerId>,
    surface: Surface,
}

impl Layer {
    pub fn new(id: LayerId, name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            id,
            props: LayerProps {
                name: name.into(),
                visible: true,
                opacity: 1.0,
                blend: BlendMode::Normal,
                alpha_locked: false,
            },
            group: None,
            surface: Surface::new(width, height),
        }
    }

    /// Group this layer belongs to, if any.
    pub fn group(&self) -> Option<LayerId> {
        self.group
    }

    /// Move the layer into a group, or back to the top level with `None`.
    pub fn set_group(&mut self, group: Option<LayerId>) {
        self.group = group;
    }

    pub fn id(&self) -> LayerId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.props.name
    }

    pub fn visible(&self) -> bool {
        self.props.visible
    }

    pub fn opacity(&self) -> f32 {
        self.props.opacity
    }

    pub fn blend(&self) -> BlendMode {
        self.props.blend
    }

    /// True when the layer's alpha channel is locked.
    pub fn alpha_locked(&self) -> bool {
        self.props.alpha_locked
    }

    pub fn props(&self) -> LayerProps {
        self.props.clone()
    }

    /// Replace the properties; opacity is clamped to `0.0..=1.0`.
    pub fn set_props(&mut self, mut props: LayerProps) {
        props.opacity = if props.opacity.is_finite() {
            props.opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if props.name.trim().is_empty() {
            props.name = self.props.name.clone();
        }
        self.props = props;
    }

    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    pub fn surface_mut(&mut self) -> &mut Surface {
        &mut self.surface
    }
}

/// Composite `src` (premultiplied) over `dst` (premultiplied) with a blend mode
/// and a layer opacity.
///
/// Uses the standard W3C compositing formula so blend modes behave correctly
/// where the backdrop is transparent or partially covered.
pub fn composite_pixel(dst: Color32, src: Color32, mode: BlendMode, opacity: f32) -> Color32 {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let src_alpha = src.a() as f32 / 255.0;
    let sa = src_alpha * opacity;
    if sa <= 0.0 {
        return dst;
    }

    // Fast path: Normal blending is plain premultiplied source-over.
    if mode == BlendMode::Normal {
        if sa >= 1.0 {
            return src;
        }
        let inverse = 1.0 - sa;
        return Color32::from_rgba_premultiplied(
            to_u8(src.r() as f32 + dst.r() as f32 * inverse),
            to_u8(src.g() as f32 + dst.g() as f32 * inverse),
            to_u8(src.b() as f32 + dst.b() as f32 * inverse),
            to_u8(sa * 255.0 + dst.a() as f32 * inverse),
        );
    }

    let dst_alpha = dst.a() as f32 / 255.0;
    if dst_alpha <= 0.0 {
        // Nothing behind the paint, so the blend function has nothing to act on.
        return Color32::from_rgba_premultiplied(
            to_u8(src.r() as f32 * opacity),
            to_u8(src.g() as f32 * opacity),
            to_u8(src.b() as f32 * opacity),
            to_u8(sa * 255.0),
        );
    }

    let straight = |channel: u8, alpha: f32| (channel as f32 / 255.0) / alpha;
    let src_channels = [
        straight(src.r(), src_alpha),
        straight(src.g(), src_alpha),
        straight(src.b(), src_alpha),
    ];
    let dst_channels = [
        straight(dst.r(), dst_alpha),
        straight(dst.g(), dst_alpha),
        straight(dst.b(), dst_alpha),
    ];

    let mut out = [0.0f32; 4];
    for channel in 0..3 {
        let blended = mode.channel(dst_channels[channel], src_channels[channel]);
        out[channel] = (1.0 - sa) * dst_alpha * dst_channels[channel]
            + sa * ((1.0 - dst_alpha) * src_channels[channel] + dst_alpha * blended);
    }
    out[3] = sa + dst_alpha * (1.0 - sa);

    // `out` is premultiplied in 0..1, matching Color32's storage.
    Color32::from_rgba_premultiplied(
        to_u8(out[0] * 255.0),
        to_u8(out[1] * 255.0),
        to_u8(out[2] * 255.0),
        to_u8(out[3] * 255.0),
    )
}

fn to_u8(value: f32) -> u8 {
    value.clamp(0.0, 255.0).round() as u8
}

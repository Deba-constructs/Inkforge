//! A single paint layer and the blending maths used to composite layers.

use crate::egui::Color32;
use crate::engine::Surface;

/// Stable identifier for a layer, so history and the UI can refer to it even
/// after reordering.
pub type LayerId = u64;

/// Blending modes, following the W3C *Compositing and Blending* specification —
/// the same set Photoshop, Krita and Clip Studio Paint expose, so a document
/// painted elsewhere looks the same here.
///
/// "Add" and "Linear Dodge" are one formula in those applications (and in the
/// specification), so they are one mode here rather than two names for one
/// operation. The label spells out both names an artist might look for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    LinearDodge,
    ColorDodge,
    ColorBurn,
    Darken,
    Lighten,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    /// Every mode, in the order the picker shows them: the regular set first,
    /// then the contrast pair, then the component modes.
    pub const ALL: [Self; 17] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::SoftLight,
        Self::HardLight,
        Self::LinearDodge,
        Self::ColorDodge,
        Self::ColorBurn,
        Self::Darken,
        Self::Lighten,
        Self::Difference,
        Self::Exclusion,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Overlay => "Overlay",
            Self::SoftLight => "Soft Light",
            Self::HardLight => "Hard Light",
            Self::LinearDodge => "Linear Dodge (Add)",
            Self::ColorDodge => "Color Dodge",
            Self::ColorBurn => "Color Burn",
            Self::Darken => "Darken",
            Self::Lighten => "Lighten",
            Self::Difference => "Difference",
            Self::Exclusion => "Exclusion",
            Self::Hue => "Hue",
            Self::Saturation => "Saturation",
            Self::Color => "Color",
            Self::Luminosity => "Luminosity",
        }
    }

    /// The stable name used in `.inkforge` manifests and in saved brush and
    /// layer presets.
    pub fn key(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::SoftLight => "soft_light",
            Self::HardLight => "hard_light",
            Self::LinearDodge => "linear_dodge",
            Self::ColorDodge => "color_dodge",
            Self::ColorBurn => "color_burn",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
            Self::Difference => "difference",
            Self::Exclusion => "exclusion",
            Self::Hue => "hue",
            Self::Saturation => "saturation",
            Self::Color => "color",
            Self::Luminosity => "luminosity",
        }
    }

    /// Read a mode back from its [`Self::key`], accepting the spellings other
    /// applications and older INKFORGE builds write ("add" for linear dodge,
    /// "colour" for color).
    pub fn from_key(key: &str) -> Self {
        let key = key.trim().to_ascii_lowercase().replace(' ', "_");
        match key.as_str() {
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "soft_light" | "softlight" => Self::SoftLight,
            "hard_light" | "hardlight" => Self::HardLight,
            "add" | "addition" | "linear_dodge" | "lineardodge" => Self::LinearDodge,
            "color_dodge" | "colour_dodge" => Self::ColorDodge,
            "color_burn" | "colour_burn" => Self::ColorBurn,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "difference" => Self::Difference,
            "exclusion" => Self::Exclusion,
            "hue" => Self::Hue,
            "saturation" => Self::Saturation,
            "color" | "colour" => Self::Color,
            "luminosity" => Self::Luminosity,
            _ => Self::Normal,
        }
    }

    /// Blend one pixel's straight-alpha RGB against the backdrop's.
    ///
    /// Separable modes work a channel at a time; Hue, Saturation, Color and
    /// Luminosity work on the colour as a whole, exactly as the W3C
    /// specification defines them.
    fn blend_rgb(self, backdrop: [f32; 3], source: [f32; 3]) -> [f32; 3] {
        match self {
            Self::Normal => source,
            Self::Multiply => map2(backdrop, source, |b, s| b * s),
            Self::Screen => map2(backdrop, source, |b, s| b + s - b * s),
            Self::Overlay => map2(backdrop, source, |b, s| {
                if b <= 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - s)
                }
            }),
            Self::SoftLight => map2(backdrop, source, |b, s| {
                if s <= 0.5 {
                    b - (1.0 - 2.0 * s) * b * (1.0 - b)
                } else {
                    let d = if b <= 0.25 {
                        ((16.0 * b - 12.0) * b + 4.0) * b
                    } else {
                        b.sqrt()
                    };
                    b + (2.0 * s - 1.0) * (d - b)
                }
            }),
            Self::HardLight => map2(backdrop, source, |b, s| {
                if s <= 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - s)
                }
            }),
            Self::LinearDodge => map2(backdrop, source, |b, s| (b + s).min(1.0)),
            Self::ColorDodge => map2(backdrop, source, |b, s| {
                if b <= 0.0 {
                    0.0
                } else if s >= 1.0 {
                    1.0
                } else {
                    (b / (1.0 - s)).min(1.0)
                }
            }),
            Self::ColorBurn => map2(backdrop, source, |b, s| {
                if b >= 1.0 {
                    1.0
                } else if s <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - b) / s).min(1.0)
                }
            }),
            Self::Darken => map2(backdrop, source, f32::min),
            Self::Lighten => map2(backdrop, source, f32::max),
            Self::Difference => map2(backdrop, source, |b, s| (b - s).abs()),
            Self::Exclusion => map2(backdrop, source, |b, s| b + s - 2.0 * b * s),
            Self::Hue => set_lum(set_saturation(source, saturation(backdrop)), luminance(backdrop)),
            Self::Saturation => set_lum(
                set_saturation(backdrop, saturation(source)),
                luminance(backdrop),
            ),
            Self::Color => set_lum(source, luminance(backdrop)),
            Self::Luminosity => set_lum(backdrop, luminance(source)),
        }
    }
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

fn map2(a: [f32; 3], b: [f32; 3], f: impl Fn(f32, f32) -> f32) -> [f32; 3] {
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

/// Rec. 601 luma weights, as the compositing specification defines them.
fn luminance(color: [f32; 3]) -> f32 {
    0.3 * color[0] + 0.59 * color[1] + 0.11 * color[2]
}

fn saturation(color: [f32; 3]) -> f32 {
    let max = color[0].max(color[1]).max(color[2]);
    let min = color[0].min(color[1]).min(color[2]);
    max - min
}

/// Push a colour into gamut without changing its perceived lightness.
fn clip_color(mut color: [f32; 3]) -> [f32; 3] {
    let l = luminance(color);
    let n = color[0].min(color[1]).min(color[2]);
    let x = color[0].max(color[1]).max(color[2]);
    if n < 0.0 {
        for channel in &mut color {
            *channel = l + (*channel - l) * l / (l - n);
        }
    }
    if x > 1.0 {
        for channel in &mut color {
            *channel = l + (*channel - l) * (1.0 - l) / (x - l);
        }
    }
    color
}

fn set_lum(color: [f32; 3], target: f32) -> [f32; 3] {
    let delta = target - luminance(color);
    clip_color([
        color[0] + delta,
        color[1] + delta,
        color[2] + delta,
    ])
}

fn set_saturation(color: [f32; 3], target: f32) -> [f32; 3] {
    let mut order = [0usize, 1, 2];
    order.sort_by(|a, b| color[*a].partial_cmp(&color[*b]).unwrap_or(std::cmp::Ordering::Equal));
    let (min, mid, max) = (order[0], order[1], order[2]);

    let mut out = [0.0f32; 3];
    if color[max] > color[min] {
        out[mid] = (color[mid] - color[min]) * target / (color[max] - color[min]);
        out[max] = target;
        out[min] = 0.0;
    }
    out
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
    /// Alpha inheritance: the layer only shows where the layer below it has
    /// paint, while keeping its own colours and blend mode. Clip Studio Paint
    /// calls this "Clip to layer below"; it is separate from the alpha lock and
    /// from group masks.
    pub clip_to_below: bool,
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
                clip_to_below: false,
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

    /// True when the layer inherits the alpha of the layer below it.
    pub fn clip_to_below(&self) -> bool {
        self.props.clip_to_below
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
    composite_pixel_masked(dst, src, mode, opacity, 1.0)
}

/// The same composite, with an extra coverage mask (`0.0..=1.0`) applied to the
/// source — this is how a clipped layer inherits the alpha of the layer below.
pub fn composite_pixel_masked(
    dst: Color32,
    src: Color32,
    mode: BlendMode,
    opacity: f32,
    mask: f32,
) -> Color32 {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let mask = if mask.is_finite() {
        mask.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let strength = opacity * mask;
    let src_alpha = src.a() as f32 / 255.0;
    let sa = src_alpha * strength;
    if sa <= 0.0 {
        return dst;
    }

    // Fast path: Normal blending is plain premultiplied source-over.
    if mode == BlendMode::Normal {
        let inverse = 1.0 - sa;
        return Color32::from_rgba_premultiplied(
            to_u8(src.r() as f32 * strength + dst.r() as f32 * inverse),
            to_u8(src.g() as f32 * strength + dst.g() as f32 * inverse),
            to_u8(src.b() as f32 * strength + dst.b() as f32 * inverse),
            to_u8(sa * 255.0 + dst.a() as f32 * inverse),
        );
    }

    let dst_alpha = dst.a() as f32 / 255.0;
    if dst_alpha <= 0.0 {
        // Nothing behind the paint, so the blend function has nothing to act on.
        return Color32::from_rgba_premultiplied(
            to_u8(src.r() as f32 * strength),
            to_u8(src.g() as f32 * strength),
            to_u8(src.b() as f32 * strength),
            to_u8(sa * 255.0),
        );
    }

    let straight = |channel: u8, alpha: f32| (channel as f32 / 255.0) / alpha;
    let source = [
        straight(src.r(), src_alpha),
        straight(src.g(), src_alpha),
        straight(src.b(), src_alpha),
    ];
    let backdrop = [
        straight(dst.r(), dst_alpha),
        straight(dst.g(), dst_alpha),
        straight(dst.b(), dst_alpha),
    ];

    let blended = mode.blend_rgb(backdrop, source);
    let out_alpha = sa + dst_alpha * (1.0 - sa);

    // Premultiplied, in 0..1, matching Color32's storage.
    let mut out = [0.0f32; 3];
    for channel in 0..3 {
        out[channel] = (1.0 - sa) * dst_alpha * backdrop[channel]
            + sa * ((1.0 - dst_alpha) * source[channel] + dst_alpha * blended[channel]);
        out[channel] = out[channel].clamp(0.0, out_alpha);
    }

    Color32::from_rgba_premultiplied(
        to_u8(out[0] * 255.0),
        to_u8(out[1] * 255.0),
        to_u8(out[2] * 255.0),
        to_u8(out_alpha * 255.0),
    )
}

fn to_u8(value: f32) -> u8 {
    value.clamp(0.0, 255.0).round() as u8
}

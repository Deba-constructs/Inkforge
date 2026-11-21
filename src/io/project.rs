//! The `.inkforge` project format.
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! "INKFORGE"          8 bytes, magic
//! version             u32, currently 1
//! manifest length     u32
//! manifest            UTF-8 JSON (see [`Manifest`])
//! layer images        one PNG per paint layer, in manifest order
//! ```
//!
//! Layer pixels are stored as straight-alpha PNGs instead of raw RGBA: an empty
//! or flat layer compresses to a few kilobytes, so a layered illustration stays
//! small on disk while remaining lossless. The manifest keeps layer ids, names,
//! visibility, opacity, blend mode, group membership and the active row, so a
//! reopened document is exactly the one that was saved.

use std::fs;
use std::path::Path;

use image::{ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};

use crate::egui::Color32;
use crate::engine::{Background, Canvas, PixelRegion, Surface};
use crate::layers::{Adjustment, BlendMode, Entry, Group, GroupProps, Layer, LayerProps};

use super::straight_rgba;

/// File magic. Also the first check when loading, so a renamed PNG is rejected
/// with a clear message instead of a confusing parse error.
pub const MAGIC: &[u8; 8] = b"INKFORGE";
/// Format version written by this build.
///
/// Version 2 adds an optional mask stream after each layer's pixels. Version 1
/// files (no masks) still load; a version 1 build refuses a version 2 file
/// instead of misreading the mask as the next layer.
///
/// Version 3 adds the print setup: resolution, pixel aspect ratio, bleed and
/// safe area. Those fields default, so every older project opens unchanged.
pub const FORMAT_VERSION: u32 = 3;

#[derive(Serialize, Deserialize)]
struct Manifest {
    name: String,
    width: u32,
    height: u32,
    background: BackgroundDto,
    active: usize,
    entries: Vec<EntryDto>,
    /// Dots per inch. Version 1 and 2 projects have no such field and open at
    /// the default 72 dpi.
    #[serde(default = "default_dpi")]
    dpi: f32,
    /// Pixel width divided by pixel height. Missing in older projects.
    #[serde(default = "unit_aspect")]
    pixel_aspect: f32,
    /// Bleed included on each side of the canvas, in pixels.
    #[serde(default)]
    bleed: f32,
    /// Safe-area margin inwards from the trim edge, in pixels.
    #[serde(default)]
    safe: f32,
}

fn default_dpi() -> f32 {
    crate::engine::DEFAULT_DPI
}

fn unit_aspect() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum EntryDto {
    Layer(LayerDto),
    Group(GroupDto),
}

#[derive(Serialize, Deserialize)]
struct LayerDto {
    id: u64,
    name: String,
    visible: bool,
    opacity: f32,
    blend: String,
    #[serde(default)]
    group: Option<u64>,
    /// "Lock transparent pixels" on this layer. Older projects have no such
    /// field and load with the lock off.
    #[serde(default)]
    alpha_locked: bool,
    /// "Clip to layer below" (alpha inheritance). Older projects have no such
    /// field and load unclipped.
    #[serde(default)]
    clip_to_below: bool,
    /// Live colour adjustments. Older projects have no such field and load
    /// neutral (no adjustment).
    #[serde(default)]
    adjust: AdjustmentDto,
    /// Length of this layer's PNG stream, in bytes.
    image: u32,
    /// Length of this layer's mask stream, in bytes; 0 means no mask. Older
    /// projects have no such field and load without masks.
    #[serde(default)]
    mask: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct AdjustmentDto {
    #[serde(default)]
    brightness: f32,
    #[serde(default)]
    contrast: f32,
    #[serde(default = "neutral_saturation")]
    saturation: f32,
    #[serde(default)]
    hue: f32,
}

fn neutral_saturation() -> f32 {
    1.0
}

impl Default for AdjustmentDto {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 1.0,
            hue: 0.0,
        }
    }
}

impl From<Adjustment> for AdjustmentDto {
    fn from(adjust: Adjustment) -> Self {
        let adjust = adjust.sanitised();
        Self {
            brightness: adjust.brightness,
            contrast: adjust.contrast,
            saturation: adjust.saturation,
            hue: adjust.hue,
        }
    }
}

impl From<AdjustmentDto> for Adjustment {
    fn from(dto: AdjustmentDto) -> Self {
        Adjustment {
            brightness: dto.brightness,
            contrast: dto.contrast,
            saturation: dto.saturation,
            hue: dto.hue,
        }
        .sanitised()
    }
}

#[derive(Serialize, Deserialize)]
struct GroupDto {
    id: u64,
    name: String,
    visible: bool,
    opacity: f32,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum BackgroundDto {
    White,
    Transparent,
    Custom { r: u8, g: u8, b: u8, a: u8 },
}

impl From<Background> for BackgroundDto {
    fn from(background: Background) -> Self {
        match background {
            Background::White => Self::White,
            Background::Transparent => Self::Transparent,
            Background::Custom(color) => Self::Custom {
                r: color.r(),
                g: color.g(),
                b: color.b(),
                a: color.a(),
            },
        }
    }
}

impl From<BackgroundDto> for Background {
    fn from(dto: BackgroundDto) -> Self {
        match dto {
            BackgroundDto::White => Background::White,
            BackgroundDto::Transparent => Background::Transparent,
            BackgroundDto::Custom { r, g, b, a } => {
                Background::Custom(Color32::from_rgba_unmultiplied(r, g, b, a))
            }
        }
    }
}

fn blend_name(blend: BlendMode) -> &'static str {
    blend.key()
}

fn parse_blend(name: &str) -> BlendMode {
    BlendMode::from_key(name)
}

/// Write `canvas` to `path` as an `.inkforge` project.
pub fn save_project(path: &Path, canvas: &Canvas) -> Result<(), String> {
    let mut entries = Vec::with_capacity(canvas.entry_count());
    let mut images: Vec<Vec<u8>> = Vec::new();

    for entry in canvas.entries() {
        match entry {
            Entry::Layer(layer) => {
                let png = encode_layer(layer.surface())?;
                let mask = match layer.mask() {
                    Some(mask) => encode_mask(mask)?,
                    None => Vec::new(),
                };
                entries.push(EntryDto::Layer(LayerDto {
                    id: layer.id(),
                    name: layer.name().to_owned(),
                    visible: layer.visible(),
                    opacity: layer.opacity(),
                    blend: blend_name(layer.blend()).to_owned(),
                    group: layer.group(),
                    alpha_locked: layer.alpha_locked(),
                    clip_to_below: layer.clip_to_below(),
                    adjust: layer.adjustment().into(),
                    image: png.len() as u32,
                    mask: mask.len() as u32,
                }));
                images.push(png);
                // A mask always follows its layer's pixels, so the reader can
                // walk the streams in manifest order without an index.
                if !mask.is_empty() {
                    images.push(mask);
                }
            }
            Entry::Group(group) => entries.push(EntryDto::Group(GroupDto {
                id: group.id(),
                name: group.name().to_owned(),
                visible: group.visible(),
                opacity: group.opacity(),
            })),
        }
    }

    let manifest = Manifest {
        name: canvas.name().to_owned(),
        width: canvas.width(),
        height: canvas.height(),
        background: canvas.background().into(),
        active: canvas.active_layer_index(),
        entries,
        dpi: canvas.dpi(),
        pixel_aspect: canvas.pixel_aspect(),
        bleed: canvas.bleed_px(),
        safe: canvas.safe_px(),
    };

    let json = serde_json::to_vec(&manifest)
        .map_err(|error| format!("Could not serialise the project manifest: {error}"))?;

    let mut bytes = Vec::with_capacity(16 + json.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&json);
    for png in &images {
        bytes.extend_from_slice(png);
    }

    write_atomically(path, &bytes)
}

/// Read an `.inkforge` project.
pub fn load_project(path: &Path) -> Result<Canvas, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "Could not read {}: {error}",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        )
    })?;

    if bytes.len() < 16 || &bytes[0..8] != MAGIC {
        return Err(String::from(
            "That file is not an INKFORGE project (the header is missing). \
             PNG and JPEG images can be opened with File ▸ Open image.",
        ));
    }

    let version = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if version == 0 || version > FORMAT_VERSION {
        return Err(format!(
            "This project uses format version {version}, but this build reads up to \
             version {FORMAT_VERSION}. A newer INKFORGE made it."
        ));
    }

    let json_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let images_start = 16 + json_len;
    if images_start > bytes.len() {
        return Err(String::from(
            "The project file is truncated: its manifest runs past the end of the file.",
        ));
    }

    let manifest: Manifest = serde_json::from_slice(&bytes[16..images_start])
        .map_err(|error| format!("The project's manifest is damaged: {error}"))?;

    let width = manifest.width;
    let height = manifest.height;
    let mut offset = images_start;
    let mut entries = Vec::with_capacity(manifest.entries.len());

    for entry in manifest.entries {
        match entry {
            EntryDto::Group(group) => {
                let mut created = Group::new(group.id, group.name.clone());
                created.set_props(GroupProps {
                    name: group.name,
                    visible: group.visible,
                    opacity: group.opacity,
                });
                entries.push(Entry::Group(created));
            }
            EntryDto::Layer(layer) => {
                let length = layer.image as usize;
                if offset + length > bytes.len() {
                    return Err(format!(
                        "The project file is truncated: \"{}\" is missing pixel data.",
                        layer.name
                    ));
                }
                let png = &bytes[offset..offset + length];
                offset += length;

                // The mask (if any) follows this layer's pixels.
                let mask_length = layer.mask as usize;
                let mask = if mask_length > 0 {
                    if offset + mask_length > bytes.len() {
                        return Err(format!(
                            "The project file is truncated: the mask of \"{}\" is missing.",
                            layer.name
                        ));
                    }
                    let stream = &bytes[offset..offset + mask_length];
                    offset += mask_length;
                    Some(decode_mask(stream, width, height, &layer.name)?)
                } else {
                    None
                };

                let decoded = image::load_from_memory(png)
                    .map_err(|error| {
                        format!("The pixels of \"{}\" are damaged: {error}", layer.name)
                    })?
                    .to_rgba8();
                if decoded.width() != width || decoded.height() != height {
                    return Err(format!(
                        "\"{}\" is {} × {} but the canvas is {width} × {height}.",
                        layer.name,
                        decoded.width(),
                        decoded.height()
                    ));
                }

                let mut created = Layer::new(layer.id, layer.name.clone(), width, height);
                let pixels: Vec<Color32> = decoded
                    .as_raw()
                    .chunks_exact(4)
                    .map(|pixel| {
                        Color32::from_rgba_unmultiplied(pixel[0], pixel[1], pixel[2], pixel[3])
                    })
                    .collect();
                created.surface_mut().write_region(
                    PixelRegion {
                        x: 0,
                        y: 0,
                        width,
                        height,
                    },
                    &pixels,
                );
                created.set_mask(mask);
                created.set_props(LayerProps {
                    name: layer.name,
                    visible: layer.visible,
                    opacity: layer.opacity,
                    blend: parse_blend(&layer.blend),
                    alpha_locked: layer.alpha_locked,
                    clip_to_below: layer.clip_to_below,
                    adjust: layer.adjust.into(),
                });
                created.set_group(layer.group);
                entries.push(Entry::Layer(created));
            }
        }
    }

    let mut canvas = Canvas::from_entries(
        manifest.name,
        width,
        height,
        manifest.background.into(),
        entries,
        manifest.active,
    )?;
    canvas.set_print_setup(
        manifest.dpi,
        manifest.pixel_aspect,
        manifest.bleed,
        manifest.safe,
    );
    Ok(canvas)
}

/// Encode one layer's pixels as a straight-alpha PNG stream.
fn encode_layer(surface: &Surface) -> Result<Vec<u8>, String> {
    let mut rgba = Vec::with_capacity(surface.pixels().len() * 4);
    for pixel in surface.pixels() {
        rgba.extend_from_slice(&straight_rgba(*pixel));
    }

    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            &rgba,
            surface.width(),
            surface.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| format!("Could not encode a layer as PNG: {error}"))?;
    Ok(out)
}

/// Encode a mask as a tiny grey+alpha PNG; only the alpha channel carries the
/// coverage, so two bytes per pixel are enough.
fn encode_mask(mask: &Surface) -> Result<Vec<u8>, String> {
    let mut grey = Vec::with_capacity(mask.pixels().len() * 2);
    for pixel in mask.pixels() {
        let coverage = pixel.a();
        grey.push(coverage);
        grey.push(coverage);
    }

    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            &grey,
            mask.width(),
            mask.height(),
            ExtendedColorType::La8,
        )
        .map_err(|error| format!("Could not encode a layer mask as PNG: {error}"))?;
    Ok(out)
}

/// Decode a mask stream back into white pixels whose alpha is the coverage.
fn decode_mask(
    stream: &[u8],
    width: u32,
    height: u32,
    layer_name: &str,
) -> Result<Surface, String> {
    let decoded = image::load_from_memory(stream)
        .map_err(|error| format!("The mask of \"{layer_name}\" is damaged: {error}"))?
        .to_rgba8();
    if decoded.width() != width || decoded.height() != height {
        return Err(format!(
            "The mask of \"{layer_name}\" is {} × {} but the canvas is {width} × {height}.",
            decoded.width(),
            decoded.height()
        ));
    }

    let mut mask = Surface::new(width, height);
    let pixels: Vec<Color32> = decoded
        .as_raw()
        .chunks_exact(4)
        .map(|pixel| {
            let coverage = pixel[3];
            Color32::from_rgba_premultiplied(coverage, coverage, coverage, coverage)
        })
        .collect();
    mask.write_region(
        PixelRegion {
            x: 0,
            y: 0,
            width,
            height,
        },
        &pixels,
    );
    Ok(mask)
}

/// Write through a sibling temp file so an interrupted save cannot leave the
/// artist with half a project.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("inkforge.part");
    fs::write(&temp, bytes).map_err(|error| {
        format!(
            "Could not write {}: {error}",
            temp.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| temp.display().to_string())
        )
    })?;

    // Windows refuses to rename over an existing file, so fall back to removing
    // the old copy first. The complete temp file is already on disk, so the
    // worst case is losing the previous save, never the new one.
    if fs::rename(&temp, path).is_err() {
        let _ = fs::remove_file(path);
        fs::rename(&temp, path).map_err(|error| {
            format!("Could not save the project: {error} (temporary copy left at {})", temp.display())
        })?;
    }
    Ok(())
}

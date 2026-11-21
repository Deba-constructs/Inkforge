//! OpenRaster (`.ora`) export.
//!
//! OpenRaster is the shared layered-image format used by Krita, GIMP, MyPaint,
//! Pinta and others. An `.ora` file is a ZIP archive holding:
//!
//! ```text
//! mimetype                   "image/openraster", stored first and uncompressed
//! stack.xml                  the layer tree (names, groups, opacity, blends)
//! mergedimage.png            the flattened artwork
//! Thumbnails/thumbnail.png   a small preview
//! data/layerN.png            one PNG per paint layer
//! ```
//!
//! The archive is written with stored (uncompressed) entries only, which the
//! format allows and which keeps this module free of a compressor dependency.
//! The PNG layer streams are already deflated, so the file is barely larger
//! than a compressed archive would be.
//!
//! Everything INKFORGE shows on screen is preserved: layer and group names,
//! visibility, opacity, blend modes, and — baked into each layer's exported
//! pixels, because OpenRaster has no portable representation for them — layer
//! masks, live colour adjustments and clipping. The `.inkforge` project keeps
//! those editable; the `.ora` is an interchange copy that looks right
//! everywhere.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::egui::Color32;
use crate::engine::Canvas;
use crate::layers::{BlendMode, Entry, Group, Layer};

use super::raster::flatten_rgba;
use super::straight_rgba;

/// The MIME type every OpenRaster file declares.
pub const ORA_MIMETYPE: &str = "image/openraster";
/// OpenRaster specification version this exporter targets.
pub const ORA_VERSION: &str = "0.0.6";
/// Longest edge of the generated thumbnail, in pixels.
const THUMBNAIL_MAX: u32 = 256;

/// Export the whole document as an OpenRaster archive.
pub fn export_ora(canvas: &mut Canvas, path: &Path) -> Result<(), String> {
    // Flatten first: it needs the canvas mutably, which ends before the layer
    // tree is borrowed below.
    let (merged_width, merged_height, merged) = flatten_rgba(canvas);
    let nodes = collect_nodes(canvas);
    let stack = stack_xml(canvas.width(), canvas.height(), &nodes);

    let mut zip = ZipWriter::new();
    // The specification requires `mimetype` to be the first entry, stored.
    zip.add("mimetype", ORA_MIMETYPE.as_bytes())?;
    zip.add("stack.xml", stack.as_bytes())?;
    zip.add(
        "mergedimage.png",
        &encode_png(merged_width, merged_height, &merged)?,
    )?;

    let (thumb_width, thumb_height, thumb) = thumbnail(&merged, merged_width, merged_height);
    zip.add(
        "Thumbnails/thumbnail.png",
        &encode_png(thumb_width, thumb_height, &thumb)?,
    )?;

    for node in &nodes {
        match node {
            OraNode::Layer { layer, src } => {
                let (width, height, rgba) = layer_pixels(layer);
                zip.add(src, &encode_png(width, height, &rgba)?)?;
            }
            OraNode::Group { children, .. } => {
                for (layer, src) in children {
                    let (width, height, rgba) = layer_pixels(layer);
                    zip.add(src, &encode_png(width, height, &rgba)?)?;
                }
            }
        }
    }

    let bytes = zip.finish();
    if let Err(error) = fs::write(path, &bytes) {
        // Never leave a half-written archive behind under the real name.
        let _ = fs::remove_file(path);
        return Err(format!("Could not write {}: {error}", path.display()));
    }
    Ok(())
}

/// The layer tree in the order OpenRaster describes it: the first child of a
/// stack is the topmost, so this is built bottom-first and emitted reversed.
enum OraNode<'a> {
    Layer {
        layer: &'a Layer,
        src: String,
    },
    Group {
        group: &'a Group,
        children: Vec<(&'a Layer, String)>,
    },
}

/// Walk the stack bottom-to-top, giving every layer a data file name.
fn collect_nodes(canvas: &Canvas) -> Vec<OraNode<'_>> {
    let mut nodes: Vec<OraNode<'_>> = Vec::with_capacity(canvas.entry_count());
    let mut next_image = 0usize;
    let mut name = || {
        let path = format!("data/layer{next_image}.png");
        next_image += 1;
        path
    };

    for entry in canvas.entries() {
        match entry {
            Entry::Layer(layer) => match layer.group() {
                // A layer whose group row was lost still gets exported rather
                // than dropping the artist's paint.
                Some(group) if matches!(nodes.last(), Some(OraNode::Group { group: open, .. }) if open.id() == group) => {
                    if let Some(OraNode::Group { children, .. }) = nodes.last_mut() {
                        children.push((layer, name()));
                    }
                }
                _ => nodes.push(OraNode::Layer { layer, src: name() }),
            },
            Entry::Group(group) => nodes.push(OraNode::Group {
                group,
                children: Vec::new(),
            }),
        }
    }

    nodes
}

/// Build `stack.xml` for a document.
fn stack_xml(width: u32, height: u32, nodes: &[OraNode<'_>]) -> String {
    let mut xml = String::with_capacity(512 + nodes.len() * 160);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<image version=\"{ORA_VERSION}\" w=\"{width}\" h=\"{height}\">\n"
    ));
    // The root stack carries no attributes: the specification fixes its
    // rendering and forbids name, opacity, visibility and composite-op there.
    xml.push_str("  <stack>\n");
    for node in nodes.iter().rev() {
        match node {
            OraNode::Layer { layer, src } => {
                xml.push_str(&format!(
                    "    <layer name=\"{}\" src=\"{src}\" opacity=\"{}\"{} composite-op=\"{}\" />\n",
                    escape(layer.name()),
                    opacity(layer.opacity()),
                    visibility(layer.visible()),
                    composite_op(layer.blend()),
                ));
            }
            OraNode::Group { group, children } => {
                xml.push_str(&format!(
                    "    <stack name=\"{}\" opacity=\"{}\"{} composite-op=\"svg:src-over\" isolation=\"isolate\">\n",
                    escape(group.name()),
                    opacity(group.opacity()),
                    visibility(group.visible()),
                ));
                for (layer, src) in children.iter().rev() {
                    xml.push_str(&format!(
                        "      <layer name=\"{}\" src=\"{src}\" opacity=\"{}\"{} composite-op=\"{}\" />\n",
                        escape(layer.name()),
                        opacity(layer.opacity()),
                        visibility(layer.visible()),
                        composite_op(layer.blend()),
                    ));
                }
                xml.push_str("    </stack>\n");
            }
        }
    }
    xml.push_str("  </stack>\n");
    xml.push_str("</image>\n");
    xml
}

fn opacity(value: f32) -> String {
    let value = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let mut text = format!("{value:.4}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    text
}

fn visibility(visible: bool) -> &'static str {
    if visible {
        ""
    } else {
        " visibility=\"hidden\""
    }
}

/// Translate a blend mode into the OpenRaster/SVG compositing name.
///
/// Every mode INKFORGE offers has an exact counterpart in the specification,
/// including Linear Dodge (`svg:plus`, the "lighter" operator) and the four
/// non-separable modes.
fn composite_op(blend: BlendMode) -> &'static str {
    match blend {
        BlendMode::Normal => "svg:src-over",
        BlendMode::Multiply => "svg:multiply",
        BlendMode::Screen => "svg:screen",
        BlendMode::Overlay => "svg:overlay",
        BlendMode::SoftLight => "svg:soft-light",
        BlendMode::HardLight => "svg:hard-light",
        BlendMode::LinearDodge => "svg:plus",
        BlendMode::ColorDodge => "svg:color-dodge",
        BlendMode::ColorBurn => "svg:color-burn",
        BlendMode::Darken => "svg:darken",
        BlendMode::Lighten => "svg:lighten",
        BlendMode::Difference => "svg:difference",
        BlendMode::Exclusion => "svg:exclusion",
        BlendMode::Hue => "svg:hue",
        BlendMode::Saturation => "svg:saturation",
        BlendMode::Color => "svg:color",
        BlendMode::Luminosity => "svg:luminosity",
    }
}

/// A layer's pixels as OpenRaster stores them: straight-alpha RGBA with the
/// mask, the live adjustment and clipping already baked in.
///
/// OpenRaster has no portable attribute for those three, so writing the
/// visible result is the only honest choice: the exported image looks the way
/// the canvas looks. The editable versions stay in the `.inkforge` project.
fn layer_pixels(layer: &Layer) -> (u32, u32, Vec<u8>) {
    let surface = layer.surface();
    let (width, height) = (surface.width(), surface.height());
    let adjust = layer.adjustment();
    let adjusted = !adjust.is_identity();
    let masked = layer.has_mask();

    let mut rgba = Vec::with_capacity(surface.pixels().len() * 4);
    for y in 0..height {
        for x in 0..width {
            let mut pixel = surface.pixel(x, y);
            if adjusted {
                pixel = adjust.apply(pixel);
            }
            if masked {
                let coverage = layer.coverage_at(x, y).clamp(0.0, 1.0);
                if coverage < 1.0 {
                    let scale = |channel: u8| (channel as f32 * coverage).round() as u8;
                    pixel = Color32::from_rgba_premultiplied(
                        scale(pixel.r()),
                        scale(pixel.g()),
                        scale(pixel.b()),
                        scale(pixel.a()),
                    );
                }
            }
            rgba.extend_from_slice(&straight_rgba(pixel));
        }
    }
    (width, height, rgba)
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    use image::{ExtendedColorType, ImageEncoder};

    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|error| format!("Could not encode a PNG for the archive: {error}"))?;
    Ok(out)
}

/// Nearest-neighbour thumbnail that fits inside [`THUMBNAIL_MAX`] without
/// changing the aspect ratio. A small document is used as it is.
fn thumbnail(rgba: &[u8], width: u32, height: u32) -> (u32, u32, Vec<u8>) {
    if width <= THUMBNAIL_MAX && height <= THUMBNAIL_MAX {
        return (width, height, rgba.to_vec());
    }

    let scale = (THUMBNAIL_MAX as f32 / width as f32).min(THUMBNAIL_MAX as f32 / height as f32);
    let out_width = ((width as f32 * scale).round() as u32).clamp(1, THUMBNAIL_MAX);
    let out_height = ((height as f32 * scale).round() as u32).clamp(1, THUMBNAIL_MAX);

    let mut out = Vec::with_capacity(out_width as usize * out_height as usize * 4);
    for y in 0..out_height {
        let source_y = (y as u64 * height as u64 / out_height as u64).min(height as u64 - 1) as u32;
        for x in 0..out_width {
            let source_x =
                (x as u64 * width as u64 / out_width as u64).min(width as u64 - 1) as u32;
            let index = (source_y as usize * width as usize + source_x as usize) * 4;
            out.extend_from_slice(&rgba[index..index + 4]);
        }
    }
    (out_width, out_height, out)
}

/// Escape text for use inside an XML attribute.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

// ---- ZIP writing ---------------------------------------------------------

/// A minimal ZIP writer for stored (uncompressed) entries.
///
/// OpenRaster permits stored entries, and the payloads here are already
/// compressed PNG streams, so writing our own keeps the dependency list short
/// and the output byte-for-byte predictable.
struct ZipWriter {
    data: Vec<u8>,
    central: Vec<u8>,
    count: usize,
    time: u16,
    date: u16,
}

impl ZipWriter {
    fn new() -> Self {
        let (time, date) = dos_timestamp();
        Self {
            data: Vec::new(),
            central: Vec::new(),
            count: 0,
            time,
            date,
        }
    }

    fn add(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let name_len = u16::try_from(name.len())
            .map_err(|_| String::from("An archive entry name is too long."))?;
        let size = u32::try_from(bytes.len()).map_err(|_| {
            String::from("One layer is too large for the OpenRaster archive (over 4 GB).")
        })?;
        let offset = u32::try_from(self.data.len())
            .map_err(|_| String::from("The OpenRaster archive would exceed 4 GB."))?;
        let crc = crc32(bytes);

        let mut local = Vec::with_capacity(30 + name.len());
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        local.extend_from_slice(&20u16.to_le_bytes()); // version needed
        local.extend_from_slice(&0u16.to_le_bytes()); // flags
        local.extend_from_slice(&0u16.to_le_bytes()); // stored
        local.extend_from_slice(&self.time.to_le_bytes());
        local.extend_from_slice(&self.date.to_le_bytes());
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes()); // compressed
        local.extend_from_slice(&size.to_le_bytes()); // uncompressed
        local.extend_from_slice(&name_len.to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes()); // extra field
        local.extend_from_slice(name.as_bytes());
        self.data.extend_from_slice(&local);
        self.data.extend_from_slice(bytes);

        self.central
            .extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        self.central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        self.central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        self.central.extend_from_slice(&0u16.to_le_bytes()); // flags
        self.central.extend_from_slice(&0u16.to_le_bytes()); // stored
        self.central.extend_from_slice(&self.time.to_le_bytes());
        self.central.extend_from_slice(&self.date.to_le_bytes());
        self.central.extend_from_slice(&crc.to_le_bytes());
        self.central.extend_from_slice(&size.to_le_bytes());
        self.central.extend_from_slice(&size.to_le_bytes());
        self.central.extend_from_slice(&name_len.to_le_bytes());
        self.central.extend_from_slice(&0u16.to_le_bytes()); // extra
        self.central.extend_from_slice(&0u16.to_le_bytes()); // comment
        self.central.extend_from_slice(&0u16.to_le_bytes()); // disk
        self.central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        self.central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        self.central.extend_from_slice(&offset.to_le_bytes());
        self.central.extend_from_slice(name.as_bytes());

        self.count += 1;
        Ok(())
    }

    fn finish(mut self) -> Vec<u8> {
        let central_offset = self.data.len() as u32;
        let central_size = self.central.len() as u32;
        self.data.extend_from_slice(&self.central);

        let count = self.count.min(u16::MAX as usize) as u16;
        self.data.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.data.extend_from_slice(&0u16.to_le_bytes()); // this disk
        self.data.extend_from_slice(&0u16.to_le_bytes()); // disk with the directory
        self.data.extend_from_slice(&count.to_le_bytes());
        self.data.extend_from_slice(&count.to_le_bytes());
        self.data.extend_from_slice(&central_size.to_le_bytes());
        self.data.extend_from_slice(&central_offset.to_le_bytes());
        self.data.extend_from_slice(&0u16.to_le_bytes()); // archive comment
        self.data
    }
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
};

/// Standard CRC-32, as every ZIP entry header carries.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc = CRC_TABLE[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// Current local time as a DOS date and time pair, which is what ZIP headers
/// use. Falls back to 1980-01-01 when the clock is before the epoch.
fn dos_timestamp() -> (u16, u16) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);

    let days = (seconds / 86_400) as i64;
    let clock = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);

    let year = (year - 1980).clamp(0, 127) as u16;
    let date = (year << 9) | (month as u16) << 5 | day as u16;
    let time = ((clock / 3600) as u16) << 11
        | (((clock % 3600) / 60) as u16) << 5
        | ((clock % 60) / 2) as u16;
    (time, date)
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

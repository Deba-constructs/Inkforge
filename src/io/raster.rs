//! PNG and JPEG import/export.
//!
//! Export always flattens what the artist sees: every visible layer over the
//! document background. Import either becomes a new document (sized to the
//! image) or a new layer inside the current one.

use std::fs::File;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};

use crate::egui::Color32;
use crate::engine::{Background, Canvas, MIN_CANVAS_DIM, PixelRegion};
use crate::layers::{composite_pixel, BlendMode};

use super::png::{self, PngMeta};
use super::straight_rgba;

/// Image formats INKFORGE can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG image",
            Self::Jpeg => "JPEG image",
        }
    }

    /// Second extension accepted for the same format, if any.
    pub fn extra_extension(self) -> Option<&'static str> {
        match self {
            Self::Png => None,
            Self::Jpeg => Some("jpeg"),
        }
    }
}

/// Flatten the visible artwork over the document background and write it out.
///
/// PNG keeps transparency; JPEG has no alpha channel, so transparent areas are
/// composited onto white instead of turning black.
pub fn export_image(
    canvas: &mut Canvas,
    path: &Path,
    format: ImageFormat,
) -> Result<(), String> {
    let (width, height, rgba) = flatten_rgba(canvas);

    match format {
        ImageFormat::Png => {
            // The PNG carries the document's resolution and colour space, so
            // a print shop or another art application opens it at the size the
            // artist designed.
            let meta = PngMeta {
                dpi_x: canvas.dpi() * canvas.pixel_aspect(),
                dpi_y: canvas.dpi(),
                srgb: true,
            };
            let bytes = png::encode_rgba(&rgba, width, height, Some(meta))?;
            std::fs::write(path, bytes).map_err(|error| {
                format!("Could not write {}: {error}", path.display())
            })
        }
        ImageFormat::Jpeg => {
            // Drop the alpha channel, compositing see-through pixels onto white.
            let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
            for pixel in rgba.chunks_exact(4) {
                let alpha = pixel[3] as u32;
                let over_white = |value: u8| {
                    ((value as u32 * alpha + 255 * (255 - alpha)) / 255).min(255) as u8
                };
                rgb.extend_from_slice(&[
                    over_white(pixel[0]),
                    over_white(pixel[1]),
                    over_white(pixel[2]),
                ]);
            }

            let file = File::create(path).map_err(|error| {
                format!("Could not create {}: {error}", path.display())
            })?;
            JpegEncoder::new_with_quality(file, 92)
                .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
                .map_err(|error| format!("Could not encode the JPEG: {error}"))
        }
    }
}

/// Flatten every visible layer over the document background and return the
/// result as straight-alpha RGBA bytes, row major.
///
/// Shared by PNG/JPEG export and by the OpenRaster writer, so both always
/// produce exactly the same visible picture.
pub(crate) fn flatten_rgba(canvas: &mut Canvas) -> (u32, u32, Vec<u8>) {
    let (width, height) = (canvas.width(), canvas.height());
    let background = canvas.background().fill();
    let composite = canvas.composite();

    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let painted = composite.pixel(x, y);
            let visible = match background {
                Some(backdrop) => composite_pixel(backdrop, painted, BlendMode::Normal, 1.0),
                None => painted,
            };
            rgba.extend_from_slice(&straight_rgba(visible));
        }
    }
    (width, height, rgba)
}

/// Decode any PNG or JPEG into premultiplied pixels, row major.
pub fn read_image(path: &Path) -> Result<(u32, u32, Vec<Color32>), String> {
    let bytes = fs_read(path)?;
    let decoded = image::load_from_memory(&bytes).map_err(|error| {
        format!(
            "{} is not a PNG or JPEG image this build can read: {error}",
            display_name(path)
        )
    })?;

    let rgba = decoded.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    if width == 0 || height == 0 {
        return Err(format!("{} is empty (0 pixels).", display_name(path)));
    }

    let pixels = rgba
        .as_raw()
        .chunks_exact(4)
        .map(|pixel| Color32::from_rgba_unmultiplied(pixel[0], pixel[1], pixel[2], pixel[3]))
        .collect();
    Ok((width, height, pixels))
}

/// Open an image as its own document, named after the file.
pub fn open_image(path: &Path) -> Result<Canvas, String> {
    let (width, height, pixels) = read_image(path)?;

    // Very small images are padded to the minimum canvas size so there is room
    // to paint; the artwork itself keeps its exact pixels at the top-left.
    let canvas_width = width.max(MIN_CANVAS_DIM);
    let canvas_height = height.max(MIN_CANVAS_DIM);
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("Image"));

    let mut canvas = Canvas::try_new(name, canvas_width, canvas_height, Background::Transparent)?;
    let layer = canvas
        .active_layer_id()
        .ok_or_else(|| String::from("The new document has no layer to place the image in."))?;

    let region = PixelRegion {
        x: 0,
        y: 0,
        width: width.min(canvas_width),
        height: height.min(canvas_height),
    };
    let mut buffer = Vec::with_capacity(region.pixel_count());
    for row in 0..region.height {
        let start = row as usize * width as usize;
        buffer.extend_from_slice(&pixels[start..start + region.width as usize]);
    }

    if let Some(surface) = canvas.layer_surface_mut(layer) {
        surface.write_region(region, &buffer);
    }
    canvas.invalidate_composite(None);
    canvas.mark_saved();

    Ok(canvas)
}

fn fs_read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path)
        .map_err(|error| format!("Could not read {}: {error}", display_name(path)))
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

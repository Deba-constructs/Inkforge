//! PNG stream helpers: encoding plus the metadata chunks that make a PNG a
//! print-ready file.
//!
//! The `image` crate writes a correct but bare PNG. Two small ancillary chunks
//! are added here, before the image data:
//!
//! * `pHYs` — the physical pixel density, which is how other applications learn
//!   the document's DPI. Width and height densities differ when the document
//!   has a non-square pixel aspect ratio.
//! * `sRGB` — declares the colour space, which is the only one INKFORGE
//!   composites in. Without it a viewer may guess and shift the colours.
//!
//! Both chunks are covered by a CRC-32 like every other PNG chunk, so the same
//! CRC routine is shared with the ZIP writer in [`super::ora`].

use image::{ExtendedColorType, ImageEncoder};

/// Physical resolution and colour-space declaration for one PNG stream.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PngMeta {
    /// Pixels per inch along the x axis (already includes the pixel aspect).
    pub dpi_x: f32,
    /// Pixels per inch along the y axis.
    pub dpi_y: f32,
    /// Write the `sRGB` chunk.
    pub srgb: bool,
}

/// Encode straight-alpha RGBA bytes as a PNG, optionally with print metadata.
pub(crate) fn encode_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    meta: Option<PngMeta>,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|error| format!("Could not encode the PNG: {error}"))?;

    if let Some(meta) = meta {
        // pHYs first, then sRGB, so the file reads sRGB then pHYs: the order
        // image viewers and print software both accept without complaint.
        if meta.dpi_x.is_finite()
            && meta.dpi_y.is_finite()
            && meta.dpi_x >= 1.0
            && meta.dpi_y >= 1.0
        {
            let mut data = Vec::with_capacity(9);
            data.extend_from_slice(&pixels_per_metre(meta.dpi_x).to_be_bytes());
            data.extend_from_slice(&pixels_per_metre(meta.dpi_y).to_be_bytes());
            data.push(1); // unit specifier: metre
            insert_after_ihdr(&mut out, b"pHYs", &data)?;
        }
        if meta.srgb {
            // Rendering intent 0: perceptual, which is what sRGB artwork means.
            insert_after_ihdr(&mut out, b"sRGB", &[0])?;
        }
    }

    Ok(out)
}

/// 300 dpi is 11811 pixels per metre: 300 / 0.0254 = 11811.02…
fn pixels_per_metre(dpi: f32) -> u32 {
    let value = (dpi as f64 / 0.0254).round();
    value.clamp(1.0, u32::MAX as f64) as u32
}

/// Offset just past the PNG signature and its `IHDR` chunk.
const IHDR_END: usize = 8 + 4 + 4 + 13 + 4;

/// Insert an ancillary chunk directly after `IHDR`, which every PNG writer
/// places first.
fn insert_after_ihdr(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Result<(), String> {
    // Refuse anything that is not a PNG instead of corrupting it.
    if png.len() < IHDR_END || &png[0..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return Err(String::from(
            "The encoder did not produce a PNG stream that can carry metadata.",
        ));
    }
    let length = u32::try_from(data.len())
        .map_err(|_| String::from("A PNG metadata chunk is too large."))?;

    let mut chunk = Vec::with_capacity(12 + data.len());
    chunk.extend_from_slice(&length.to_be_bytes());
    chunk.extend_from_slice(kind);
    chunk.extend_from_slice(data);

    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    chunk.extend_from_slice(&crc32(&crc_input).to_be_bytes());

    png.splice(IHDR_END..IHDR_END, chunk);
    Ok(())
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

/// Standard CRC-32, as PNG chunks and ZIP entries both use.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc = CRC_TABLE[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walk a PNG's chunk list: (type, length, crc valid).
    fn chunks(png: &[u8]) -> Vec<([u8; 4], usize, bool)> {
        let mut out = Vec::new();
        let mut offset = 8;
        while offset + 8 <= png.len() {
            let length = u32::from_be_bytes([
                png[offset],
                png[offset + 1],
                png[offset + 2],
                png[offset + 3],
            ]) as usize;
            let kind = [
                png[offset + 4],
                png[offset + 5],
                png[offset + 6],
                png[offset + 7],
            ];
            let data_start = offset + 8;
            let crc_start = data_start + length;
            if crc_start + 4 > png.len() {
                break;
            }
            let stored = u32::from_be_bytes([
                png[crc_start],
                png[crc_start + 1],
                png[crc_start + 2],
                png[crc_start + 3],
            ]);
            out.push((kind, length, stored == crc32(&png[offset + 4..crc_start])));
            offset = crc_start + 4;
        }
        out
    }

    fn rgba(width: u32, height: u32) -> Vec<u8> {
        vec![128u8; width as usize * height as usize * 4]
    }

    #[test]
    fn physical_resolution_and_colour_space_are_written() {
        let meta = PngMeta {
            dpi_x: 300.0,
            dpi_y: 300.0,
            srgb: true,
        };
        let png = encode_rgba(&rgba(2, 2), 2, 2, Some(meta)).expect("png");
        let list = chunks(&png);
        let kinds: Vec<[u8; 4]> = list.iter().map(|(kind, _, _)| *kind).collect();

        let srgb = kinds.iter().position(|kind| kind == b"sRGB").expect("sRGB");
        let phys = kinds.iter().position(|kind| kind == b"pHYs").expect("pHYs");
        assert_eq!(kinds[0], *b"IHDR", "IHDR must stay first");
        assert!(srgb < phys, "chunks keep their documented order");
        assert!(
            kinds.iter().position(|kind| kind == b"IDAT").unwrap() > phys,
            "metadata must precede the image data"
        );
        assert!(
            list.iter().all(|(_, _, valid)| *valid),
            "chunk CRCs are correct"
        );

        // 300 dpi / 0.0254 = 11811.02 -> 11811 pixels per metre.
        let phys_chunk = &png[8..];
        let offset = phys_chunk
            .windows(4)
            .position(|window| window == b"pHYs")
            .expect("pHYs bytes")
            + 4;
        let ppu = u32::from_be_bytes([
            phys_chunk[offset],
            phys_chunk[offset + 1],
            phys_chunk[offset + 2],
            phys_chunk[offset + 3],
        ]);
        assert_eq!(ppu, 11811);
    }

    #[test]
    fn a_non_square_pixel_aspect_writes_two_densities() {
        let meta = PngMeta {
            dpi_x: 600.0,
            dpi_y: 300.0,
            srgb: true,
        };
        let png = encode_rgba(&rgba(1, 1), 1, 1, Some(meta)).expect("png");
        let offset = png
            .windows(4)
            .position(|window| window == b"pHYs")
            .expect("pHYs bytes")
            + 4;
        let x = u32::from_be_bytes([
            png[offset],
            png[offset + 1],
            png[offset + 2],
            png[offset + 3],
        ]);
        let y = u32::from_be_bytes([
            png[offset + 4],
            png[offset + 5],
            png[offset + 6],
            png[offset + 7],
        ]);
        assert_eq!(x, 23622);
        assert_eq!(y, 11811);
    }

    #[test]
    fn without_metadata_the_stream_is_a_plain_png() {
        let png = encode_rgba(&rgba(1, 1), 1, 1, None).expect("png");
        let kinds: Vec<[u8; 4]> = chunks(&png).into_iter().map(|(kind, _, _)| kind).collect();
        assert!(!kinds.contains(&*b"pHYs"));
        assert!(!kinds.contains(&*b"sRGB"));
    }

    #[test]
    fn crc32_matches_the_published_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}

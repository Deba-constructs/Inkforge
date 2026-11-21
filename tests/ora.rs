//! Tests for OpenRaster (`.ora`) export.
//!
//! The archive is read back here with a deliberately small reader that only
//! understands stored entries, which is exactly what the exporter writes. That
//! proves the bytes are a valid ZIP (signatures, sizes, CRC-32, central
//! directory) instead of trusting the writer.

use std::fs;
use std::path::PathBuf;

use inkforge::egui::{pos2, Color32};
use inkforge::engine::{Background, BrushSettings, Canvas};
use inkforge::io;
use inkforge::layers::{BlendMode, LayerProps};

fn brush(size: f32) -> BrushSettings {
    let mut brush = BrushSettings::default();
    brush.set_size(size);
    brush.set_hardness(1.0);
    brush.set_opacity(1.0);
    brush
}

fn canvas(width: u32, height: u32, background: Background) -> Canvas {
    Canvas::try_new("Test", width, height, background).expect("valid canvas")
}

fn dab_at(canvas: &mut Canvas, x: f32, y: f32, colour: Color32) {
    let id = canvas.active_layer_id().expect("an active paint layer");
    let dab = brush(9.0).dab(colour, false);
    canvas.paint_layer(id, pos2(x, y), pos2(x, y), &dab);
}

fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "inkforge-ora-{}-{name}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

// ---- a stored-only ZIP reader -------------------------------------------

struct Entry {
    name: String,
    bytes: Vec<u8>,
    /// Offset of the entry's local header, so the central directory's pointer
    /// can be checked against the data actually written.
    offset: usize,
}

fn read_zip(bytes: &[u8]) -> Vec<Entry> {
    // End of central directory: last 22 bytes (no archive comment is written).
    let end = bytes.len() - 22;
    assert_eq!(
        &bytes[end..end + 4],
        b"PK\x05\x06",
        "missing end of directory"
    );
    let count = u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]);
    let central_size = u32::from_le_bytes(bytes[end + 12..end + 16].try_into().unwrap());
    let central_offset = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap());
    assert_eq!(
        central_size as usize,
        end - central_offset as usize,
        "the central directory size must match the bytes before the end record"
    );

    let mut entries = Vec::new();
    let mut cursor = central_offset as usize;
    for _ in 0..count {
        assert_eq!(
            &bytes[cursor..cursor + 4],
            b"PK\x01\x02",
            "missing central directory header"
        );
        let method = u16::from_le_bytes([bytes[cursor + 10], bytes[cursor + 11]]);
        assert_eq!(method, 0, "every entry must be stored");
        let crc = u32::from_le_bytes(bytes[cursor + 16..cursor + 20].try_into().unwrap());
        let size = u32::from_le_bytes(bytes[cursor + 24..cursor + 28].try_into().unwrap());
        let name_len = u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[cursor + 32], bytes[cursor + 33]]) as usize;
        let offset =
            u32::from_le_bytes(bytes[cursor + 42..cursor + 46].try_into().unwrap()) as usize;
        let name = String::from_utf8(bytes[cursor + 46..cursor + 46 + name_len].to_vec())
            .expect("entry names are UTF-8");

        // The local header must agree with the directory, and the data must
        // follow it exactly where the directory says.
        assert_eq!(&bytes[offset..offset + 4], b"PK\x03\x04");
        let local_name = u16::from_le_bytes([bytes[offset + 26], bytes[offset + 27]]) as usize;
        let local_extra = u16::from_le_bytes([bytes[offset + 28], bytes[offset + 29]]) as usize;
        let start = offset + 30 + local_name + local_extra;
        let data = bytes[start..start + size as usize].to_vec();
        assert_eq!(crc32(&data), crc, "CRC mismatch for {name}");

        entries.push(Entry {
            name,
            bytes: data,
            offset,
        });
        cursor += 46 + name_len + extra_len + comment_len;
    }
    entries
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xFFFF_FFFF
}

fn entry<'a>(entries: &'a [Entry], name: &str) -> &'a [u8] {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.bytes.as_slice())
        .unwrap_or_else(|| panic!("missing archive entry {name}"))
}

fn text<'a>(entries: &'a [Entry], name: &str) -> &'a str {
    std::str::from_utf8(entry(entries, name)).expect("text entry")
}

fn decode(bytes: &[u8]) -> image::RgbaImage {
    image::load_from_memory(bytes)
        .expect("decodable PNG")
        .to_rgba8()
}

fn export(canvas: &mut Canvas, path: &PathBuf) -> Vec<Entry> {
    io::export_ora(canvas, path).expect("export");
    read_zip(&fs::read(path).expect("read archive"))
}

// ---- file layout --------------------------------------------------------

#[test]
fn an_ora_archive_has_the_required_layout() {
    let dir = scratch("layout");
    let path = dir.join("painting.ora");

    let mut source = canvas(64, 48, Background::White);
    dab_at(&mut source, 20.0, 20.0, Color32::from_rgb(30, 120, 200));

    let entries = export(&mut source, &path);

    // mimetype must be the first entry, stored and exact.
    assert_eq!(entries[0].name, "mimetype");
    assert_eq!(text(&entries, "mimetype"), "image/openraster");
    assert_eq!(entries[0].offset, 0);
    for required in ["stack.xml", "mergedimage.png", "Thumbnails/thumbnail.png"] {
        assert!(
            entries.iter().any(|entry| entry.name == required),
            "{required} is missing"
        );
    }

    let merged = decode(entry(&entries, "mergedimage.png"));
    assert_eq!((merged.width(), merged.height()), (64, 48));

    let thumbnail = decode(entry(&entries, "Thumbnails/thumbnail.png"));
    assert!(thumbnail.width() <= 256 && thumbnail.height() <= 256);

    let stack = text(&entries, "stack.xml");
    assert!(stack.contains("<image version=\"0.0.6\" w=\"64\" h=\"48\">"));
    assert!(stack.contains("data/layer0.png"));
}

#[test]
fn a_large_document_gets_a_scaled_thumbnail() {
    let dir = scratch("thumbnail");
    let path = dir.join("big.ora");

    let mut source = canvas(600, 300, Background::White);
    dab_at(&mut source, 60.0, 60.0, Color32::from_rgb(10, 10, 10));

    let entries = export(&mut source, &path);
    let thumbnail = decode(entry(&entries, "Thumbnails/thumbnail.png"));
    assert_eq!(thumbnail.width(), 256);
    assert_eq!(thumbnail.height(), 128);
}

// ---- layer tree ---------------------------------------------------------

#[test]
fn layers_groups_names_opacity_and_blends_reach_the_stack() {
    let dir = scratch("stack");
    let path = dir.join("layered.ora");

    let mut source = canvas(64, 48, Background::Transparent);
    let base = source.active_layer_id().expect("base layer");
    dab_at(&mut source, 10.0, 10.0, Color32::from_rgb(200, 30, 30));
    let mut props = source.layer_props(base).expect("props");
    props.name = String::from("Ground");
    props.opacity = 0.5;
    props.blend = BlendMode::Multiply;
    source.set_layer_props(base, props);

    // A second layer, grouped, hidden, in a group that is also semi-opaque.
    let (_, hidden) = source.layer_add_above_active();
    let hidden_id = hidden.id();
    dab_at(&mut source, 30.0, 30.0, Color32::from_rgb(30, 200, 30));
    let mut props = source.layer_props(hidden_id).expect("props");
    props.name = String::from("Glow & \"shine\" <2>");
    props.visible = false;
    props.blend = BlendMode::LinearDodge;
    source.set_layer_props(hidden_id, props);

    let (index, _) = source.group_add_around_active().expect("group");
    let group_id = source.entries()[index].id();
    let mut group_props = source.group_props(group_id).expect("group props");
    group_props.name = String::from("Lighting");
    group_props.opacity = 0.75;
    source.set_group_props(group_id, group_props);

    let entries = export(&mut source, &path);
    let stack = text(&entries, "stack.xml");

    assert!(stack.contains("name=\"Ground\""), "{stack}");
    assert!(stack.contains("opacity=\"0.5\""), "{stack}");
    assert!(stack.contains("composite-op=\"svg:multiply\""), "{stack}");
    assert!(
        stack.contains("name=\"Glow &amp; &quot;shine&quot; &lt;2&gt;\""),
        "layer names must be XML-escaped: {stack}"
    );
    assert!(stack.contains("visibility=\"hidden\""), "{stack}");
    assert!(stack.contains("composite-op=\"svg:plus\""), "{stack}");
    assert!(stack.contains("<stack name=\"Lighting\" opacity=\"0.75\""));
    assert!(stack.contains("isolation=\"isolate\""), "{stack}");

    // Two layers, two PNG streams, both canvas-sized.
    let layers: Vec<&Entry> = entries
        .iter()
        .filter(|entry| entry.name.starts_with("data/") && entry.name.ends_with(".png"))
        .collect();
    assert_eq!(layers.len(), 2, "one PNG per paint layer");
    for layer in &layers {
        let image = decode(&layer.bytes);
        assert_eq!((image.width(), image.height()), (64, 48));
    }
}

#[test]
fn the_top_layer_is_written_first() {
    let dir = scratch("order");
    let path = dir.join("order.ora");

    let mut source = canvas(32, 32, Background::Transparent);
    let bottom = source.active_layer_id().expect("bottom");
    let mut props = source.layer_props(bottom).expect("props");
    props.name = String::from("Bottom");
    source.set_layer_props(bottom, props);

    let (_, top) = source.layer_add_above_active();
    let top_id = top.id();
    let mut props = source.layer_props(top_id).expect("props");
    props.name = String::from("Top");
    source.set_layer_props(top_id, props);

    let entries = export(&mut source, &path);
    let stack = text(&entries, "stack.xml");
    let top_at = stack.find("name=\"Top\"").expect("top layer");
    let bottom_at = stack.find("name=\"Bottom\"").expect("bottom layer");
    assert!(
        top_at < bottom_at,
        "OpenRaster lists the uppermost element first: {stack}"
    );
}

// ---- pixels -------------------------------------------------------------

#[test]
fn a_layers_exported_pixels_match_its_paint() {
    let dir = scratch("pixels");
    let path = dir.join("pixels.ora");

    let mut source = canvas(32, 32, Background::Transparent);
    dab_at(&mut source, 16.0, 16.0, Color32::from_rgb(240, 60, 20));

    let entries = export(&mut source, &path);
    let image = decode(entry(&entries, "data/layer0.png"));

    let centre = image.get_pixel(16, 16);
    assert!(centre[3] > 200, "the dab must be opaque: {centre:?}");
    assert!(
        centre[0] > 200 && centre[1] < 100 && centre[2] < 100,
        "the dab keeps its colour: {centre:?}"
    );
    let corner = image.get_pixel(0, 0);
    assert_eq!(corner[3], 0, "unpainted pixels stay transparent");
}

#[test]
fn masks_and_adjustments_are_baked_into_the_exported_layer() {
    let dir = scratch("baked");
    let path = dir.join("baked.ora");

    let mut source = canvas(32, 32, Background::Transparent);
    let id = source.active_layer_id().expect("layer");
    dab_at(&mut source, 16.0, 16.0, Color32::from_rgb(120, 120, 120));

    // A mask that hides everything except a hole at (16, 16), plus a hue
    // rotation, so both have to be visible in the exported bytes.
    assert!(source.add_layer_mask(id, false), "a new mask");
    let mask = source.layer_mask_mut(id).expect("mask");
    let mut dab = brush(9.0).dab(Color32::WHITE, false);
    dab.radius = 9.0;
    mask.stamp(pos2(16.0, 16.0), &dab);

    let mut props = source.layer_props(id).expect("props");
    props.adjust.brightness = 0.2;
    props.adjust.saturation = 0.0;
    source.set_layer_props(id, props);

    let entries = export(&mut source, &path);
    let image = decode(entry(&entries, "data/layer0.png"));

    assert!(image.get_pixel(16, 16)[3] > 200, "the unmasked hole shows");
    assert_eq!(
        image.get_pixel(2, 2)[3],
        0,
        "masked-away pixels must be clear"
    );

    // Saturation 0 makes the grey dab grey; brightness 0.2 lifts it.
    let centre = image.get_pixel(16, 16);
    assert!(centre[0] > 150, "brightness must be baked in: {centre:?}");
    assert!(
        centre[0] == centre[1] && centre[1] == centre[2],
        "desaturation must be baked in: {centre:?}"
    );

    // The document itself is untouched: the mask and the adjustment are still
    // live and editable in memory.
    assert!(source.layer_mask(id).is_some());
    assert_eq!(
        source.layer_props(id).expect("props").adjust.saturation,
        0.0
    );
}

#[test]
fn the_merged_image_matches_a_png_export_of_the_same_document() {
    let dir = scratch("merged");
    let ora = dir.join("document.ora");
    let png = dir.join("document.png");

    let mut source = canvas(48, 40, Background::White);
    dab_at(&mut source, 20.0, 20.0, Color32::from_rgb(200, 40, 90));
    let (_, second) = source.layer_add_above_active();
    let second_id = second.id();
    dab_at(&mut source, 26.0, 22.0, Color32::from_rgb(20, 90, 200));
    let mut props = source.layer_props(second_id).expect("props");
    props.blend = BlendMode::Screen;
    props.opacity = 0.6;
    source.set_layer_props(second_id, props);

    let entries = export(&mut source, &ora);
    io::export_image(&mut source, &png, io::ImageFormat::Png).expect("png export");

    let merged = decode(entry(&entries, "mergedimage.png"));
    let flattened = decode(&fs::read(&png).expect("png bytes"));
    assert_eq!(merged.dimensions(), flattened.dimensions());
    assert_eq!(
        merged.as_raw(),
        flattened.as_raw(),
        "the archive's merged image must be the same flattened artwork"
    );
}

#[test]
fn an_ora_file_written_to_an_unwritable_place_fails_cleanly() {
    let dir = scratch("unwritable");
    let mut source = canvas(16, 16, Background::White);
    let path = dir.join("missing-folder").join("out.ora");
    let error = io::export_ora(&mut source, &path).expect_err("must fail");
    assert!(error.contains("Could not write"), "{error}");
}

#[test]
fn a_single_layer_document_still_exports_one_stream() {
    let dir = scratch("single");
    let path = dir.join("single.ora");
    let mut source = canvas(16, 16, Background::White);
    let entries = export(&mut source, &path);
    let layers = entries
        .iter()
        .filter(|entry| entry.name.starts_with("data/"))
        .count();
    assert_eq!(layers, 1);
    assert!(text(&entries, "stack.xml").contains("<stack>"));
}

#[test]
fn group_opacity_survives_as_a_stack_attribute() {
    let dir = scratch("group-opacity");
    let path = dir.join("group.ora");

    let mut source = canvas(16, 16, Background::Transparent);
    let (index, _) = source.group_add_around_active().expect("group");
    let group_id = source.entries()[index].id();
    let mut props = source.group_props(group_id).expect("props");
    props.name = String::from("Shading");
    props.visible = false;
    props.opacity = 0.25;
    source.set_group_props(group_id, props);

    let entries = export(&mut source, &path);
    let stack = text(&entries, "stack.xml");
    assert!(
        stack.contains("<stack name=\"Shading\" opacity=\"0.25\" visibility=\"hidden\""),
        "{stack}"
    );
}

#[test]
fn the_exported_layers_keep_their_props_untouched_in_memory() {
    let dir = scratch("untouched");
    let path = dir.join("untouched.ora");

    let mut source = canvas(24, 24, Background::Transparent);
    let id = source.active_layer_id().expect("layer");
    dab_at(&mut source, 12.0, 12.0, Color32::from_rgb(12, 34, 56));
    let before = source.layer_surface(id).expect("surface").pixels().to_vec();
    let props = source.layer_props(id).expect("props");

    export(&mut source, &path);

    assert_eq!(
        source.layer_surface(id).expect("surface").pixels(),
        before.as_slice(),
        "exporting never rewrites the document"
    );
    assert_eq!(source.layer_props(id).expect("props"), props);
}

#[test]
fn a_layer_props_helper_stays_in_sync() {
    // Guards the test-side helper: setting a name must survive into the struct
    // the exporter reads, otherwise the stack assertions would pass vacuously.
    let mut source = canvas(16, 16, Background::Transparent);
    let id = source.active_layer_id().expect("layer");
    let mut props = source.layer_props(id).expect("props");
    props.name = String::from("Renamed");
    source.set_layer_props(id, props);
    let read_back: LayerProps = source.layer_props(id).expect("props");
    assert_eq!(read_back.name, "Renamed");
}

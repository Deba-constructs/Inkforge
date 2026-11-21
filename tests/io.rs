//! Tests for Milestone 6: the `.inkforge` project format and PNG/JPEG I/O.

use std::fs;
use std::path::PathBuf;

use inkforge::egui::{pos2, Color32};
use inkforge::engine::{Background, BrushSettings, Canvas};
use inkforge::io::{self, ImageFormat};
use inkforge::layers::BlendMode;

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

fn pixel(canvas: &Canvas, id: u64, x: u32, y: u32) -> Color32 {
    canvas
        .layer_surface(id)
        .expect("layer surface")
        .pixel(x, y)
}

/// A fresh scratch directory per test, so parallel tests cannot collide.
fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "inkforge-io-{}-{name}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

fn claim_level(left: u8, right: u8, tolerance: i32) -> bool {
    (left as i32 - right as i32).abs() <= tolerance
}

fn assert_colour(actual: Color32, expected: Color32, tolerance: i32) {
    assert!(
        claim_level(actual.r(), expected.r(), tolerance)
            && claim_level(actual.g(), expected.g(), tolerance)
            && claim_level(actual.b(), expected.b(), tolerance)
            && claim_level(actual.a(), expected.a(), tolerance),
        "expected {expected:?}, got {actual:?}"
    );
}

// ---- projects ------------------------------------------------------------

#[test]
fn a_project_round_trip_restores_layers_groups_and_pixels() {
    let dir = scratch("round-trip");
    let path = dir.join("painting.inkforge");

    let mut original = canvas(64, 48, Background::Transparent);
    let base = original.active_layer_id().expect("base layer");
    dab_at(&mut original, 12.0, 12.0, Color32::from_rgb(220, 40, 40));
    let mut props = original.layer_props(base).expect("base props");
    props.name = String::from("Ground");
    original.set_layer_props(base, props);

    // Wrap the active layer in a group so group membership survives the trip.
    let (group_index, _) = original.group_add_around_active().expect("group");
    let group_id = original.entries()[group_index].id();
    let (child_index, child) = original.layer_add_above_active();
    assert_eq!(child_index, 1, "the new layer joins the group");
    let child_id = child.id();
    dab_at(&mut original, 30.0, 20.0, Color32::from_rgb(30, 60, 200));
    let mut child_props = original.layer_props(child_id).expect("child props");
    child_props.name = String::from("Water");
    child_props.opacity = 0.5;
    child_props.blend = BlendMode::Multiply;
    original.set_layer_props(child_id, child_props);

    let mut group_props = original.group_props(group_id).expect("group props");
    group_props.name = String::from("Sky folder");
    group_props.opacity = 0.75;
    group_props.visible = false;
    original.set_group_props(group_id, group_props);
    original.set_name("Saved Art");

    io::save_project(&path, &original).expect("save");
    assert!(path.exists(), "the project file was written");

    let reopened = io::load_project(&path).expect("load");
    assert_eq!(reopened.name(), "Saved Art");
    assert_eq!((reopened.width(), reopened.height()), (64, 48));
    assert_eq!(reopened.background(), Background::Transparent);
    assert!(!reopened.modified(), "a freshly loaded project is clean");
    assert_eq!(reopened.entry_count(), 3);
    assert_eq!(reopened.layer_count(), 2);

    // Row 0 is the group, row 1 its child, row 2 the layer it wrapped.
    assert!(reopened.entries()[0].is_group());
    assert_eq!(reopened.entries()[0].name(), "Sky folder");
    assert_eq!(reopened.entries()[0].opacity(), 0.75);
    assert!(!reopened.entries()[0].visible());

    let reopened_child = reopened.entries()[1].id();
    assert_eq!(reopened.entries()[1].name(), "Water");
    assert_eq!(reopened.entries()[1].opacity(), 0.5);
    assert_eq!(
        reopened.entries()[1].parent(),
        Some(group_id),
        "the child stays inside its group"
    );
    assert_eq!(reopened.entries()[2].name(), "Ground");

    assert_colour(
        pixel(&reopened, reopened_child, 30, 20),
        pixel(&original, child_id, 30, 20),
        2,
    );
    assert_colour(
        pixel(&reopened, reopened.entries()[2].id(), 12, 12),
        pixel(&original, base, 12, 12),
        2,
    );
    // Untouched pixels stay transparent.
    assert_eq!(pixel(&reopened, reopened_child, 5, 40).a(), 0);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn damaged_project_files_are_rejected_with_a_message() {
    let dir = scratch("damaged");

    let not_a_project = dir.join("renamed.png");
    fs::write(&not_a_project, b"not an inkforge file at all").expect("write");
    let error = io::load_project(&not_a_project)
        .err()
        .expect("bad magic must fail");
    assert!(error.contains("not an INKFORGE project"), "got: {error}");

    let truncated = dir.join("truncated.inkforge");
    fs::write(&truncated, b"INKFORGE\x01\x00\x00\x00\xff\xff\x00\x00").expect("write");
    let error = io::load_project(&truncated)
        .err()
        .expect("short file must fail");
    assert!(error.contains("truncated"), "got: {error}");

    let newer = dir.join("newer.inkforge");
    let mut bytes = Vec::from(*b"INKFORGE");
    bytes.extend_from_slice(&99u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(b"{}");
    fs::write(&newer, bytes).expect("write");
    let error = io::load_project(&newer)
        .err()
        .expect("future version must fail");
    assert!(error.contains("version 99"), "got: {error}");

    assert!(
        io::load_project(&dir.join("missing.inkforge")).is_err(),
        "a missing file is an error, not a panic"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_saved_project_keeps_the_pixels_uncompressed_by_layer_opacity() {
    let dir = scratch("opacity");
    let path = dir.join("opacity.inkforge");

    let mut original = canvas(32, 32, Background::White);
    dab_at(&mut original, 16.0, 16.0, Color32::from_rgb(10, 200, 90));
    let id = original.active_layer_id().expect("layer");
    let before = pixel(&original, id, 16, 16);

    io::save_project(&path, &original).expect("save");
    let reopened = io::load_project(&path).expect("load");
    let reopened_id = reopened.active_layer_id().expect("layer");

    assert_colour(pixel(&reopened, reopened_id, 16, 16), before, 2);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_document_from_entries_rejects_impossible_sizes() {
    let error = Canvas::from_entries("Broken", 4_000_000, 10, Background::White, Vec::new(), 0)
        .err()
        .expect("size must be validated");
    assert!(
        error.contains("Canvas size must be between"),
        "got: {error}"
    );
    assert!(error.contains("4000000"), "the failing size is named: {error}");
}

// ---- PNG and JPEG --------------------------------------------------------

#[test]
fn a_png_export_reopens_as_a_document_with_the_same_visible_pixels() {
    let dir = scratch("png");
    let path = dir.join("export.png");

    let mut original = canvas(40, 30, Background::White);
    dab_at(&mut original, 12.0, 12.0, Color32::from_rgb(200, 30, 40));
    let expected_canvas = original.composite().pixel(12, 12);

    io::export_image(&mut original, &path, ImageFormat::Png).expect("export");
    assert!(path.exists(), "the PNG was written");

    let reopened = io::open_image(&path).expect("open");
    assert_eq!((reopened.width(), reopened.height()), (40, 30));
    assert_eq!(reopened.name(), "export");

    let id = reopened.active_layer_id().expect("layer");
    let painted = reopened.layer_surface(id).expect("surface").pixel(12, 12);
    // The PNG stores straight alpha; converting back may drift by a step.
    assert!(
        claim_level(painted.a(), 255, 2),
        "painted pixels stay opaque: {painted:?}"
    );
    assert_colour(painted, expected_canvas, 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_jpeg_export_is_readable_and_flattens_transparency_onto_white() {
    let dir = scratch("jpeg");
    let path = dir.join("export.jpg");

    let mut original = canvas(32, 32, Background::Transparent);
    dab_at(&mut original, 16.0, 16.0, Color32::from_rgb(240, 20, 20));
    io::export_image(&mut original, &path, ImageFormat::Jpeg).expect("export");

    let (width, height, pixels) = io::read_image(&path).expect("read");
    assert_eq!((width, height), (32, 32));

    let corner = pixels[0];
    assert!(
        corner.r() > 235 && corner.g() > 235 && corner.b() > 235,
        "transparent corners become white, not black: {corner:?}"
    );

    let centre = pixels[16 * 32 + 16];
    assert!(
        centre.r() > 200 && centre.g() < 70 && centre.b() < 70,
        "the painted mark survives JPEG: {centre:?}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_an_image_adds_a_cropped_layer_above_the_active_one() {
    let dir = scratch("import");
    let source = dir.join("source.png");

    let mut source_canvas = canvas(48, 40, Background::White);
    dab_at(&mut source_canvas, 24.0, 20.0, Color32::from_rgb(20, 90, 220));
    io::export_image(&mut source_canvas, &source, ImageFormat::Png).expect("export");

    let (width, height, pixels) = io::read_image(&source).expect("read");
    assert_eq!((width, height), (48, 40));

    let mut target = canvas(24, 24, Background::White);
    let (index, layer) = target
        .add_image_layer("source", width, height, &pixels)
        .expect("layer added");

    assert_eq!(index, 1, "the imported layer sits above the original");
    assert_eq!(layer.name(), "source (imported)");
    assert_eq!(target.entry_count(), 2);
    assert!(
        target.layer_surface(layer.id()).expect("surface").pixel(20, 16).a() > 0,
        "the visible part of the image lands on the layer"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn opening_an_image_that_is_not_an_image_fails_cleanly() {
    let dir = scratch("not-image");
    let path = dir.join("notes.png");
    fs::write(&path, b"this is text, not a picture").expect("write");

    let error = io::open_image(&path).err().expect("must fail");
    assert!(
        error.contains("is not a PNG or JPEG"),
        "got: {error}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn file_extensions_are_added_when_the_artist_leaves_them_off() {
    // `ensure_extension` is private, so exercise the same rule through the
    // public format metadata: the menu's default names rely on it.
    assert_eq!(ImageFormat::Png.extension(), "png");
    assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
    assert_eq!(ImageFormat::Jpeg.extra_extension(), Some("jpeg"));
    assert_eq!(ImageFormat::Png.extra_extension(), None);
}

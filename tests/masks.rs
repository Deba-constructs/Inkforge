//! Layer masks: non-destructive hiding of a layer's pixels.
//!
//! A mask is a full-canvas coverage buffer; the layer's own pixels are never
//! changed. These tests check the compositing, mask painting, inversion,
//! deletion, undo, serialisation and how masks move through duplication,
//! document transforms and merge-down.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use inkforge::egui::{pos2, Color32};
use inkforge::engine::{Background, BrushSettings, Canvas, PixelRegion, Transform};
use inkforge::history::{Edit, History, Recorder};

const SIZE: u32 = 16;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_path(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "inkforge-mask-{}-{n}-{name}.inkforge",
        std::process::id()
    ))
}

fn new_canvas() -> Canvas {
    Canvas::try_new("Masks", SIZE, SIZE, Background::White).expect("valid canvas")
}

fn full_region() -> PixelRegion {
    PixelRegion {
        x: 0,
        y: 0,
        width: SIZE,
        height: SIZE,
    }
}

fn fill(canvas: &mut Canvas, id: u64, colour: Color32) {
    let pixels = vec![colour; (SIZE * SIZE) as usize];
    canvas
        .layer_surface_mut(id)
        .expect("layer surface")
        .write_region(full_region(), &pixels);
    canvas.invalidate_composite(None);
}

fn dab(radius: f32) -> inkforge::engine::BrushDab {
    let mut brush = BrushSettings::default();
    brush.set_size(radius * 2.0);
    brush.set_hardness(1.0);
    brush.set_opacity(1.0);
    brush.dab(Color32::BLACK, false)
}

fn coverage(canvas: &Canvas, id: u64, x: u32, y: u32) -> u8 {
    canvas
        .layer_mask(id)
        .expect("mask")
        .pixel(x, y)
        .a()
}

fn composite_alpha(canvas: &mut Canvas, x: u32, y: u32) -> u8 {
    canvas.composite_mut().pixel(x, y).a()
}

fn red() -> Color32 {
    Color32::from_rgba_unmultiplied(230, 20, 20, 255)
}

#[test]
fn a_reveal_all_mask_changes_nothing() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());

    assert!(canvas.add_layer_mask(id, true));
    assert!(canvas.layer_has_mask(id));
    assert_eq!(coverage(&canvas, id, 0, 0), 255);
    assert_eq!(coverage(&canvas, id, 12, 9), 255);
    assert_eq!(composite_alpha(&mut canvas, 6, 6), 255);
}

#[test]
fn a_hide_all_mask_hides_the_layer_without_touching_its_pixels() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());

    assert!(canvas.add_layer_mask(id, false));
    assert_eq!(composite_alpha(&mut canvas, 6, 6), 0);

    // Every pixel is still there, fully opaque: deleting the mask brings the
    // artwork back exactly.
    let surface = canvas.layer_surface(id).expect("surface");
    assert_eq!(surface.pixel(6, 6).a(), 255);
    assert_eq!(surface.pixel(6, 6).r(), red().r());
}

#[test]
fn painting_the_mask_reveals_only_the_painted_area() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    canvas.add_layer_mask(id, false);

    let brush = dab(2.0);
    canvas.paint_layer_mask(id, pos2(5.0, 5.0), pos2(5.0, 5.0), &brush);

    assert_eq!(coverage(&canvas, id, 5, 5), 255);
    assert_eq!(coverage(&canvas, id, 13, 13), 0);
    assert_eq!(composite_alpha(&mut canvas, 5, 5), 255);
    assert_eq!(composite_alpha(&mut canvas, 13, 13), 0);

    // The layer's own pixels were not touched by mask painting.
    assert_eq!(
        canvas.layer_surface(id).expect("surface").pixel(13, 13).a(),
        255
    );
}

#[test]
fn the_eraser_takes_coverage_away_from_the_mask() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    canvas.add_layer_mask(id, true);

    let mut eraser = dab(3.0);
    eraser.erase = true;
    canvas.paint_layer_mask(id, pos2(8.0, 8.0), pos2(8.0, 8.0), &eraser);

    assert_eq!(coverage(&canvas, id, 8, 8), 0);
    assert_eq!(composite_alpha(&mut canvas, 8, 8), 0);
    assert_eq!(coverage(&canvas, id, 1, 1), 255);
}

#[test]
fn inverting_a_mask_swaps_hidden_and_visible() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    canvas.add_layer_mask(id, true);

    let eraser = {
        let mut dab = dab(2.0);
        dab.erase = true;
        dab
    };
    canvas.paint_layer_mask(id, pos2(5.0, 5.0), pos2(5.0, 5.0), &eraser);

    assert!(canvas.invert_layer_mask(id).is_some());

    assert_eq!(coverage(&canvas, id, 5, 5), 255);
    assert_eq!(coverage(&canvas, id, 13, 13), 0);
    assert_eq!(composite_alpha(&mut canvas, 13, 13), 0);
}

#[test]
fn deleting_a_mask_restores_every_pixel() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    canvas.add_layer_mask(id, false);

    let removed = canvas.set_layer_mask(id, None).expect("a mask was there");
    assert_eq!(removed.pixel(4, 4).a(), 0);
    assert!(!canvas.layer_has_mask(id));
    assert_eq!(composite_alpha(&mut canvas, 4, 4), 255);
}

#[test]
fn adding_and_deleting_a_mask_undoes_as_one_step() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    let mut history = History::new();

    assert!(canvas.add_layer_mask(id, false));
    history.push(Edit::mask_added(id, false));
    assert!(canvas.layer_has_mask(id));
    assert_eq!(composite_alpha(&mut canvas, 4, 4), 0);

    history.undo(&mut canvas).expect("undo");
    assert!(!canvas.layer_has_mask(id));
    assert_eq!(composite_alpha(&mut canvas, 4, 4), 255);

    history.redo(&mut canvas).expect("redo");
    assert!(canvas.layer_has_mask(id));
    assert_eq!(composite_alpha(&mut canvas, 4, 4), 0);

    // A deleted mask comes back with its painted coverage intact.
    canvas.paint_layer_mask(id, pos2(4.0, 4.0), pos2(4.0, 4.0), &dab(2.0));
    assert_eq!(coverage(&canvas, id, 4, 4), 255);

    let removed = canvas.set_layer_mask(id, None).expect("removed");
    history.push(Edit::mask_removed(id, removed));
    assert!(!canvas.layer_has_mask(id));
    assert_eq!(composite_alpha(&mut canvas, 4, 4), 255);

    history.undo(&mut canvas).expect("undo delete");
    assert!(canvas.layer_has_mask(id));
}

#[test]
fn mask_pixels_undo_restores_the_original_coverage() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    canvas.add_layer_mask(id, true);

    let mut history = History::new();
    let mut recorder = Recorder::new();
    recorder.record_region(canvas.layer_mask(id).expect("mask"), full_region());
    canvas.invert_layer_mask(id).expect("inverted");
    let patches = recorder.finish(canvas.layer_mask(id).expect("mask"));
    assert!(!patches.is_empty());
    history.push(Edit::mask_pixels(id, patches, "invert layer mask"));

    assert_eq!(coverage(&canvas, id, 7, 7), 0);
    history.undo(&mut canvas).expect("undo");
    assert_eq!(coverage(&canvas, id, 7, 7), 255);
    history.redo(&mut canvas).expect("redo");
    assert_eq!(coverage(&canvas, id, 7, 7), 0);
}

#[test]
fn masks_survive_saving_and_loading() {
    let path = temp_path("roundtrip");
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    canvas.add_layer_mask(id, false);
    canvas.paint_layer_mask(
        id,
        pos2(5.0, 5.0),
        pos2(5.0, 5.0),
        &dab(2.0),
    );

    inkforge::io::save_project(&path, &canvas).expect("save");
    let mut loaded = inkforge::io::load_project(&path).expect("load");
    let _ = std::fs::remove_file(&path);

    assert!(loaded.layer_has_mask(id));
    assert_eq!(coverage(&loaded, id, 5, 5), 255);
    assert_eq!(coverage(&loaded, id, 13, 13), 0);
    assert_eq!(composite_alpha(&mut loaded, 5, 5), 255);
    assert_eq!(composite_alpha(&mut loaded, 13, 13), 0);
}

#[test]
fn duplicating_a_layer_copies_its_mask() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    canvas.add_layer_mask(id, false);

    let (_, copy) = canvas.layer_duplicate(0).expect("duplicate");
    assert!(copy.has_mask(), "the duplicate carries the mask");
    assert_eq!(canvas.layer_mask(copy.id()).expect("mask").pixel(3, 3).a(), 0);
}

#[test]
fn a_document_flip_flips_the_mask_with_the_pixels() {
    let mut canvas = new_canvas();
    let id = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, id, red());
    canvas.add_layer_mask(id, false);
    canvas.paint_layer_mask(id, pos2(4.5, 4.5), pos2(4.5, 4.5), &dab(1.5));
    assert!(coverage(&canvas, id, 4, 4) > 0);

    canvas.transform(Transform::FlipHorizontal);

    assert_eq!(coverage(&canvas, id, 4, 4), 0);
    assert!(coverage(&canvas, id, 11, 4) > 0);
}

#[test]
fn merging_down_bakes_the_mask_into_the_pixels() {
    let mut canvas = new_canvas();
    let bottom = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, bottom, Color32::from_rgba_unmultiplied(20, 30, 220, 255));

    let (index, top) = canvas.layer_add_above_active();
    let top = top.id();
    fill(&mut canvas, top, red());
    canvas.add_layer_mask(top, false);

    canvas.layer_merge_down(index).expect("merge");
    // The hidden top layer left the base untouched.
    let merged = canvas
        .composite_mut()
        .pixel(10, 10);
    assert_eq!((merged.r(), merged.g(), merged.b()), (20, 30, 220));

    // Now a partly revealed merge paints only where the mask allowed.
    let mut canvas = new_canvas();
    let bottom = canvas.active_layer_id().expect("layer");
    fill(&mut canvas, bottom, Color32::from_rgba_unmultiplied(20, 30, 220, 255));
    let (index, top) = canvas.layer_add_above_active();
    let top = top.id();
    fill(&mut canvas, top, red());
    canvas.add_layer_mask(top, false);
    canvas.paint_layer_mask(top, pos2(8.0, 8.0), pos2(8.0, 8.0), &dab(2.0));
    canvas.layer_merge_down(index).expect("merge");

    let revealed = canvas.composite_mut().pixel(8, 8);
    assert!(revealed.r() > 180, "revealed area is red, got {revealed:?}");
    let hidden = canvas.composite_mut().pixel(1, 1);
    assert_eq!((hidden.r(), hidden.g(), hidden.b()), (20, 30, 220));
}

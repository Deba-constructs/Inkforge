//! Whole-document flips and rotations: exact pixel mapping, lossless inverse,
//! resizing on a quarter turn, undo/redo and saving.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use inkforge::app::InkforgeApp;
use inkforge::egui::Color32;
use inkforge::engine::{Background, Canvas, PixelRegion, Transform};
use inkforge::layers::{Entry, Layer, LayerStack};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_path(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "inkforge-transform-{}-{n}-{name}.inkforge",
        std::process::id()
    ))
}

/// A distinct colour per pixel, so any mis-mapping is visible.
fn marker(x: u32, y: u32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (x * 7 % 256) as u8,
        (y * 11 % 256) as u8,
        ((x + y) * 13 % 256) as u8,
        255,
    )
}

fn filled_layer(id: u64, width: u32, height: u32) -> Layer {
    let mut layer = Layer::new(id, format!("layer {id}"), width, height);
    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            data.push(marker(x, y));
        }
    }
    layer.surface_mut().write_region(
        PixelRegion {
            x: 0,
            y: 0,
            width,
            height,
        },
        &data,
    );
    layer
}

fn canvas_with(width: u32, height: u32) -> Canvas {
    let entries = vec![Entry::Layer(filled_layer(0, width, height))];
    Canvas::from_entries("transform", width, height, Background::White, entries, 0)
        .expect("canvas")
}

#[test]
fn flip_horizontal_mirrors_columns() {
    let mut canvas = canvas_with(18, 16);
    let id = canvas.active_layer_id().expect("layer");
    canvas.transform(Transform::FlipHorizontal);

    let surface = canvas.layer_surface(id).expect("surface");
    assert_eq!((surface.width(), surface.height()), (18, 16));
    for y in 0..16 {
        for x in 0..18 {
            assert_eq!(
                surface.pixel(x, y),
                marker(17 - x, y),
                "pixel ({x}, {y}) after a horizontal flip"
            );
        }
    }
}

#[test]
fn flip_vertical_mirrors_rows() {
    let mut canvas = canvas_with(17, 19);
    let id = canvas.active_layer_id().expect("layer");
    canvas.transform(Transform::FlipVertical);

    let surface = canvas.layer_surface(id).expect("surface");
    for y in 0..19 {
        for x in 0..17 {
            assert_eq!(surface.pixel(x, y), marker(x, 18 - y));
        }
    }
}

#[test]
fn rotate_90_cw_swaps_the_document_size() {
    let mut canvas = canvas_with(18, 16);
    let id = canvas.active_layer_id().expect("layer");
    canvas.transform(Transform::Rotate90Cw);

    assert_eq!(
        (canvas.width(), canvas.height()),
        (16, 18),
        "a quarter turn swaps width and height"
    );

    let surface = canvas.layer_surface(id).expect("surface");
    assert_eq!((surface.width(), surface.height()), (16, 18));
    for y in 0..18 {
        for x in 0..16 {
            // The old left column becomes the new top row.
            assert_eq!(surface.pixel(x, y), marker(y, 16 - 1 - x));
        }
    }
}

#[test]
fn every_transform_is_undone_by_its_inverse() {
    for transform in Transform::ALL {
        let mut canvas = canvas_with(18, 16);
        let id = canvas.active_layer_id().expect("layer");
        let before: Vec<Color32> = canvas
            .layer_surface(id)
            .expect("surface")
            .pixels()
            .to_vec();

        canvas.transform(transform);
        assert_ne!(
            canvas.layer_surface(id).expect("surface").pixels(),
            before.as_slice(),
            "{transform:?} changed nothing"
        );

        canvas.transform(transform.inverse());
        assert_eq!(
            (canvas.width(), canvas.height()),
            (18, 16),
            "{transform:?} did not restore the size"
        );
        assert_eq!(
            canvas.layer_surface(id).expect("surface").pixels(),
            before.as_slice(),
            "{transform:?} was not lossless"
        );
    }
}

#[test]
fn a_transform_carries_the_selection_with_the_artwork() {
    let mut canvas = canvas_with(16, 16);
    canvas.select_all();
    assert!(canvas.selection().is_some());

    canvas.transform(Transform::Rotate90Cw);
    let turned = canvas
        .selection()
        .expect("a whole-canvas selection must still cover the turned page");
    assert_eq!(turned.bounds().width, 16, "the box came across the turn");
    assert_eq!(turned.bounds().height, 16);
    assert!(turned.contains(0, 0) && turned.contains(15, 15));

    // Undo applies the inverse transform, which brings the selection back too.
    canvas.transform(Transform::Rotate90Ccw);
    let back = canvas.selection().expect("the selection came back");
    assert_eq!(back.bounds().width, 16);
    assert!(back.contains(0, 0));
}

#[test]
fn every_layer_of_the_stack_is_transformed() {
    let mut stack = LayerStack::from_entries(
        3,
        2,
        vec![
            Entry::Layer(filled_layer(0, 3, 2)),
            Entry::Layer(filled_layer(1, 3, 2)),
        ],
        1,
    );
    stack.transform(Transform::Rotate90Cw);
    assert_eq!((stack.width(), stack.height()), (2, 3));
    for entry in stack.entries() {
        let layer = entry.as_layer().expect("layer");
        assert_eq!(
            (layer.surface().width(), layer.surface().height()),
            (2, 3),
            "every layer surface must follow the canvas"
        );
    }
}

#[test]
fn a_rotation_undoes_and_redoes_through_the_app() {
    let mut app = InkforgeApp::with_defaults();
    let id = app.canvas.active_layer_id().expect("layer");
    let before: Vec<Color32> = app
        .canvas
        .layer_surface(id)
        .expect("surface")
        .pixels()
        .to_vec();
    let (width, height) = (app.canvas.width(), app.canvas.height());

    app.transform_document(Transform::Rotate90Cw);
    assert_eq!(
        (app.canvas.width(), app.canvas.height()),
        (height, width),
        "the document turned"
    );

    app.undo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (width, height));
    assert_eq!(
        app.canvas.layer_surface(id).expect("surface").pixels(),
        before.as_slice(),
        "undo restores the exact pixels"
    );

    app.redo();
    assert_eq!((app.canvas.width(), app.canvas.height()), (height, width));
}

#[test]
fn a_transformed_document_saves_and_reloads() {
    let path = temp_path("save");
    let mut canvas = canvas_with(18, 16);
    canvas.transform(Transform::Rotate90Cw);

    inkforge::io::save_project(&path, &canvas).expect("save");
    let loaded = inkforge::io::load_project(&path).expect("load");
    let _ = std::fs::remove_file(&path);

    assert_eq!((loaded.width(), loaded.height()), (16, 18));

    let id = loaded.active_layer_id().expect("layer");
    let surface = loaded.layer_surface(id).expect("surface");
    for y in 0..18 {
        for x in 0..16 {
            assert_eq!(
                surface.pixel(x, y),
                marker(y, 15 - x),
                "the turned pixels must survive the project file"
            );
        }
    }
}

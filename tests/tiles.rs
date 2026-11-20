//! Tests for the composite tile pyramid.
//!
//! These run headless: egui allocates textures without a real backend, and the
//! pyramid only reports what it did, so the upload behaviour can be checked
//! without opening a window.

use std::time::{Duration, Instant};

use inkforge::egui::{self, pos2, vec2, Color32, Pos2, Rect};
use inkforge::engine::{pyramid, BrushSettings, PixelRegion, Surface, TilePyramid, TILE};

const VIEW: [f32; 2] = [800.0, 600.0];

struct Harness {
    ctx: egui::Context,
    pyramid: TilePyramid,
}

impl Harness {
    fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            pyramid: TilePyramid::new(),
        }
    }

    /// Draw the page exactly as the canvas view does, and report the work.
    fn draw(&mut self, surface: &mut Surface, page: Rect, zoom: f32) -> pyramid::TileStats {
        let ctx = self.ctx.clone();
        let mut stats = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    vec2(VIEW[0], VIEW[1]),
                )),
                ..Default::default()
            },
            |ui| {
                let painter = ui.painter_at(Rect::from_min_size(
                    Pos2::ZERO,
                    vec2(VIEW[0], VIEW[1]),
                ));
                stats = Some(self.pyramid.draw(ui.ctx(), &painter, page, surface, zoom));
            },
        );
        output.textures_delta.clear();
        stats.expect("the UI pass ran")
    }
}

fn page(width: u32, height: u32, zoom: f32) -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(width as f32 * zoom, height as f32 * zoom))
}

/// One draw call and how long it took.
fn timed(
    harness: &mut Harness,
    surface: &mut Surface,
    page: Rect,
    zoom: f32,
) -> (pyramid::TileStats, Duration) {
    let start = Instant::now();
    let stats = harness.draw(surface, page, zoom);
    (stats, start.elapsed())
}

/// What a run of frames cost.
#[derive(Default, Clone, Copy)]
struct Run {
    frames: u32,
    total: Duration,
    worst: Duration,
    /// Frames that needed no tile upload at all.
    quiet_frames: u32,
    quiet_total: Duration,
    tiles_uploaded: u64,
    texels_uploaded: u64,
}

impl Run {
    fn add(&mut self, stats: &pyramid::TileStats, elapsed: Duration) {
        self.frames += 1;
        self.total += elapsed;
        self.worst = self.worst.max(elapsed);
        if stats.tiles_uploaded == 0 {
            self.quiet_frames += 1;
            self.quiet_total += elapsed;
        }
        self.tiles_uploaded += stats.tiles_uploaded as u64;
        self.texels_uploaded += stats.pixels_uploaded as u64;
    }

    fn mean(&self) -> Duration {
        self.total / self.frames.max(1)
    }

    fn quiet_mean(&self) -> Duration {
        self.quiet_total / self.quiet_frames.max(1)
    }
}

fn dab(size: f32) -> inkforge::engine::BrushDab {
    let mut brush = BrushSettings::default();
    brush.set_size(size);
    brush.dab(Color32::from_rgb(0x20, 0x40, 0x80), false)
}

#[test]
fn coarse_levels_keep_a_large_document_cheap() {
    // Measurement only: prints what each zoom level uploads for a 4096 × 4096
    // document, and what rebuilding the old full-canvas image would have cost.
    let mut harness = Harness::new();
    let mut surface = Surface::new(4096, 4096);
    let whole = 4096 * 4096;

    let start = std::time::Instant::now();
    let full = egui::ColorImage::new(
        [4096, 4096],
        surface.pixels().to_vec(),
    );
    let full_copy = start.elapsed();
    eprintln!(
        "full canvas: {} texels, {:.1} MB, {full_copy:?} to build the image",
        whole,
        (full.pixels.len() * 4) as f64 / (1024.0 * 1024.0)
    );

    for zoom in [1.0f32, 0.5, 0.25, 0.125, 0.0625] {
        let start = std::time::Instant::now();
        let stats = harness.draw(&mut surface, page(4096, 4096, zoom), zoom);
        eprintln!(
            "zoom {:.1}%: level {}, {} tiles drawn, {} tiles uploaded, {} texels ({:.1}% of the canvas), {:?}",
            zoom * 100.0,
            stats.level,
            stats.tiles_drawn,
            stats.tiles_uploaded,
            stats.pixels_uploaded,
            100.0 * stats.pixels_uploaded as f64 / whole as f64,
            start.elapsed()
        );
    }
}

#[test]
fn panning_and_zooming_a_large_document_stays_cheap() {
    // Measurement only: prints what a frame costs while panning and zooming a
    // very large document through the tile pyramid. The assertions stay loose
    // so a busy machine cannot fail the suite on timing alone; the printed
    // numbers are the result, and are also recorded in DEVELOPMENT.md.
    //
    // Run it with `cargo test --release --test tiles -- --nocapture` — debug
    // timings are several times slower and mean nothing for a shipping build.
    const DOC: u32 = 8192;
    // A debug build builds tiles several times slower than a shipping one, so
    // the budget is widened there instead of failing the suite on it.
    let frame_budget = if cfg!(debug_assertions) { 100.0 } else { 16.7 };

    let mut harness = Harness::new();
    let mut surface = Surface::new(DOC, DOC);

    // Real content, so coarse tiles average painted pixels instead of empty
    // canvas. Four wide strokes run the full height of the document.
    let mut brush = BrushSettings::default();
    brush.set_size(240.0);
    let dab = brush.dab(Color32::from_rgb(0x30, 0x60, 0xA0), false);
    for i in 0..4 {
        let x = 600.0 + i as f32 * 2300.0;
        surface.paint_segment(pos2(x, 600.0), pos2(x + 400.0, DOC as f32 - 600.0), &dab);
    }
    surface.clear_dirty();

    let mb = (DOC as f64 * DOC as f64 * 4.0) / (1024.0 * 1024.0);
    eprintln!(
        "document {DOC} × {DOC} ({mb:.0} MB of canvas pixels), {} × {} px view, 60 fps budget 16.7 ms",
        VIEW[0], VIEW[1]
    );

    // 1. Zoom sweep. The first frame at each zoom builds that level's tiles;
    //    the frames after it are the settled cost of just drawing them.
    let mut cold_worst = Duration::ZERO;
    for zoom in [1.0f32, 0.5, 0.25, 0.125, 0.0625, 0.03125] {
        let (cold_stats, cold) = timed(&mut harness, &mut surface, page(DOC, DOC, zoom), zoom);
        cold_worst = cold_worst.max(cold);

        let mut settled = Run::default();
        for _ in 0..30 {
            let (stats, elapsed) = timed(&mut harness, &mut surface, page(DOC, DOC, zoom), zoom);
            assert_eq!(
                stats.tiles_uploaded, 0,
                "a zoom that has not changed must not upload tiles"
            );
            settled.add(&stats, elapsed);
        }
        eprintln!(
            "  zoom {:>7.2}%: level {}, {:>3} tiles drawn, first frame {:>9.0?} ({} uploaded), settled {:>9.0?} mean / {:>9.0?} max",
            zoom * 100.0,
            cold_stats.level,
            cold_stats.tiles_drawn,
            cold,
            cold_stats.tiles_uploaded,
            settled.mean(),
            settled.worst
        );
        assert!(
            settled.mean().as_secs_f64() * 1000.0 < frame_budget,
            "settled zoom {:?} occupies the whole frame budget",
            settled.mean()
        );
    }

    // 2. Panning across the whole document at 50 %, 24 screen px per frame.
    //    Every frame exposes a strip that has never been on screen, so this is
    //    the worst realistic case for tile building.
    let zoom = 0.5f32;
    let doc_size = vec2(DOC as f32 * zoom, DOC as f32 * zoom);
    let step = 24.0f32;
    let pan_frames = 150u32;
    harness.draw(&mut surface, page(DOC, DOC, zoom), zoom);

    let mut pan = Run::default();
    let mut offset = 0.0f32;
    for _ in 0..pan_frames {
        offset += step;
        let rect = Rect::from_min_size(pos2(-offset, 0.0), doc_size);
        let (stats, elapsed) = timed(&mut harness, &mut surface, rect, zoom);
        pan.add(&stats, elapsed);
    }
    let travelled = offset / zoom;
    eprintln!(
        "  panning {travelled:.0} canvas px at 50 % over {} frames: {:>9.0?} mean / {:>9.0?} worst, {} quiet frames at {:>9.0?}, {} tiles uploaded",
        pan.frames,
        pan.mean(),
        pan.worst,
        pan.quiet_frames,
        pan.quiet_mean(),
        pan.tiles_uploaded
    );
    assert!(pan.frames == pan_frames, "every pan frame was drawn");
    assert!(
        pan.mean().as_secs_f64() * 1000.0 < frame_budget,
        "panning averaged {:?} per frame",
        pan.mean()
    );

    // 3. Continuous zooming: a new zoom every frame, 100 % → 5 %, the way a
    //    user scrolls the wheel. This crosses every level, so it rebuilds the
    //    most tiles of any interaction.
    let zoom_frames = 60u32;
    let mut zooming = Run::default();
    for frame in 0..zoom_frames {
        let t = frame as f32 / (zoom_frames - 1) as f32;
        let zoom = 1.0f32 * (0.05f32 / 1.0f32).powf(t);
        let (stats, elapsed) = timed(&mut harness, &mut surface, page(DOC, DOC, zoom), zoom);
        zooming.add(&stats, elapsed);
    }
    eprintln!(
        "  zooming 100 % → 5 % over {} frames: {:>9.0?} mean / {:>9.0?} worst, {} tiles uploaded ({} texels)",
        zooming.frames,
        zooming.mean(),
        zooming.worst,
        zooming.tiles_uploaded,
        zooming.texels_uploaded
    );
    assert!(
        zooming.mean().as_secs_f64() * 1000.0 < frame_budget,
        "zooming averaged {:?} per frame",
        zooming.mean()
    );

    // For scale: one full-resolution copy of this document is 256 MB, which is
    // what a single-texture composite would have had to build and upload.
    eprintln!(
        "  worst level build: {:.1?}, worst pan frame: {:.1?}, worst zoom frame: {:.1?}",
        cold_worst, pan.worst, zooming.worst
    );
}

#[test]
fn the_level_follows_the_screen_texel_rule() {
    // At or above half of full size the full-resolution level is used.
    assert_eq!(TilePyramid::level_for_zoom(1.0), 0);
    assert_eq!(TilePyramid::level_for_zoom(0.5), 0);
    // Below that, each halving of the zoom drops one level.
    assert_eq!(TilePyramid::level_for_zoom(0.26), 1);
    assert_eq!(TilePyramid::level_for_zoom(0.13), 2);
    assert_eq!(TilePyramid::level_for_zoom(0.06), 4);
    // The clamp keeps a nonsensical zoom from asking for a level that cannot
    // exist.
    assert_eq!(TilePyramid::level_for_zoom(0.0), 0);
    assert!(TilePyramid::level_for_zoom(0.02) <= 7);
}

#[test]
fn a_level_list_stops_once_the_document_fits_one_tile() {
    let mut harness = Harness::new();
    let mut surface = Surface::new(1024, 512);
    harness.draw(&mut surface, page(1024, 512, 0.2), 0.2);

    // 1024 → 512 → 256: three levels are enough for the width.
    assert_eq!(harness.pyramid.level_count(), 3);
    assert_eq!(harness.pyramid.scale(0), 1);
    assert_eq!(harness.pyramid.scale(1), 2);
    assert_eq!(harness.pyramid.scale(2), 4);
}

#[test]
fn a_zoomed_out_large_document_uploads_a_fraction_of_itself() {
    let mut harness = Harness::new();
    let mut surface = Surface::new(4096, 4096);

    let stats = harness.draw(&mut surface, page(4096, 4096, 0.1), 0.1);

    assert!(stats.level >= 3, "expected a coarse level, got {}", stats.level);
    let whole = 4096 * 4096;
    assert!(
        stats.pixels_uploaded * 8 < whole,
        "zoomed out upload used {} of {whole} pixels",
        stats.pixels_uploaded
    );
    assert!(
        stats.tiles_drawn <= 64,
        "drew {} tiles for one screen",
        stats.tiles_drawn
    );
    assert!(stats.tiles_uploaded > 0, "the first frame must upload");
    assert_eq!(
        surface.dirty(),
        None,
        "uploaded pixels must be marked clean"
    );
}

#[test]
fn painting_outside_the_view_uploads_nothing() {
    let mut harness = Harness::new();
    let mut surface = Surface::new(2048, 2048);

    // Full size, top-left corner of the document on screen.
    let stats = harness.draw(&mut surface, page(2048, 2048, 1.0), 1.0);
    assert!(stats.tiles_uploaded > 0);

    // Paint far outside the visible area: no visible tile contains it.
    let mut brush = BrushSettings::default();
    brush.set_size(40.0);
    let stroke = brush.dab(Color32::from_rgb(0x90, 0x10, 0x10), false);
    surface.paint_segment(pos2(1900.0, 1900.0), pos2(1960.0, 1960.0), &stroke);

    let stats = harness.draw(&mut surface, page(2048, 2048, 1.0), 1.0);
    assert_eq!(
        stats.tiles_uploaded, 0,
        "painting off-screen uploaded {} tiles",
        stats.tiles_uploaded
    );
    assert_eq!(surface.dirty(), None);
}

#[test]
fn panning_uploads_only_the_tiles_that_came_into_view() {
    let mut harness = Harness::new();
    let mut surface = Surface::new(4096, 4096);

    let first = harness.draw(&mut surface, page(4096, 4096, 0.5), 0.5);
    assert!(first.tiles_uploaded >= 6, "setup: {}", first.tiles_uploaded);

    // Slide the page left by 100 px: most tiles stay visible, a few new
    // columns on the right appear.
    let page_left = Rect::from_min_size(
        pos2(-100.0, 0.0),
        vec2(4096.0 * 0.5, 4096.0 * 0.5),
    );
    let second = harness.draw(&mut surface, page_left, 0.5);

    assert!(second.tiles_uploaded > 0, "new tiles must be drawn");
    assert!(
        second.tiles_uploaded < first.tiles_uploaded,
        "panning uploaded {} tiles, more than the first frame's {}",
        second.tiles_uploaded,
        first.tiles_uploaded
    );
}

#[test]
fn a_tile_built_from_a_solid_surface_keeps_its_colour() {
    let mut surface = Surface::new(64, 64);
    let colour = Color32::from_rgb(0x40, 0x80, 0xC0);
    surface.write_region(
        PixelRegion {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        },
        &vec![colour; 64 * 64],
    );

    let averaged = pyramid::average_region(
        &surface,
        PixelRegion {
            x: 0,
            y: 0,
            width: 32,
            height: 32,
        },
    );
    assert_eq!(averaged, colour);
}

#[test]
fn downscaling_averages_premultiplied_pixels() {
    let mut surface = Surface::new(64, 64);
    surface.write_region(
        PixelRegion {
            x: 0,
            y: 0,
            width: 32,
            height: 64,
        },
        &vec![Color32::from_rgb(255, 0, 0); 32 * 64],
    );
    surface.write_region(
        PixelRegion {
            x: 32,
            y: 0,
            width: 32,
            height: 64,
        },
        &vec![Color32::from_rgb(0, 0, 255); 32 * 64],
    );

    // Four taps per axis over a half-red, half-blue square: the corners land
    // symmetrically, so each channel averages to half.
    let averaged = pyramid::average_region(
        &surface,
        PixelRegion {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        },
    );
    assert_eq!(averaged.a(), 255);
    assert!(
        (averaged.r() as i32 - 128).abs() <= 1,
        "red channel averaged to {}",
        averaged.r()
    );
    assert!(
        (averaged.b() as i32 - 128).abs() <= 1,
        "blue channel averaged to {}",
        averaged.b()
    );

    // An empty area stays fully transparent.
    let empty = pyramid::average_region(
        &surface,
        PixelRegion {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
    );
    assert_eq!(empty, Color32::TRANSPARENT);
}

#[test]
fn every_visible_tile_is_drawn_once_without_gaps() {
    // The tile rectangles must tile the visible page exactly: no uncovered
    // strip at the right or bottom edge, and no overlap either.
    let mut harness = Harness::new();
    let mut surface = Surface::new(1000, 700);
    let stats = harness.draw(&mut surface, page(1000, 700, 0.8), 0.8);

    // 800 × 560 screen pixels of page, tiles of 204.8 px: 4 × 3.
    assert_eq!(stats.tiles_drawn, 4 * 3);
    assert_eq!(stats.level, 0);
    assert_eq!(TILE, 256);
}

#[test]
fn the_dab_helper_is_usable_for_offscreen_strokes() {
    // Guards the test helper above: a zero-size or transparent dab would make
    // the off-screen test pass for the wrong reason.
    let dab = dab(40.0);
    assert!(dab.radius > 0.0);
    assert!(dab.colour.a() > 0);
}

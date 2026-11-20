//! Dirty-region bookkeeping behind the partial GPU uploads.
//!
//! These checks exist because the optimisation is invisible: if the dirty
//! rectangle were wrong, the screen would show stale or torn pixels and no
//! compiler would notice. Keeping the region honest is what makes it safe to
//! upload only that rectangle.

use std::time::Instant;

use inkforge::egui::{pos2, Color32, Pos2};
use inkforge::engine::{BrushSettings, PixelRegion, Surface};

fn small_dab() -> inkforge::engine::BrushDab {
    let mut brush = BrushSettings::default();
    brush.set_size(10.0);
    brush.dab(Color32::BLACK, false)
}

#[test]
fn a_small_dab_marks_only_the_pixels_it_touches() {
    let mut surface = Surface::new(1024, 768);
    surface.clear_dirty();

    let dab = small_dab();
    surface.stamp(pos2(100.0, 100.0), &dab);

    let dirty = surface.dirty().expect("a stamp must mark pixels dirty");
    assert!(dirty.width <= 16, "dirty width was {}", dirty.width);
    assert!(dirty.height <= 16, "dirty height was {}", dirty.height);
    assert!(dirty.pixel_count() < 400, "dirty region {:?}", dirty);
    // The dab is centred on (100, 100), so the region must contain it.
    assert!(dirty.x <= 100 && dirty.x + dirty.width >= 100);
    assert!(dirty.y <= 100 && dirty.y + dirty.height >= 100);
}

#[test]
fn separate_dabs_accumulate_into_one_bounding_region() {
    let mut surface = Surface::new(400, 400);
    surface.clear_dirty();

    let dab = small_dab();
    surface.stamp(pos2(50.0, 40.0), &dab);
    let first = surface.dirty().expect("first dab");
    surface.stamp(pos2(300.0, 250.0), &dab);
    let both = surface.dirty().expect("second dab");

    assert!(both.x <= first.x);
    assert!(both.y <= first.y);
    assert!(both.x + both.width >= 300);
    assert!(both.y + both.height >= 250);
}

#[test]
fn a_full_clear_marks_the_whole_surface() {
    let mut surface = Surface::new(64, 32);
    surface.write_region(
        PixelRegion {
            x: 0,
            y: 0,
            width: 64,
            height: 32,
        },
        &[Color32::WHITE; 64 * 32],
    );
    assert_eq!(
        surface.dirty(),
        Some(PixelRegion {
            x: 0,
            y: 0,
            width: 64,
            height: 32,
        })
    );

    surface.clear();
    assert_eq!(
        surface.dirty(),
        Some(PixelRegion {
            x: 0,
            y: 0,
            width: 64,
            height: 32,
        }),
        "clearing must invalidate the whole texture"
    );
}

#[test]
fn writing_a_region_marks_exactly_that_region() {
    let mut surface = Surface::new(100, 100);
    surface.clear_dirty();

    let region = PixelRegion {
        x: 10,
        y: 20,
        width: 5,
        height: 4,
    };
    surface.write_region(region, &[Color32::WHITE; 20]);

    assert_eq!(surface.dirty(), Some(region));

    surface.clear_dirty();
    assert_eq!(surface.dirty(), None, "an uploaded region is forgotten");
}

#[test]
fn many_workers_stamp_exactly_the_same_pixels_as_one() {
    let mut brush = BrushSettings::default();
    brush.set_size(600.0);
    brush.set_hardness(0.6);
    brush.set_opacity(0.55);
    brush.set_grain(0.35);

    // A large dab, a dab clipped by the canvas edge, and a large erase: the
    // three paths a big brush takes. If the bands shared even one pixel the
    // results would differ.
    for (erase, centre) in [
        (false, pos2(600.0, 600.0)),
        (false, pos2(20.0, 30.0)),
        (true, pos2(600.0, 600.0)),
    ] {
        let dab = brush.dab(Color32::from_rgb(0x40, 0x80, 0xC0), erase);
        let mut one = Surface::new(1200, 1200);
        let mut several = Surface::new(1200, 1200);
        one.stamp_with_workers(centre, &dab, 1);
        several.stamp_with_workers(centre, &dab, 4);
        assert_eq!(
            one.pixels(),
            several.pixels(),
            "one worker and four workers disagree at {centre:?} (erase: {erase})"
        );
    }
}

#[test]
fn a_large_dab_is_cheaper_with_several_workers() {
    // Measurement only: the assertion above proves correctness. Timings are
    // printed for the verification log and are deliberately not asserted, so a
    // busy or single-core machine cannot fail the suite.
    let mut brush = BrushSettings::default();
    brush.set_size(600.0);
    brush.set_hardness(0.6);
    brush.set_opacity(0.55);
    let dab = brush.dab(Color32::from_rgb(0x40, 0x80, 0xC0), false);

    let repeats = 10;

    for size in [300.0f32, 600.0] {
        let mut bench_dab = dab;
        bench_dab.radius = size * 0.5;

        for workers in [1usize, 4, 8] {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..repeats {
                let mut surface = Surface::new(1200, 1200);
                let start = Instant::now();
                surface.stamp_with_workers(pos2(600.0, 600.0), &bench_dab, workers);
                total += start.elapsed();
            }
            eprintln!(
                "{size} px dab, {repeats} runs: {workers} worker(s) {:?} each",
                total / repeats
            );
        }
    }
}

#[test]
fn a_full_canvas_segment_does_not_claim_a_giant_region_for_a_short_stroke() {
    // A short diagonal stroke must not dirty the whole surface, otherwise the
    // renderer would fall back to a full upload every frame.
    let mut surface = Surface::new(4096, 4096);
    surface.clear_dirty();

    let dab = small_dab();
    surface.paint_segment(Pos2::new(2000.0, 2000.0), Pos2::new(2100.0, 2050.0), &dab);

    let dirty = surface.dirty().expect("the stroke must mark pixels");
    assert!(
        dirty.pixel_count() < 4096 * 4096 / 100,
        "a short stroke dirtied {} pixels",
        dirty.pixel_count()
    );
}

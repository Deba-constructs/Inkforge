//! Tests for brush presets, spacing and grain.

use inkforge::egui::{pos2, Color32};
use inkforge::engine::{BrushSettings, Surface, PRESETS};

/// Total alpha in the surface — a rough measure of how much ink a stroke laid down.
fn ink_mass(surface: &Surface) -> u64 {
    surface.pixels().iter().map(|pixel| pixel.a() as u64).sum()
}

/// A cheap hash of every alpha value, so two strokes that lay down the same
/// amount of ink in a different shape still count as different marks.
fn alpha_hash(surface: &Surface) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for pixel in surface.pixels() {
        hash ^= pixel.a() as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Paint the same stroke with a preset and return
/// (ink mass, covered pixels, shape hash).
fn paint_with_preset(index: usize) -> (u64, usize, u64) {
    let mut brush = BrushSettings::default();
    brush.apply_preset(&PRESETS[index]);
    let dab = brush.dab(Color32::BLACK, false);

    let mut surface = Surface::new(256, 256);
    surface.paint_segment(pos2(40.0, 128.0), pos2(216.0, 128.0), &dab);

    let covered = surface.pixels().iter().filter(|pixel| pixel.a() > 0).count();
    (ink_mass(&surface), covered, alpha_hash(&surface))
}

/// Index of a preset by name, so the library can be reordered freely.
fn index_of(name: &str) -> usize {
    PRESETS
        .iter()
        .position(|preset| preset.name == name)
        .unwrap_or_else(|| panic!("no preset named {name}"))
}

#[test]
fn default_settings_are_the_first_preset() {
    assert_eq!(BrushSettings::default().matching_preset(), Some(0));
}

#[test]
fn every_preset_round_trips_through_the_settings() {
    for (index, preset) in PRESETS.iter().enumerate() {
        let mut brush = BrushSettings::default();
        brush.apply_preset(preset);
        assert_eq!(
            brush.matching_preset(),
            Some(index),
            "preset {} does not survive apply → match",
            preset.name
        );
    }
}

#[test]
fn changing_a_setting_leaves_the_preset() {
    let mut brush = BrushSettings::default();
    brush.set_size(77.0);
    assert_eq!(brush.matching_preset(), None);
    // ... and comes back when the value returns to the preset's.
    brush.set_size(PRESETS[0].size);
    assert_eq!(brush.matching_preset(), Some(0));
}

#[test]
fn presets_produce_distinct_marks() {
    let signatures: Vec<(u64, usize, u64)> = (0..PRESETS.len()).map(paint_with_preset).collect();

    for left in 0..signatures.len() {
        for right in left + 1..signatures.len() {
            assert_ne!(
                signatures[left], signatures[right],
                "presets '{}' and '{}' paint indistinguishable marks",
                PRESETS[left].name,
                PRESETS[right].name
            );
        }
    }

    // The requirement is that presets feel different, not merely that the
    // numbers differ: the airbrush must cover far more of the canvas than the pen.
    let (_, pen_coverage, _) = signatures[index_of("Ink Pen")];
    let (_, airbrush_coverage, _) = signatures[index_of("Airbrush")];
    assert!(
        airbrush_coverage > pen_coverage * 3,
        "airbrush covered {airbrush_coverage} px, pen covered {pen_coverage} px"
    );
}

#[test]
fn tight_spacing_paints_solidly_and_loose_spacing_is_lighter() {
    let mut brush = BrushSettings::default();
    brush.set_size(20.0);
    brush.set_hardness(1.0);

    let mut stroke = |spacing: f32| {
        brush.set_spacing(spacing);
        let mut surface = Surface::new(200, 60);
        surface.paint_segment(
            pos2(20.0, 30.0),
            pos2(180.0, 30.0),
            &brush.dab(Color32::BLACK, false),
        );
        surface
    };

    let tight = stroke(0.05);
    let tight_min = (22..178).map(|x| tight.pixel(x, 30).a()).min().unwrap();
    assert!(
        tight_min > 240,
        "tight spacing should be solid, lowest alpha was {tight_min}"
    );

    let loose = stroke(0.9);
    assert!(
        ink_mass(&loose) < ink_mass(&tight),
        "wider spacing must lay down less ink ({} vs {})",
        ink_mass(&loose),
        ink_mass(&tight)
    );
}

#[test]
fn grain_varies_alpha_across_the_tip() {
    let mut brush = BrushSettings::default();
    brush.set_size(40.0);
    brush.set_hardness(1.0);
    brush.set_grain(1.0);

    let mut grainy = Surface::new(64, 64);
    grainy.stamp(pos2(32.0, 32.0), &brush.dab(Color32::BLACK, false));

    let samples: Vec<u8> = (26..38)
        .flat_map(|y| (26..38).map(move |x| (x, y)))
        .map(|(x, y)| grainy.pixel(x, y).a())
        .collect();
    let min = *samples.iter().min().unwrap();
    let max = *samples.iter().max().unwrap();
    assert!(min < 120, "grain should expose light pixels, min {min}");
    assert!(max > 200, "grain should still leave solid pixels, max {max}");

    // Grain is deterministic: the same dab gives the same pixels again.
    let mut again = Surface::new(64, 64);
    again.stamp(pos2(32.0, 32.0), &brush.dab(Color32::BLACK, false));
    assert_eq!(grainy.pixels(), again.pixels());
}

#[test]
fn a_clean_tip_has_a_uniform_core() {
    let mut brush = BrushSettings::default();
    brush.set_size(40.0);
    brush.set_hardness(1.0);
    brush.set_grain(0.0);

    let mut surface = Surface::new(64, 64);
    surface.stamp(pos2(32.0, 32.0), &brush.dab(Color32::BLACK, false));

    let centre = surface.pixel(32, 32).a();
    assert!(centre > 240);
    for (x, y) in [(28, 32), (36, 32), (32, 28), (32, 36)] {
        assert_eq!(surface.pixel(x, y).a(), centre);
    }
}

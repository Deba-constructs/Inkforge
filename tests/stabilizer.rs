//! Tests for the stroke stabiliser.

use inkforge::egui::{pos2, Pos2};
use inkforge::engine::stabilizer::MAX_LAG;
use inkforge::engine::Stabilizer;

#[test]
fn zero_smoothing_passes_the_pointer_through() {
    let mut stabilizer = Stabilizer::begin(pos2(10.0, 10.0));
    let raw = pos2(140.0, 90.0);
    let out = stabilizer.push(raw, 0.0);
    assert!((out - raw).length() < 1e-4);
}

#[test]
fn smoothing_flattens_a_shaky_path() {
    let mut stabilizer = Stabilizer::begin(pos2(0.0, 0.0));
    let mut raw_deviation = 0.0f32;
    let mut smooth_deviation = 0.0f32;

    for step in 0..60 {
        let x = step as f32 * 4.0;
        // Shake +/-6 px around the horizontal centreline.
        let y = if step % 2 == 0 { 6.0 } else { -6.0 };
        let out = stabilizer.push(pos2(x, y), 0.85);

        raw_deviation = raw_deviation.max(y.abs());
        smooth_deviation = smooth_deviation.max(out.y.abs());
    }

    assert!(
        smooth_deviation < raw_deviation * 0.75,
        "stabilised deviation {smooth_deviation} should be much flatter than raw {raw_deviation}"
    );
}

#[test]
fn high_smoothing_lags_but_never_more_than_the_limit() {
    let mut stabilizer = Stabilizer::begin(Pos2::ZERO);
    let target = pos2(600.0, 0.0);
    let out = stabilizer.push(target, 1.0);

    let lag = (target - out).length();
    assert!(lag > 0.0, "a strong stabiliser must smooth, not snap");
    assert!(
        lag <= MAX_LAG + 0.001,
        "lag {lag} must stay within {MAX_LAG}"
    );
}

#[test]
fn low_smoothing_follows_quickly() {
    let mut stabilizer = Stabilizer::begin(Pos2::ZERO);
    let target = pos2(30.0, 0.0);
    let out = stabilizer.push(target, 0.1);
    assert!(
        (out - target).length() < 8.0,
        "a light stabiliser should stay close to the pointer"
    );
}

#[test]
fn a_fast_flick_still_reaches_the_target_over_a_few_updates() {
    let mut stabilizer = Stabilizer::begin(Pos2::ZERO);
    let target = pos2(200.0, 120.0);

    let mut out = Pos2::ZERO;
    for _ in 0..40 {
        out = stabilizer.push(target, 0.9);
    }

    assert!(
        (out - target).length() < 0.5,
        "holding still must converge on the pointer, ended {out:?}"
    );
}

#[test]
fn non_finite_input_is_ignored() {
    let mut stabilizer = Stabilizer::begin(pos2(5.0, 5.0));
    let out = stabilizer.push(pos2(f32::NAN, f32::INFINITY), 0.5);
    assert_eq!(out, pos2(5.0, 5.0));
}

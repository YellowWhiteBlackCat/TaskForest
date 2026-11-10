//! Headless tests for the shared chart slide-in vocabulary.

use std::time::Duration;

use super::{CHART_SLIDE_DURATION, chart_slide_ramp};

#[test]
fn the_chart_slide_is_one_hundred_eighty_milliseconds() {
    assert_eq!(CHART_SLIDE_DURATION, Duration::from_millis(180));
    assert_eq!(CHART_SLIDE_DURATION.as_secs_f32(), 0.18);
}

#[test]
fn the_slide_ramp_starts_and_settles_exactly() {
    assert_eq!(chart_slide_ramp(0.0), 0.0);
    assert_eq!(chart_slide_ramp(1.0), 1.0);
    // The midpoint of a symmetric ease-in-out is the midpoint of the sweep.
    assert_eq!(chart_slide_ramp(0.5), 0.5);
}

#[test]
fn the_slide_ramp_is_symmetric_monotonic_and_never_overshoots() {
    let mut previous = 0.0;
    for step in 0..=10_000 {
        let t = step as f32 / 10_000.0;
        let eased = chart_slide_ramp(t);
        assert!(
            (0.0..=1.0).contains(&eased),
            "ramp left the unit interval: {eased}"
        );
        assert!(
            eased >= previous,
            "ramp went backwards at {t}: {eased} < {previous}"
        );
        previous = eased;
        // Ease-in-out spends equal time in each half.
        assert!(
            (chart_slide_ramp(t) + chart_slide_ramp(1.0 - t) - 1.0).abs() < 1e-6,
            "ramp is not symmetric at {t}"
        );
    }
}

#[test]
fn the_slide_ramp_answers_finished_for_a_missing_clock() {
    // The house rule for a shared ramp: an out-of-range or non-finite progress
    // never paints an overshoot and never propagates a NaN into a length.
    for missing in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            chart_slide_ramp(missing),
            1.0,
            "{missing} must read as finished"
        );
    }
    assert_eq!(chart_slide_ramp(-4.0), 0.0);
    assert_eq!(chart_slide_ramp(9.0), 1.0);
}

/// The slope this fold replaced was Iced's hand-written ease-in-out. Its two
/// halves now have one owner; the fixture spells the same arithmetic so a
/// future curve change fails the test. `powi(2)` is not used in the fixture:
/// some targets lower it to a different 1-ULP answer than `x * x`, which
/// would make a platform float-library difference look like a curve change.
#[test]
fn the_slide_ramp_keeps_the_replaced_spelling_bit_for_bit() {
    let replaced = |raw: f32| {
        let t = if raw.is_finite() {
            raw.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if t < 0.5 {
            2.0 * t * t
        } else {
            let x = -2.0 * t + 2.0;
            1.0 - x * x / 2.0
        }
    };
    let duration = CHART_SLIDE_DURATION.as_micros();
    for microsecond in 0..=duration {
        let raw = (microsecond as f64 / duration as f64) as f32;
        assert_eq!(
            chart_slide_ramp(raw).to_bits(),
            replaced(raw).to_bits(),
            "the fold moved the curve at {microsecond} µs"
        );
    }
}

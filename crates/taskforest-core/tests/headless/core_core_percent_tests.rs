//! Headless tests for the percent vocabulary's shared constants.

use super::*;

#[test]
fn max_percent_is_the_ceiling_percent_series_project_against() {
    assert_eq!(MAX_PERCENT, 100.0);
}

#[test]
fn min_percent_is_the_floor_paired_with_that_ceiling() {
    assert_eq!(MIN_PERCENT, 0.0);
}

#[test]
fn the_clamp_answers_like_the_bare_domain_bounds() {
    // The owner must be the same arithmetic the surfaces wrote: a rewrite that
    // changed any answer here would be a visible change, not a refactor.
    let mut changed = 0_usize;
    let mut steps = 0_usize;
    for hundredths in -2_000_i16..=2_000 {
        let value = f32::from(hundredths) / 100.0;
        let expected = value.clamp(0.0, 100.0);
        steps += 1;
        if f32::clamped_percent(value) != expected {
            changed += 1;
        }
        let as_f64 = f64::from(value);
        if f64::clamped_percent(as_f64) != as_f64.clamp(0.0, 100.0) {
            changed += 1;
        }
    }
    assert_eq!(changed, 0, "the owner disagrees with the domain bounds");
    assert_eq!(steps, 4_001);
}

#[test]
fn the_clamp_holds_the_domain_edges_exactly() {
    assert_eq!(f32::clamped_percent(-0.1), 0.0);
    assert_eq!(f32::clamped_percent(0.0), 0.0);
    assert_eq!(f32::clamped_percent(99.9), 99.9);
    assert_eq!(f32::clamped_percent(MAX_PERCENT), MAX_PERCENT);
    assert_eq!(f32::clamped_percent(100.1), MAX_PERCENT);
    assert_eq!(f64::clamped_percent(-1.0), 0.0);
    assert_eq!(f64::clamped_percent(1_000.0), 100.0);
}

#[test]
fn the_clamp_leaves_non_finite_readings_for_the_absence_owner() {
    // The clamp is the domain boundary, spelled exactly as the replaced sites
    // spelled it: an infinite reading moves to the nearer bound, `NaN` comes
    // back unchanged, and neither is silently renamed "0%". Absence stays with
    // the caller that filters finiteness before painting. `NaN` comparisons are
    // rejected by the lint table, so the NaN answer is pinned by identity.
    assert!(f32::clamped_percent(f32::NAN).is_nan());
    assert_eq!(f32::clamped_percent(f32::INFINITY), MAX_PERCENT);
    assert_eq!(f32::clamped_percent(f32::NEG_INFINITY), MIN_PERCENT);
    assert!(f64::clamped_percent(f64::NAN).is_nan());
    assert_eq!(f64::clamped_percent(f64::INFINITY), f64::from(MAX_PERCENT));
    assert_eq!(
        f64::clamped_percent(f64::NEG_INFINITY),
        f64::from(MIN_PERCENT)
    );
}

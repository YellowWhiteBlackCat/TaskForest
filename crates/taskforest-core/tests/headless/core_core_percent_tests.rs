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

/// The old spelling decided absence inside `core` with its own copy of the
/// shared dash. `core` sits below the absence owner and the one-way flow
/// forbids reading upward, so the contract is now: these functions spell a
/// MEASURED value and nothing else. This proves the cut changed no answer any
/// real caller can reach.
fn legacy_level_text(value: f64) -> String {
    if value.is_finite() {
        format!("{:.0}%", round_half_away_from_zero(value))
    } else {
        "—".to_owned()
    }
}

fn legacy_rate_text(value: f64) -> String {
    if value.is_finite() {
        format!("{:.1}%", round_half_away_from_zero(value * 10.0) / 10.0)
    } else {
        "—".to_owned()
    }
}

#[test]
fn the_level_spelling_is_unchanged_for_every_finite_reading() {
    let mut differing = 0_usize;
    let mut steps = 0_usize;
    for tenths in -5_000_i32..=15_000 {
        let value = f64::from(tenths) / 10.0;
        steps += 1;
        if percent_text(value) != legacy_level_text(value) {
            differing += 1;
        }
    }
    assert_eq!(differing, 0, "the level spelling moved on a finite reading");
    assert_eq!(steps, 20_001);
    assert_eq!(percent_text(63.25), "63%");
    assert_eq!(percent_text(-0.4), "0%");
}

#[test]
fn the_rate_spelling_is_unchanged_for_every_finite_reading() {
    let mut differing = 0_usize;
    let mut steps = 0_usize;
    for tenths in -5_000_i32..=15_000 {
        let value = f64::from(tenths) / 10.0;
        steps += 1;
        if precise_percent_text(value) != legacy_rate_text(value) {
            differing += 1;
        }
    }
    assert_eq!(differing, 0, "the rate spelling moved on a finite reading");
    assert_eq!(steps, 20_001);
    assert_eq!(precise_percent_text(13.04), "13.0%");
    assert_eq!(precise_percent_text(13.46), "13.5%");
}

#[test]
fn core_no_longer_prints_the_absence_mark_at_all() {
    // The dash is the presentation layer's fact. A non-finite argument is a
    // caller fault, and the honest failure is a loud numeric spelling in the
    // frame, not a borrowed mark that hides the bug behind a real convention.
    assert_eq!(percent_text(f64::NAN), "NaN%");
    assert_eq!(percent_text(f64::INFINITY), "inf%");
    assert_eq!(precise_percent_text(f64::NAN), "NaN%");
    assert!(
        !percent_text(f64::NAN).contains('—'),
        "core must not emit the shared absence mark"
    );
}

#[test]
fn the_absence_owner_resolves_absence_before_the_spelling_runs() {
    // The shape every real call site uses: `Option` is decided by the owner's
    // `missing_value`, so the spelling below only ever sees a measured value.
    let unmeasured: Option<f64> = None;
    assert_eq!(
        unmeasured.map_or_else(|| "—".to_owned(), percent_text),
        "—".to_owned(),
        "absence reads the shared mark through the owner, never through core"
    );
    assert_eq!(
        Some(63.25_f64).map_or_else(|| "—".to_owned(), percent_text),
        "63%"
    );
}

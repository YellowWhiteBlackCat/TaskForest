//! test-intent: behavior
//!
//! The frontend's interaction curve: this crate names one ease-out constant and
//! one progress helper, and both are wired to the shared owner. The solver's own
//! endpoint and monotonicity laws are proven in `zhuge-ui-core::easing`; these
//! tests prove this frontend rides the curve it claims and never invents timing.

use std::time::Duration;

use super::{INTERACTION_EASE_OUT, eased_progress};
use zhuge_ui_core::easing::CubicBezier;

#[test]
fn the_curve_is_the_named_standard_ease_out() {
    assert_eq!(
        INTERACTION_EASE_OUT,
        CubicBezier::new(0.16, 1.0, 0.3, 1.0),
        "the shared curve is the standard cubic-bezier ease-out"
    );
    let half = INTERACTION_EASE_OUT.sample(0.5);
    assert!(half > 0.5, "an ease-out is past halfway at halfway: {half}");
    assert!(
        (INTERACTION_EASE_OUT.sample(0.25) - 0.25).abs() > 0.05,
        "the curve is not a linear ramp: {}",
        INTERACTION_EASE_OUT.sample(0.25)
    );
}

#[test]
fn the_progress_helper_answers_the_final_value_without_a_time_budget() {
    assert_eq!(
        eased_progress(Duration::from_millis(10), Duration::ZERO),
        1.0,
        "a transition with no duration is a state change, not an animation"
    );
    assert_eq!(
        eased_progress(Duration::from_millis(180), Duration::from_millis(180)),
        1.0,
        "the finished transition answers the end value"
    );
    let half = eased_progress(Duration::from_millis(90), Duration::from_millis(180));
    assert!(
        half > 0.5,
        "the halfway transition is already past halfway on an ease-out: {half}"
    );
}

#[test]
fn the_progress_helper_tracks_the_owners_ratio_and_curve() {
    // The helper is the owner's ratio fed to the owner's sample, nothing more:
    // a request before the start answers the start, and an over-long elapsed
    // time never extrapolates past the end.
    assert_eq!(
        eased_progress(Duration::ZERO, Duration::from_millis(180)),
        0.0
    );
    assert_eq!(
        eased_progress(Duration::from_secs(9), Duration::from_millis(180)),
        1.0
    );
}

//! test-intent: behavior
//!
//! The shared interaction curve: the endpoints are exact, the curve is
//! monotone, and it is an *ease-out* (fast start, gentle settle) rather
//! than a linear ramp. The progress helper answers the final value for a
//! transition with no time budget.

use std::time::Duration;

use super::{EASE_OUT_POINTS, Easing, eased_progress};

#[test]
fn the_curve_answers_its_endpoints_exactly() {
    let curve = Easing::EaseOut;
    assert_eq!(
        curve.evaluate(0.0),
        0.0,
        "the curve starts on its start value"
    );
    assert_eq!(curve.evaluate(1.0), 1.0, "the curve ends on its end value");
    assert_eq!(
        curve.evaluate(-1.0),
        0.0,
        "progress before the start clamps to the start value"
    );
    assert_eq!(
        curve.evaluate(2.0),
        1.0,
        "progress after the end clamps to the end value"
    );
}

#[test]
fn the_curve_is_the_standard_ease_out() {
    assert_eq!(
        EASE_OUT_POINTS,
        [0.16, 1.0, 0.3, 1.0],
        "the shared curve is the standard cubic-bezier ease-out"
    );
    assert_eq!(
        Easing::EaseOut.control_points(),
        EASE_OUT_POINTS,
        "the curve's own points are the shared ones"
    );
    let curve = Easing::EaseOut;
    let half = curve.evaluate(0.5);
    assert!(half > 0.5, "an ease-out is past halfway at halfway: {half}");
    assert!(
        (curve.evaluate(0.25) - 0.25).abs() > 0.05,
        "the curve is not a linear ramp: {}",
        curve.evaluate(0.25)
    );
}

#[test]
fn the_curve_never_goes_backwards() {
    let curve = Easing::EaseOut;
    let mut previous = curve.evaluate(0.0);
    for step in 1..=64 {
        let value = curve.evaluate(step as f32 / 64.0);
        assert!(
            value >= previous,
            "the curve must be monotone: step {step} answered {value} after {previous}"
        );
        previous = value;
    }
}

#[test]
fn the_progress_helper_answers_the_final_value_without_a_time_budget() {
    assert_eq!(
        eased_progress(Easing::EaseOut, Duration::from_millis(10), Duration::ZERO),
        1.0,
        "a transition with no duration is a state change, not an animation"
    );
    assert_eq!(
        eased_progress(
            Easing::EaseOut,
            Duration::from_millis(180),
            Duration::from_millis(180)
        ),
        1.0,
        "the finished transition answers the end value"
    );
    let half = eased_progress(
        Easing::EaseOut,
        Duration::from_millis(90),
        Duration::from_millis(180),
    );
    assert!(
        half > 0.5,
        "the halfway transition is already past halfway on an ease-out: {half}"
    );
}

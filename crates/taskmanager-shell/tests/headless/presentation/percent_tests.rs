use super::*;
use taskforest_core::core::ScalarObservation;
use taskforest_core::core::failure::FailureKind;

/// The percent readout has one owner and one spelling: a missing observation
/// reads the shared dash, a non-finite observation never fabricates a number,
/// and a measured value reads as a whole percent so the four frontends cannot
/// disagree about the same reading.
#[test]
fn percent_readouts_distinguish_unknown_measured_zero_and_precision() {
    assert_eq!(observed_percentage(None::<f32>), "—");
    assert_eq!(observed_percentage(Some(f32::NAN)), "—");
    assert_eq!(observed_percentage(Some(f32::INFINITY)), "—");
    assert_eq!(observed_percentage(Some(0.0)), "0%");
    assert_eq!(observed_percentage(Some(41.6)), "42%");
    assert_eq!(observed_percentage(Some(100.0)), "100%");
    assert_eq!(percent_text(62.5), "63%", "the half rounds away from zero");
}

/// A rate is a different precision from a level: it keeps a tenth of a point
/// because the tenth is the meaningful resolution of the fact.
#[test]
fn rate_readouts_keep_the_tenth() {
    assert_eq!(precise_percent_text(13.0), "13.0%");
    assert_eq!(precise_percent_text(13.04), "13.0%");
    assert_eq!(precise_percent_text(13.46), "13.5%");
    assert_eq!(precise_percent_text(0.0), "0.0%");
    assert_eq!(
        precise_percent_text(-0.04),
        "0.0%",
        "a negative zero never prints a sign"
    );
    assert_eq!(
        percent_text(-0.4),
        "0%",
        "a negative zero never prints a sign"
    );
    assert_eq!(observed_precise_percentage(None::<f32>), "—");
    assert_eq!(observed_precise_percentage(Some(f32::NAN)), "—");
    assert_eq!(observed_precise_percentage(Some(12.34)), "12.3%");
}

/// The suffixed scalar readout shares the same absence rules.
#[test]
fn scalar_readouts_distinguish_unknown_from_measured_zero() {
    assert_eq!(scalar_text(None, " W"), "—");
    assert_eq!(scalar_text(Some(f64::NAN), " W"), "—");
    assert_eq!(scalar_text(Some(0.0), " W"), "0.0 W");
    assert_eq!(scalar_text(Some(12.34), " W"), "12.3 W");
}

/// A percent observation keeps its own availability: the readout must not
/// turn an unobserved percentage into a measured zero.
#[test]
fn an_unobserved_percentage_never_reads_as_zero() {
    let missing: ScalarObservation<f32> =
        ScalarObservation::unavailable(FailureKind::PermissionDenied);
    assert!(missing.current_value().is_none());
    assert_eq!(observed_percentage(missing.current_value().copied()), "—");
}

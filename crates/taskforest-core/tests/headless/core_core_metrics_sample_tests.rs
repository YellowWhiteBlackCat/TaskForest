//! Behavior tests for the shared metric sample vocabulary.

use super::*;

#[test]
fn a_measured_sample_carries_its_measurement_instant() {
    let sample = MetricSample::measured(42.5, 1_700_000_000_000, 1_700_000_001_000);
    assert_eq!(sample.value(), Some(42.5));
    assert_eq!(sample.measured_at_ms(), Some(1_700_000_000_000));
    assert_eq!(sample.completed_at_ms(), 1_700_000_001_000);
    assert!(!sample.is_gap());
}

#[test]
fn a_gap_sample_carries_no_value_and_no_instant() {
    let sample = MetricSample::gap(1_700_000_001_000);
    assert_eq!(sample.value(), None);
    assert_eq!(sample.measured_at_ms(), None);
    assert_eq!(sample.completed_at_ms(), 1_700_000_001_000);
    assert!(sample.is_gap());
}

#[test]
fn the_completion_time_never_replaces_the_measurement_instant() {
    // The acceptance time orders samples; it is not the sampling instant. A
    // consumer that shows when a value was sampled must read the measured
    // instant, so the two must stay distinguishable.
    let sample = MetricSample::measured(1.0, 500, 900);
    assert_ne!(sample.measured_at_ms(), Some(sample.completed_at_ms()));
}

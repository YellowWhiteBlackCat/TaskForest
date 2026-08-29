use super::*;

use crate::core::{FailureKind, ScalarAvailability, ScalarObservation};

fn cpu(value: f32, at: u64) -> ScalarObservation<f32> {
    ScalarObservation::available(value, at)
}

#[test]
fn empty_input_does_not_create_an_aggregate() {
    assert_eq!(aggregate_f32([].iter(), 10), None);
}

#[test]
fn measured_zero_remains_available_zero() {
    let values = [cpu(0.0, 1), cpu(0.0, 1)];
    let aggregate = aggregate_f32(values.iter(), 2).expect("members produce an aggregate");

    assert_eq!(aggregate.availability(), ScalarAvailability::Available);
    assert_eq!(aggregate.current_value(), Some(&0.0));
    assert_eq!(aggregate.member_count(), 2);
    assert_eq!(aggregate.current_member_count(), 2);
    assert_eq!(aggregate.known_member_count(), 2);
}

#[test]
fn all_unknown_stays_unknown_instead_of_becoming_zero() {
    let values = [ScalarObservation::<f32>::default()];
    let aggregate = aggregate_f32(values.iter(), 2).expect("member produces an aggregate");

    assert_eq!(aggregate.availability(), ScalarAvailability::Unknown);
    assert_eq!(aggregate.current_value(), None);
    assert_eq!(aggregate.last_known_value(), None);
    assert_eq!(aggregate.current_member_count(), 0);
    assert_eq!(aggregate.known_member_count(), 0);
}

#[test]
fn mixed_current_and_unavailable_is_partial_with_coverage_counts() {
    let values = [
        cpu(3.0, 1),
        ScalarObservation::unavailable(FailureKind::PermissionDenied),
    ];
    let aggregate = aggregate_f32(values.iter(), 2).expect("members produce an aggregate");

    assert_eq!(
        aggregate.availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(aggregate.current_value(), Some(&3.0));
    assert_eq!(aggregate.member_count(), 2);
    assert_eq!(aggregate.current_member_count(), 1);
    assert_eq!(aggregate.known_member_count(), 1);
}

#[test]
fn mixed_current_and_unknown_is_partial_without_inventing_a_source_reason() {
    let values = [cpu(3.0, 1), ScalarObservation::<f32>::default()];
    let aggregate = aggregate_f32(values.iter(), 2).expect("members produce an aggregate");

    assert_eq!(
        aggregate.availability(),
        ScalarAvailability::Partial(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(aggregate.current_value(), Some(&3.0));
    assert_eq!(aggregate.current_member_count(), 1);
    assert_eq!(aggregate.known_member_count(), 1);
}

#[test]
fn stale_members_are_not_exposed_as_current() {
    let values = [ScalarObservation::stale(8_u64, 4, FailureKind::TimedOut)];
    let aggregate = aggregate_u64(values.iter(), 10).expect("member produces an aggregate");

    assert_eq!(
        aggregate.availability(),
        ScalarAvailability::Stale(FailureKind::TimedOut)
    );
    assert_eq!(aggregate.current_value(), None);
    assert_eq!(aggregate.last_known_value(), Some(&8));
    assert_eq!(aggregate.known_member_count(), 1);
}

#[test]
fn all_unavailable_has_no_current_or_last_known_value() {
    let values = [ScalarObservation::<u64>::unavailable(
        FailureKind::Unsupported,
    )];
    let aggregate = aggregate_u64(values.iter(), 10).expect("member produces an aggregate");

    assert_eq!(
        aggregate.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(aggregate.current_value(), None);
    assert_eq!(aggregate.last_known_value(), None);
}

#[test]
fn u64_aggregation_saturates() {
    let values = [
        ScalarObservation::available(u64::MAX, 1),
        ScalarObservation::available(1, 1),
    ];
    let aggregate = aggregate_u64(values.iter(), 2).expect("members produce an aggregate");

    assert_eq!(aggregate.current_value(), Some(&u64::MAX));
}

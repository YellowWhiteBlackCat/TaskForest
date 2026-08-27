use super::*;
use taskmanager_core::ScalarAvailability;

#[test]
fn observed_zero_is_a_current_success() {
    let mut counter = CounterAggregate::default();
    counter.observe_sum(Ok(0));
    let snapshot = assemble_snapshot(
        counter.finish(50),
        ScalarObservation::unavailable(FailureKind::Unsupported),
        50,
    );

    assert_eq!(snapshot.current_core_events(), Some(0));
    assert_eq!(
        snapshot.core_events_observation().availability(),
        ScalarAvailability::Available
    );
    assert_eq!(
        snapshot.core_events_observation().last_success_ms(),
        Some(50)
    );
}

#[test]
fn denied_counter_is_distinct_from_unsupported_counter() {
    let denied = CounterAggregate {
        failure: Some(FailureKind::PermissionDenied),
        ..Default::default()
    }
    .finish(50);
    let unsupported = CounterAggregate::default().finish(50);

    assert_eq!(
        denied.availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(
        unsupported.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn incomplete_current_aggregate_is_typed_partial() {
    let mut counter = CounterAggregate::default();
    counter.observe_sum(Ok(3));
    counter.observe_sum(Err(FailureKind::TemporarilyUnavailable));
    let observation = counter.finish(50);

    assert_eq!(observation.current_value(), Some(&3));
    assert_eq!(
        observation.availability(),
        ScalarAvailability::Partial(FailureKind::TemporarilyUnavailable)
    );
}

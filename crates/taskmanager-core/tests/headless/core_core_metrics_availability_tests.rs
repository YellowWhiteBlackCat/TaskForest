use super::*;

#[test]
fn observed_zero_is_not_unknown() {
    let observed = ScalarObservation::available(0_u64, 42);

    assert_eq!(observed.current_value(), Some(&0));
    assert_eq!(observed.availability(), ScalarAvailability::Available);
    assert_eq!(observed.last_success_ms(), Some(42));

    let decoded: ScalarObservation<u64> =
        serde_json::from_value(serde_json::to_value(observed).expect("serialize observed zero"))
            .expect("valid observed zero must round trip");
    assert_eq!(decoded.current_value(), Some(&0));
}

#[test]
fn failure_retains_last_success_but_not_a_current_value() {
    let denied =
        ScalarObservation::available(7_u64, 42).transition_failure(FailureKind::PermissionDenied);

    assert_eq!(
        denied.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(denied.current_value(), None);
    assert_eq!(denied.last_known_value(), Some(&7));
    assert_eq!(denied.last_success_ms(), Some(42));
}

#[test]
fn first_failure_cannot_fabricate_stale_data() {
    let unavailable =
        ScalarObservation::<u64>::default().transition_failure(FailureKind::TemporarilyUnavailable);

    assert_eq!(
        unavailable.availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(unavailable.last_known_value(), None);
    assert_eq!(unavailable.last_success_ms(), None);
}

#[test]
fn unavailable_refresh_keeps_prior_value_only_as_stale() {
    let previous = ScalarObservation::available(11_u64, 100);
    let current = ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
        .retain_previous(previous);

    assert_eq!(
        current.availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(current.current_value(), None);
    assert_eq!(current.last_known_value(), Some(&11));
    assert_eq!(current.last_success_ms(), Some(100));
}

#[test]
fn optional_observation_serialization_keeps_absent_and_not_applicable_distinct() {
    let absent = OptionalObservation::<String>::absent(10);
    let not_applicable = OptionalObservation::<String>::not_applicable(20);

    let absent_json = serde_json::to_string(&absent).expect("absent state should serialize");
    let not_applicable_json =
        serde_json::to_string(&not_applicable).expect("not-applicable state should serialize");
    let decoded_absent: OptionalObservation<String> =
        serde_json::from_str(&absent_json).expect("absent state should deserialize");
    let decoded_not_applicable: OptionalObservation<String> =
        serde_json::from_str(&not_applicable_json)
            .expect("not-applicable state should deserialize");

    assert!(decoded_absent.is_current_absent());
    assert!(decoded_not_applicable.is_current_not_applicable());
    assert_ne!(absent_json, not_applicable_json);
}

#[test]
fn optional_failure_retains_confirmed_absence_as_stale() {
    let stale = OptionalObservation::<String>::absent(42).transition_failure(FailureKind::TimedOut);

    assert_eq!(stale.last_known_state(), &OptionalObservationState::Absent);
    assert_eq!(
        stale.availability(),
        ScalarAvailability::Stale(FailureKind::TimedOut)
    );
    assert_eq!(stale.last_success_ms(), Some(42));
}

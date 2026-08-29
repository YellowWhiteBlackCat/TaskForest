use super::*;

use crate::core::{FailureKind, ProcessScalarObservations, ScalarObservation};

#[test]
fn live_key_rejects_zero_pid_and_start_token() {
    assert_eq!(ProcessLiveKey::new(0, 7), None);
    assert_eq!(ProcessLiveKey::new(7, 0), None);
}

#[test]
fn live_key_round_trips_and_distinguishes_pid_reuse() {
    let first = ProcessLiveKey::new(42, 100).expect("non-zero identity");
    let reused = ProcessLiveKey::new(42, 101).expect("non-zero identity");

    assert_eq!(first.pid(), 42);
    assert_eq!(first.start_token(), 100);
    assert_eq!(first.into_identity().pid, 42);
    assert!(first.matches(first.into_identity()));
    assert_ne!(first, reused);
    assert!(!first.matches(reused.into_identity()));
}

#[test]
fn process_live_key_requires_a_current_start_token() {
    let unknown = ProcessItem::new(42, "worker");
    assert_eq!(unknown.current_live_key(), None);

    let observed =
        ProcessItem::new(42, "worker").with_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(100, 1),
            ..ProcessScalarObservations::default()
        });
    assert_eq!(observed.current_live_key(), ProcessLiveKey::new(42, 100));

    let stale = observed.with_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::stale(100, 1, FailureKind::TemporarilyUnavailable),
        ..ProcessScalarObservations::default()
    });
    assert_eq!(stale.current_live_key(), None);
}

#[test]
fn frozen_control_identity_can_be_projected_to_a_live_key_without_losing_authority() {
    let frozen = FrozenProcessIdentity::from_authoritative_parts(42, "worker", 7, 100)
        .expect("non-zero exact identity");
    let key = frozen.live_key().expect("frozen token is authoritative");

    assert_eq!(
        key,
        ProcessLiveKey::new(42, 100).expect("non-zero live key")
    );
    assert_eq!(frozen.authoritative_start_token(), Some(100));
}

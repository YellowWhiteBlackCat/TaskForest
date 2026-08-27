use super::*;

#[test]
fn counter_transition_distinguishes_first_gap_zero_rollbacks_and_failures() {
    let mut counter = CumulativeCounter::default();
    assert_eq!(
        counter.observe(Ok(100), 1_000, FailureKind::TemporarilyUnavailable),
        CounterDelta::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        counter.observe(Ok(100), 2_000, FailureKind::TemporarilyUnavailable),
        CounterDelta::Available {
            value: 0,
            elapsed_ms: 1_000
        }
    );
    assert_eq!(
        counter.observe(Ok(150), 2_000, FailureKind::TemporarilyUnavailable),
        CounterDelta::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        counter.observe(Ok(160), 1_999, FailureKind::TemporarilyUnavailable),
        CounterDelta::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        counter.observe(Ok(10), 3_000, FailureKind::TemporarilyUnavailable),
        CounterDelta::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        counter.observe(
            Err(FailureKind::PermissionDenied),
            4_000,
            FailureKind::TemporarilyUnavailable
        ),
        CounterDelta::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(
        counter.observe(Ok(20), 5_000, FailureKind::IdentityChanged),
        CounterDelta::Unavailable(FailureKind::IdentityChanged)
    );
}

#[test]
fn per_second_is_overflow_safe_and_preserves_measured_zero() {
    assert_eq!(
        CounterDelta::Available {
            value: 0,
            elapsed_ms: 500
        }
        .per_second(9),
        ScalarObservation::available(0, 9)
    );
    assert_eq!(
        CounterDelta::Available {
            value: 2_048,
            elapsed_ms: 2_000
        }
        .per_second(10),
        ScalarObservation::available(1_024, 10)
    );
    assert_eq!(
        CounterDelta::Available {
            value: u64::MAX,
            elapsed_ms: 1
        }
        .per_second(10),
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
}

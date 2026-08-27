use taskmanager_core::ScalarAvailability;

use super::*;

fn observe(
    state: &mut ProcessRateState,
    token: u64,
    at: u64,
    cpu: u64,
    read: u64,
    write: u64,
) -> ProcessRateObservations {
    state.observe(ProcessRateInput {
        pid: 7,
        start_token: token,
        observed_at_ms: at,
        clock_ticks: &Ok(100),
        cpu_ticks: Ok(cpu),
        disk_read_bytes: Ok(read),
        disk_write_bytes: Ok(write),
    })
}

#[test]
fn first_sample_is_gap_and_unchanged_second_sample_is_current_zero() {
    let mut state = ProcessRateState::default();
    let first = observe(&mut state, 99, 1_000, 10, 20, 30);
    assert_eq!(
        first.cpu_percentage.availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(first.disk_read_bytes_per_sec.current_value(), None);

    let idle = observe(&mut state, 99, 2_000, 10, 20, 30);
    assert_eq!(idle.cpu_percentage.current_value(), Some(&0.0));
    assert_eq!(idle.disk_read_bytes_per_sec.current_value(), Some(&0));
    assert_eq!(idle.disk_write_bytes_per_sec.current_value(), Some(&0));
}

#[test]
fn rates_use_elapsed_time_and_cpu_clock_frequency() {
    let mut state = ProcessRateState::default();
    observe(&mut state, 99, 1_000, 10, 20, 30);
    let current = observe(&mut state, 99, 1_500, 60, 1_020, 2_030);

    assert_eq!(current.cpu_percentage.current_value(), Some(&100.0));
    assert_eq!(
        current.disk_read_bytes_per_sec.current_value(),
        Some(&2_000)
    );
    assert_eq!(
        current.disk_write_bytes_per_sec.current_value(),
        Some(&4_000)
    );
}

#[test]
fn pid_reuse_and_counter_rollback_never_inherit_or_wrap() {
    let mut state = ProcessRateState::default();
    observe(&mut state, 99, 1_000, 100, 1_000, 2_000);
    let reused = observe(&mut state, 100, 2_000, 1, 2, 3);
    assert_eq!(
        reused.disk_read_bytes_per_sec.availability(),
        ScalarAvailability::Unavailable(FailureKind::IdentityChanged)
    );

    let current = observe(&mut state, 100, 3_000, 10, 20, 30);
    assert_eq!(current.disk_read_bytes_per_sec.current_value(), Some(&18));
    let rollback = observe(&mut state, 100, 4_000, 5, 10, 15);
    assert_eq!(
        rollback.cpu_percentage.availability(),
        ScalarAvailability::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        rollback.disk_write_bytes_per_sec.availability(),
        ScalarAvailability::Unavailable(FailureKind::IdentityChanged)
    );
}

#[test]
fn failed_counter_clears_only_its_baseline_and_recovery_starts_with_gap() {
    let mut state = ProcessRateState::default();
    observe(&mut state, 99, 1_000, 10, 20, 30);
    let failed = state.observe(ProcessRateInput {
        pid: 7,
        start_token: 99,
        observed_at_ms: 2_000,
        clock_ticks: &Ok(100),
        cpu_ticks: Ok(20),
        disk_read_bytes: Err(FailureKind::PermissionDenied),
        disk_write_bytes: Ok(40),
    });
    assert_eq!(
        failed.disk_read_bytes_per_sec.availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(failed.disk_write_bytes_per_sec.current_value(), Some(&10));

    let recovered = observe(&mut state, 99, 3_000, 30, 50, 50);
    assert_eq!(
        recovered.disk_read_bytes_per_sec.availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        recovered.disk_write_bytes_per_sec.current_value(),
        Some(&10)
    );
}

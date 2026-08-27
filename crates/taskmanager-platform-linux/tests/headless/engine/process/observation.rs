use taskmanager_core::ScalarAvailability;

use super::*;

fn stat(start_ticks: u64, user_ticks: u64, system_ticks: u64) -> ProcStatFields {
    ProcStatFields {
        threads: 0,
        start_ticks,
        user_ticks,
        system_ticks,
        nice: 0,
    }
}

fn observe_results(
    stat: Result<ProcStatFields, FailureKind>,
    fds: Result<FdCount, FailureKind>,
    clock_ticks: Result<u64, FailureKind>,
    observed_at_ms: u64,
    previous: Option<&ProcessItem>,
    rates: &mut ProcessRateState,
) -> (ProcessScalarObservations, ProcessScalarEvidence) {
    observe_with_fd_opt(
        stat,
        Some(fds),
        clock_ticks,
        observed_at_ms,
        previous,
        rates,
    )
}

fn observe_with_fd_opt(
    stat: Result<ProcStatFields, FailureKind>,
    fds: Option<Result<FdCount, FailureKind>>,
    clock_ticks: Result<u64, FailureKind>,
    observed_at_ms: u64,
    previous: Option<&ProcessItem>,
    rates: &mut ProcessRateState,
) -> (ProcessScalarObservations, ProcessScalarEvidence) {
    let confirmation = stat.map(|stat| stat.start_ticks);
    observations_from_results(
        ProcessScalarInputs {
            pid: 7,
            stat,
            identity_confirmation: confirmation,
            fds,
            memory: Ok(0),
            io: Ok(ProcIoFields {
                read_bytes: Ok(0),
                write_bytes: Ok(0),
            }),
        },
        ProcessObservationContext {
            boot_time: &Ok(1_720_000_000),
            clock_ticks: &clock_ticks,
            observed_at_ms,
            previous,
        },
        rates,
    )
}

#[test]
fn measured_zero_is_current_for_stat_and_fd_scalars() {
    let (observations, evidence) = observe_results(
        Ok(stat(0, 0, 0)),
        Ok(FdCount {
            value: 0,
            partial_failure: None,
        }),
        Ok(250),
        42,
        None,
        &mut ProcessRateState::default(),
    );

    assert_eq!(observations.memory_bytes.current_value(), Some(&0));
    assert_eq!(observations.disk_read_bytes_total.current_value(), Some(&0));
    assert_eq!(observations.threads.current_value(), Some(&0));
    assert_eq!(observations.cpu_time_secs.current_value(), Some(&0));
    assert_eq!(observations.fds.current_value(), Some(&0));
    assert_eq!(observations.nice.current_value(), Some(&0));
    assert_eq!(evidence.stat, SourceOutcome::Available);
    assert_eq!(evidence.fds, SourceOutcome::Available);
}

#[test]
fn one_clock_failure_invalidates_start_and_cpu_without_poisoning_stat_siblings() {
    let (observations, _) = observe_results(
        Ok(stat(500, 20, 5)),
        Ok(FdCount {
            value: 3,
            partial_failure: None,
        }),
        Err(FailureKind::Unsupported),
        42,
        None,
        &mut ProcessRateState::default(),
    );

    assert_eq!(
        observations.start_time_secs.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        observations.cpu_time_secs.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(observations.threads.current_value(), Some(&0));
    assert_eq!(observations.nice.current_value(), Some(&0));
}

#[test]
fn fd_failure_becomes_stale_only_when_current_stat_proves_same_identity() {
    let previous = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(7)
        .name("worker".to_owned())
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(600, 10),
            threads: ScalarObservation::available(8, 10),
            start_time_secs: ScalarObservation::available(1_720_000_006, 10),
            cpu_time_secs: ScalarObservation::available(2, 10),
            fds: ScalarObservation::available(12, 10),
            nice: ScalarObservation::available(0, 10),
            ..ProcessScalarObservations::default()
        })
        .build();
    let mut rates = ProcessRateState::default();
    let (failed, _) = observe_results(
        Ok(stat(600, 300, 200)),
        Err(FailureKind::PermissionDenied),
        Ok(100),
        20,
        Some(&previous),
        &mut rates,
    );
    assert_eq!(
        failed.fds.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(failed.fds.last_known_value(), Some(&12));
    assert_eq!(failed.fds.current_value(), None);

    let (recovered, _) = observe_results(
        Ok(stat(600, 400, 200)),
        Ok(FdCount {
            value: 13,
            partial_failure: None,
        }),
        Ok(100),
        30,
        Some(
            &taskmanager_test_support::ProcessItemFixtureBuilder::from_item(previous)
                .scalar_observations(failed)
                .build(),
        ),
        &mut rates,
    );
    assert_eq!(recovered.fds.current_value(), Some(&13));
    assert_eq!(recovered.fds.last_success_ms(), Some(30));
}

#[test]
fn exact_start_token_change_blocks_all_stale_inheritance() {
    let previous = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(7)
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(600, 10),
            memory_bytes: ScalarObservation::available(4096, 10),
            disk_read_bytes_total: ScalarObservation::available(100, 10),
            fds: ScalarObservation::available(12, 10),
            ..ProcessScalarObservations::default()
        })
        .build();
    let mut rates = ProcessRateState::default();
    let (current, _) = observations_from_results(
        ProcessScalarInputs {
            pid: 7,
            stat: Ok(stat(601, 0, 0)),
            identity_confirmation: Ok(601),
            fds: Some(Err(FailureKind::PermissionDenied)),
            memory: Err(FailureKind::PermissionDenied),
            io: Err(FailureKind::PermissionDenied),
        },
        ProcessObservationContext {
            boot_time: &Ok(1_720_000_000),
            clock_ticks: &Ok(100),
            observed_at_ms: 20,
            previous: Some(&previous),
        },
        &mut rates,
    );

    assert_eq!(
        current.memory_bytes.availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(current.memory_bytes.last_known_value(), None);
    assert_eq!(current.fds.last_known_value(), None);
}

#[test]
fn identity_race_invalidates_other_successful_proc_fields() {
    let (observations, evidence) = observations_from_results(
        ProcessScalarInputs {
            pid: 7,
            stat: Ok(stat(600, 0, 0)),
            identity_confirmation: Ok(601),
            fds: Some(Ok(FdCount {
                value: 2,
                partial_failure: None,
            })),
            memory: Ok(4096),
            io: Ok(ProcIoFields {
                read_bytes: Ok(10),
                write_bytes: Ok(20),
            }),
        },
        ProcessObservationContext {
            boot_time: &Ok(1_720_000_000),
            clock_ticks: &Ok(100),
            observed_at_ms: 20,
            previous: None,
        },
        &mut ProcessRateState::default(),
    );

    assert_eq!(
        observations.start_token.availability(),
        ScalarAvailability::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(observations.memory_bytes.current_value(), None);
    assert_eq!(
        evidence.stat,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        evidence.fds,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        evidence.memory,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        evidence.io,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        evidence.rates,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
}

#[test]
fn deferred_fd_tick_reuses_previous_value_for_unchanged_identity() {
    // fd count is sampled at lower cadence (FD_COUNT_REFRESH_EVERY_N_TICKS).
    // A deferred tick with an unchanged identity must reuse the previous fd
    // count as a Stale value (current_value hidden, last_known preserved).
    // The fd SOURCE outcome stays Available so the aggregate fd source
    // status does not toggle Available/Empty across the decimation cadence;
    // the value's Stale availability already conveys "not freshly read".
    let previous = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(7)
        .name("worker".to_owned())
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(600, 10),
            fds: ScalarObservation::available(42, 10),
            ..ProcessScalarObservations::default()
        })
        .build();
    let (observations, evidence) = observe_with_fd_opt(
        Ok(stat(600, 0, 0)),
        None,
        Ok(100),
        30,
        Some(&previous),
        &mut ProcessRateState::default(),
    );

    assert_eq!(
        observations.fds.availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable),
        "a deferred tick keeps the prior fd value as Stale, not fresh"
    );
    assert_eq!(observations.fds.last_known_value(), Some(&42));
    assert_eq!(
        observations.fds.current_value(),
        None,
        "the reused count must not pose as a fresh measurement"
    );
    assert_ne!(
        *observations.fds.last_known_value().unwrap_or(&0),
        0,
        "the reused count is the real previous value, not a fabricated 0"
    );
    assert_eq!(
        evidence.fds,
        SourceOutcome::Available,
        "a deferred tick retains the last full-tick source outcome (no Available/Empty toggle)"
    );
}

#[test]
fn deferred_fd_tick_without_previous_is_typed_unavailable_not_zero() {
    // First time we see a pid on a deferred tick: no prior fd value to
    // reuse, so the column is an honest typed Unavailable — never 0.
    let (observations, evidence) = observe_with_fd_opt(
        Ok(stat(600, 0, 0)),
        None,
        Ok(100),
        30,
        None,
        &mut ProcessRateState::default(),
    );

    assert_eq!(
        observations.fds.availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        observations.fds.current_value(),
        None,
        "no prior value means no fabricated 0 fd count"
    );
    assert_eq!(observations.fds.last_known_value(), None);
    assert_eq!(evidence.fds, SourceOutcome::Empty);
}

#[test]
fn full_fd_tick_reads_fresh_count_after_a_deferred_tick() {
    // The cadence is full → deferred → full. The second full tick must read
    // a fresh fd count (not the retained one) and report it as current.
    let previous = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(7)
        .name("worker".to_owned())
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(600, 10),
            fds: ScalarObservation::available(42, 10),
            ..ProcessScalarObservations::default()
        })
        .build();
    let (observations, evidence) = observe_with_fd_opt(
        Ok(stat(600, 0, 0)),
        Some(Ok(FdCount {
            value: 57,
            partial_failure: None,
        })),
        Ok(100),
        40,
        Some(&previous),
        &mut ProcessRateState::default(),
    );

    assert_eq!(observations.fds.current_value(), Some(&57));
    assert_eq!(observations.fds.last_success_ms(), Some(40));
    assert_eq!(evidence.fds, SourceOutcome::Available);
}

use super::*;
use taskmanager_core::ScalarAvailability;

fn source(outcome: SourceOutcome, item_count: usize) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("fixture.source"),
        outcome,
        item_count,
    }
}

#[test]
fn sysinfo_frequency_zero_is_unknown_but_positive_values_are_observed() {
    assert_eq!(observed_sysinfo_frequency_mhz(0), None);
    assert_eq!(observed_sysinfo_frequency_mhz(3_200), Some(3_200));
}

#[test]
fn scalar_source_mapping_preserves_zero_partial_and_failure_truth() {
    let available = source(SourceOutcome::Available, 1);
    let partial = source(SourceOutcome::Partial(FailureKind::PermissionDenied), 1);
    let unavailable = source(SourceOutcome::Unavailable(FailureKind::TimedOut), 0);

    assert_eq!(
        scalar_from_source(Some(0_u64), &available, 10),
        ScalarObservation::available(0, 10)
    );
    assert_eq!(
        scalar_from_source(Some(0_u64), &partial, 20),
        ScalarObservation::partial(0, 20, FailureKind::PermissionDenied)
    );
    assert_eq!(
        scalar_from_source::<u64>(None, &unavailable, 30),
        ScalarObservation::unavailable(FailureKind::TimedOut)
    );
}

#[test]
fn scalar_group_mapping_distinguishes_current_empty_partial_and_unavailable() {
    let available = source(SourceOutcome::Available, 1);
    let empty = source(SourceOutcome::Empty, 0);
    let partial = source(SourceOutcome::Partial(FailureKind::PermissionDenied), 1);
    let unavailable = source(
        SourceOutcome::Unavailable(FailureKind::TemporarilyUnavailable),
        0,
    );

    let observed = vec![ScalarObservation::available(0_u64, 10)];
    assert_eq!(
        scalar_group_from_source(observed.clone(), &available, 10).availability(),
        ScalarAvailability::Available
    );
    let confirmed_empty = scalar_group_from_source::<u64>(Vec::new(), &empty, 20);
    assert_eq!(confirmed_empty.current_observations(), Some(&[][..]));
    assert_eq!(confirmed_empty.last_success_ms(), Some(20));
    assert_eq!(
        scalar_group_from_source(observed.clone(), &partial, 30).availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    let failed = scalar_group_from_source(observed, &unavailable, 40);
    assert_eq!(failed.current_observations(), None);
    assert_eq!(failed.last_known_observations().len(), 1);
    assert_eq!(
        failed.availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn scalar_group_failure_and_recovery_keep_freshness_truth() {
    let available = SourceStatus {
        provider: ProviderId::borrowed("fixture.cpufreq"),
        outcome: SourceOutcome::Available,
        item_count: 1,
    };
    let failed = SourceStatus {
        provider: ProviderId::borrowed("fixture.cpufreq"),
        outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        item_count: 0,
    };
    let previous = scalar_group_from_source(
        vec![ScalarObservation::available(3_200_u64, 10)],
        &available,
        10,
    );
    let stale = scalar_group_from_source(
        vec![ScalarObservation::unavailable(
            FailureKind::PermissionDenied,
        )],
        &failed,
        20,
    )
    .retain_previous(previous);

    assert_eq!(stale.current_observations(), None);
    assert_eq!(stale.last_success_ms(), Some(10));
    assert_eq!(
        stale.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(
        stale.last_known_observations()[0].availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );

    let recovered = scalar_group_from_source(
        vec![ScalarObservation::available(3_400_u64, 30)],
        &available,
        30,
    )
    .retain_previous(stale);
    assert_eq!(recovered.availability(), ScalarAvailability::Available);
    assert_eq!(recovered.last_success_ms(), Some(30));
    assert_eq!(
        recovered
            .current_observations()
            .expect("recovered group is current")[0]
            .current_value(),
        Some(&3_400)
    );
}

#[test]
fn optional_source_mapping_keeps_zero_absence_partial_and_failure_distinct() {
    let available = source(SourceOutcome::Available, 1);
    let empty = source(SourceOutcome::Empty, 0);
    let partial = source(SourceOutcome::Partial(FailureKind::PermissionDenied), 1);
    let unavailable = source(SourceOutcome::Unavailable(FailureKind::TimedOut), 0);

    assert_eq!(
        optional_from_source(Some(0_u64), &available, 10),
        OptionalObservation::present(0, 10)
    );
    assert!(optional_from_source::<u64>(None, &empty, 20).is_current_absent());
    assert_eq!(
        optional_from_source(Some(0_u64), &partial, 30),
        OptionalObservation::partial_present(0, 30, FailureKind::PermissionDenied)
    );
    assert_eq!(
        optional_from_source::<u64>(None, &unavailable, 40),
        OptionalObservation::unavailable(FailureKind::TimedOut)
    );
}

#[test]
fn rapl_first_tick_and_recovery_are_gaps_but_idle_delta_is_current_zero() {
    let source = SourceStatus {
        provider: ProviderId::borrowed("fixture.rapl"),
        outcome: SourceOutcome::Available,
        item_count: 1,
    };
    let failed_source = SourceStatus {
        provider: ProviderId::borrowed("fixture.rapl"),
        outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        item_count: 0,
    };
    let first_at = Instant::now();
    let mut previous = None;

    let first = observe_rapl_power(Some(1_000), &mut previous, 10_000, first_at, 10, &source);
    assert_eq!(
        first.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );

    let idle = observe_rapl_power(
        Some(1_000),
        &mut previous,
        10_000,
        first_at + std::time::Duration::from_secs(1),
        20,
        &source,
    );
    assert_eq!(idle.current_value(), Some(&0.0));

    let failed = observe_rapl_power(
        None,
        &mut previous,
        10_000,
        first_at + std::time::Duration::from_secs(2),
        30,
        &failed_source,
    );
    assert_eq!(
        failed.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert!(
        previous.is_none(),
        "failed reads must reset the delta baseline"
    );

    let recovered = observe_rapl_power(
        Some(2_000),
        &mut previous,
        10_000,
        first_at + std::time::Duration::from_secs(3),
        40,
        &source,
    );
    assert_eq!(
        recovered.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn memory_delta_rate_preserves_first_tick_gap_and_measured_zero() {
    let first_at = Instant::now();
    let (first_rate, previous) = delta_rate(None, 1_024, first_at);
    assert_eq!(first_rate, None);

    let (second_rate, _) = delta_rate(
        previous,
        1_024,
        first_at + std::time::Duration::from_secs(1),
    );
    assert_eq!(second_rate, Some(0.0));
}

#[test]
fn usage_group_gates_phantom_slots_into_typed_gaps() {
    // One NaN and one impossible >100% phantom among healthy cores: the
    // phantom slots become typed gaps, the group degrades to an honest
    // Partial, and the healthy slots stay current.
    let poisoned = usage_group_from_percentages(&[50.0, f32::NAN, -1.0, 100.0], 10);
    assert_eq!(
        poisoned.availability(),
        ScalarAvailability::Partial(FailureKind::ProviderFault)
    );
    let slots = poisoned
        .current_observations()
        .expect("partial keeps healthy slots current");
    assert_eq!(slots[0], ScalarObservation::available(50.0, 10));
    assert_eq!(
        slots[1],
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        slots[2],
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(slots[3], ScalarObservation::available(100.0, 10));

    // A fully healthy refresh — including measured zeros and saturation
    // rounding inside the tolerance — stays Available.
    let healthy = usage_group_from_percentages(&[0.0, 100.0, 100.2], 20);
    assert_eq!(healthy.availability(), ScalarAvailability::Available);
    assert_eq!(
        healthy.current_observations().expect("healthy group")[2],
        ScalarObservation::available(100.0, 20),
        "saturation rounding clamps instead of spiking"
    );
}

#[test]
fn cpu_observation_always_carries_all_granular_sources() {
    let system = System::new_all();
    let mut previous_rapl = None;
    let observation = collect_cpu(
        &system,
        &mut previous_rapl,
        (None, None, None),
        1u64 << 32,
        Instant::now(),
        100,
    );
    let providers: Vec<&str> = observation
        .sources
        .iter()
        .map(|source| source.provider.as_str())
        .collect();
    assert_eq!(
        providers,
        [
            "linux.telemetry.cpu.cpufreq",
            "linux.telemetry.cpu.hwmon-temperature",
            "linux.telemetry.cpu.rapl",
            "linux.telemetry.cpu.sysinfo",
        ]
    );
    assert!(
        observation
            .value
            .scalar_observations()
            .global_usage_pct
            .availability()
            .is_current(),
        "sysinfo utilization remains current even if physical topology is partial"
    );
}

#[test]
fn memory_observation_always_carries_all_granular_sources() {
    let system = System::new_all();
    let mut previous_used = None;
    let first_at = Instant::now();
    let observation = collect_memory(&system, &mut previous_used, first_at, 100);
    let providers: Vec<&str> = observation
        .sources
        .iter()
        .map(|source| source.provider.as_str())
        .collect();
    assert_eq!(
        providers,
        [
            "linux.telemetry.memory.dmi",
            "linux.telemetry.memory.proc-meminfo",
            "linux.telemetry.memory.sysinfo",
            "linux.telemetry.memory.zram-zswap",
        ]
    );
    assert_eq!(observation.value.current_hardware_reserved_bytes(), None);
    assert_eq!(
        observation
            .value
            .optional_observations()
            .hardware_reserved_bytes
            .availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        observation
            .value
            .optional_observations()
            .compression
            .compressed_memory_used_bytes
            .availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(observation.value.current_used_rate_mib_per_sec(), None);
    assert_eq!(
        observation
            .value
            .scalar_observations()
            .total_bytes
            .current_value(),
        Some(&system.total_memory())
    );
    assert_eq!(
        observation
            .value
            .scalar_observations()
            .used_rate_mib_per_sec
            .availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );

    let second = collect_memory(
        &system,
        &mut previous_used,
        first_at + std::time::Duration::from_secs(1),
        200,
    );
    assert_eq!(
        second.value.current_used_rate_mib_per_sec(),
        Some(0.0),
        "an unchanged second sample is a measured zero, not unknown"
    );
    assert_eq!(
        second
            .value
            .scalar_observations()
            .used_rate_mib_per_sec
            .last_success_ms(),
        Some(200)
    );
}

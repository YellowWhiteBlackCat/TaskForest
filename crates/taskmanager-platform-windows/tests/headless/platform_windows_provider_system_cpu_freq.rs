use super::*;

#[test]
fn cpu_factory_picks_group_availability_by_live_slot_coverage() {
    // The wire validator (core crate) requires an Available group to
    // contain ONLY Available items. This locks in: full coverage ->
    // Available, partial -> Partial (mixed Some/None slots, wire-safe),
    // none -> Unavailable. The scalar max-freq mirrors coverage.
    let usages = vec![10.0_f32, 20.0, 30.0, 40.0];

    let full = CpuScalarObservationFactory::build(
        usages.clone(),
        &[Some(2400_u64), Some(2500), Some(2600), Some(2700)],
        Some(5_000),
        1_000,
        true,
    );
    assert_eq!(
        full.per_core_frequency_group.availability(),
        taskmanager_core::ScalarAvailability::Available
    );
    assert_eq!(full.frequency_mhz.current_value().copied(), Some(2700));
    assert_eq!(full.max_frequency_mhz.current_value().copied(), Some(5_000));
    // Every slot in an Available group must itself be Available.
    assert!(
        full.per_core_frequency_group
            .last_known_observations()
            .iter()
            .all(|obs| obs.availability() == taskmanager_core::ScalarAvailability::Available)
    );

    let partial = CpuScalarObservationFactory::build(
        usages.clone(),
        &[Some(2400_u64), None, Some(2600), Some(2700)],
        None,
        1_000,
        true,
    );
    assert!(matches!(
        partial.per_core_frequency_group.availability(),
        taskmanager_core::ScalarAvailability::Partial(_)
    ));
    assert_eq!(partial.frequency_mhz.current_value().copied(), Some(2700));

    // No native live-frequency source -> Unavailable, max-freq scalar also
    // Unavailable.
    let none =
        CpuScalarObservationFactory::build(usages, &[None, None, None, None], None, 1_000, true);
    assert_eq!(
        none.per_core_frequency_group.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(none.frequency_mhz.current_value(), None);
}

#[test]
fn live_win_cpu_provider_refresh() {
    let mut provider = WinCpuTelemetryProvider::new();
    // The first refresh sits inside sysinfo's minimum sampling window after
    // the construction-time baseline, so usage must surface as typed
    // unavailable — never the fabricated full-load reading — while the
    // instant facts (frequency, topology) stay available.
    let early = provider
        .refresh(1_000)
        .expect("refresh inside the priming window still reports typed state");
    let early_metrics = early.current_value().expect("metrics should be present");
    assert!(
        matches!(
            early_metrics
                .scalar_observations()
                .global_usage_pct
                .availability(),
            taskmanager_core::ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
        ),
        "usage before the second sample is honestly unavailable"
    );
    assert!(early_metrics.current_frequency_mhz().is_some());

    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    let result = provider.refresh(2_000);
    assert!(result.is_ok());
    let obs = result.unwrap();
    let metrics = obs.current_value().expect("metrics should be present");
    eprintln!(
        "LIVE WIN CPU TELEMETRY: brand={:?}, freq={:?}, max_freq={:?}, temp={:?}, cores={}",
        metrics.brand,
        metrics.current_frequency_mhz(),
        metrics.current_max_frequency_mhz(),
        metrics.current_temperature_c(),
        metrics.current_core_usage_len()
    );
    let per_core_freqs = (0..metrics.current_core_frequency_len())
        .map(|index| metrics.current_core_frequency_mhz(index))
        .collect::<Vec<_>>();
    eprintln!("  PER CORE FREQS: {per_core_freqs:?}");
    assert!(metrics.current_frequency_mhz().is_some());
    assert!(metrics.current_core_usage_len() > 0);
    assert_eq!(
        metrics
            .scalar_observations()
            .global_usage_pct
            .availability(),
        taskmanager_core::ScalarAvailability::Available
    );
    // The power overlay is a Windows-only native source: off-Windows the
    // energy preference must be an honest None, never a fabricated default.
    #[cfg(not(windows))]
    {
        assert_eq!(metrics.performance_policy.energy_preference, None);
        assert_eq!(metrics.performance_policy.active_policy, None);
    }
}

//! Correlated CPU, host, and independent-domain history regressions.

use super::*;

#[test]
fn correlated_history_is_bounded_oldest_first_and_not_prefilled() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);
    assert!(store.system_history.cpu_usage().samples().is_empty());

    for (revision, usage) in [(1, 10.0), (2, 20.0), (3, 30.0)] {
        let observation = CpuTelemetryObservation::current(observed_cpu(usage, 10), 10, Vec::new());
        ingestor
            .ingest_correlated_cpu(stamp(revision), &observation)
            .expect("increasing revisions append");
    }

    assert_eq!(
        store
            .system_history
            .cpu_usage()
            .samples()
            .into_iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(20.0), Some(30.0)]
    );
}

#[test]
fn current_zero_and_non_current_gap_remain_distinct() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let current = CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(0.0, 10),
            core_usage_group: available_group([0.0], 10),
            frequency_mhz: ScalarObservation::available(0, 10),
            temperature_c: ScalarObservation::available(0.0, 10),
            power_w: ScalarObservation::available(0.0, 10),
            ..Default::default()
        }),
        10,
        Vec::new(),
    );
    ingestor
        .ingest_correlated_cpu(stamp(1), &current)
        .expect("first accepted event should append");
    let stale = CpuTelemetryObservation::stale(
        current
            .last_known_value()
            .cloned()
            .expect("fixture has a last-known value"),
        10,
        FailureKind::TemporarilyUnavailable,
        Vec::new(),
    );
    ingestor
        .ingest_correlated_cpu(stamp(2), &stale)
        .expect("next accepted event should append a gap");

    assert_eq!(
        store.system_history.cpu_usage().samples(),
        [
            CorrelatedMetricSample {
                stamp: stamp(1),
                measured_at_ms: Some(10),
                value: Some(0.0),
            },
            CorrelatedMetricSample {
                stamp: stamp(2),
                measured_at_ms: None,
                value: None,
            },
        ]
    );
    assert_eq!(
        store.system_history.cpu_temperature().samples()[1].value,
        None
    );
    assert_eq!(
        store.system_history.cpu_core_usage()[0].samples()[1].value,
        None
    );
    assert_eq!(
        store.system_history.receipts(SystemHistoryDomain::Cpu)[1].state,
        stale.state()
    );
}

#[test]
fn dynamic_power_and_sensor_history_is_generation_scoped_and_gap_aware() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let battery_id = "power-supply:BAT0";
    let mut battery = BatteryInfo::new(battery_id, DeviceState::healthy(100));
    battery.device_generation = DeviceGeneration::new(1);
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(73, 100),
        power_w: ScalarObservation::available(12.5, 100),
        ..Default::default()
    });
    ingestor
        .ingest_correlated_power_supplies(
            stamp_at(1, 110),
            &PowerSupplySnapshot {
                timestamp_ms: 100,
                batteries: vec![battery],
                ..Default::default()
            },
        )
        .expect("first dynamic power event should be accepted");
    ingestor
        .ingest_correlated_power_supplies(
            stamp_at(2, 210),
            &PowerSupplySnapshot {
                timestamp_ms: 200,
                batteries: vec![{
                    let mut battery = BatteryInfo::new(battery_id, DeviceState::healthy(200));
                    battery.device_generation = DeviceGeneration::new(1);
                    battery
                }],
                ..Default::default()
            },
        )
        .expect("a missing scalar should append a gap");
    let battery_history = store
        .dynamic_history
        .battery_capacity_pct(&DeviceId::new(battery_id))
        .expect("battery history exists");
    assert_eq!(
        battery_history
            .samples()
            .into_iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(73.0), None]
    );

    let fan_id = "hwmon:pwm:fan1_input";
    let fan = fan_reading(
        DeviceId::new("hwmon:pwm"),
        fan_id.to_string(),
        "CPU fan".to_string(),
        1_380,
        100,
    )
    .with_device_generation(DeviceGeneration::new(4));
    ingestor
        .ingest_correlated_sensors(
            stamp_at(1, 110),
            &SensorCenterSnapshot {
                timestamp_ms: 100,
                readings: vec![fan],
                ..Default::default()
            },
        )
        .expect("fan event should be accepted");
    let fan_history = store
        .dynamic_history
        .fan_rpm(&DeviceId::new(fan_id))
        .expect("fan history exists");
    assert_eq!(
        fan_history
            .samples()
            .into_iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(1_380.0)]
    );

    ingestor
        .ingest_correlated_sensors(
            stamp_at(2, 210),
            &SensorCenterSnapshot {
                timestamp_ms: 200,
                readings: vec![
                    fan_reading(
                        DeviceId::new("hwmon:pwm"),
                        fan_id.to_string(),
                        "CPU fan".to_string(),
                        900,
                        200,
                    )
                    .with_device_generation(DeviceGeneration::new(5)),
                ],
                ..Default::default()
            },
        )
        .expect("reinserted fan event should be accepted");
    let reinserted_history = store
        .dynamic_history
        .fan_rpm(&DeviceId::new(fan_id))
        .expect("reinserted fan history exists");
    assert_eq!(
        reinserted_history
            .samples()
            .into_iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(900.0)]
    );

    ingestor
        .ingest_correlated_power_supplies(
            stamp_at(3, 310),
            &PowerSupplySnapshot {
                state: DeviceState::healthy(300),
                timestamp_ms: 300,
                ..Default::default()
            },
        )
        .expect("healthy empty power enumeration should retire old identities");
    assert!(
        store
            .dynamic_history
            .battery_capacity_pct(&DeviceId::new(battery_id))
            .is_none(),
        "a battery absent from an authoritative enumeration must not retain a ring forever"
    );

    ingestor
        .ingest_correlated_sensors(
            stamp_at(3, 310),
            &SensorCenterSnapshot {
                state: DeviceState::healthy(300),
                timestamp_ms: 300,
                ..Default::default()
            },
        )
        .expect("healthy empty sensor enumeration should retire old channels");
    assert!(
        store
            .dynamic_history
            .fan_rpm(&DeviceId::new(fan_id))
            .is_none(),
        "a channel absent from an authoritative enumeration must not retain a ring forever"
    );
}

#[test]
fn current_cpu_domain_keeps_independent_scalar_failures_as_history_gaps() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);
    let cpu = CpuTelemetryObservation::partial(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(42.0, 10),
            core_usage_group: available_group([42.0], 10),
            frequency_mhz: ScalarObservation::available(3_200, 10),
            temperature_c: ScalarObservation::unavailable(FailureKind::PermissionDenied),
            power_w: ScalarObservation::unavailable(FailureKind::Unsupported),
            ..Default::default()
        }),
        10,
        FailureKind::PermissionDenied,
        Vec::new(),
    );

    ingestor
        .ingest_correlated_cpu(stamp(1), &cpu)
        .expect("partial CPU domain remains a current accepted observation");

    assert_eq!(
        store.system_history.cpu_usage().samples()[0].value,
        Some(42.0)
    );
    assert_eq!(
        store.system_history.cpu_frequency_mhz().samples()[0].value,
        Some(3_200)
    );
    assert_eq!(
        store.system_history.cpu_temperature().samples()[0].value,
        None
    );
    assert_eq!(store.system_history.cpu_power_w().samples()[0].value, None);
}

#[test]
fn host_scalar_unknowns_are_gaps_inside_a_current_partial_domain() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);
    let host = HostRuntimeObservation::partial(
        HostRuntimeFacts {
            uptime_secs: ScalarObservation::available(0, 10),
            processes: ScalarObservation::unavailable(FailureKind::PermissionDenied),
            threads: ScalarObservation::available(0, 10),
        },
        10,
        FailureKind::PermissionDenied,
        Vec::new(),
    );

    ingestor
        .ingest_correlated_host(stamp(1), &host)
        .expect("partial is a current accepted observation");

    assert_eq!(
        store.system_history.uptime_secs().samples()[0].value,
        Some(0)
    );
    assert_eq!(
        store.system_history.process_count().samples()[0].value,
        None
    );
    assert_eq!(
        store.system_history.thread_count().samples()[0].value,
        Some(0)
    );
}

#[test]
fn revisions_are_monotonic_per_domain_and_duplicates_do_not_append() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let cpu = CpuTelemetryObservation::current(CpuMetrics::default(), 10, Vec::new());
    let memory = MemoryTelemetryObservation::current(MemoryMetrics::default(), 10, Vec::new());

    ingestor
        .ingest_correlated_cpu(stamp(4), &cpu)
        .expect("first CPU revision");
    ingestor
        .ingest_correlated_memory(stamp(4), &memory)
        .expect("same revision is independent in memory");
    let error = ingestor
        .ingest_correlated_cpu(stamp(4), &cpu)
        .expect_err("duplicate CPU revision must be rejected");

    assert_eq!(
        error,
        CorrelatedIngestionError::NonIncreasingRevision {
            domain: SystemHistoryDomain::Cpu,
            last_revision: 4,
            rejected_revision: 4,
        }
    );
    assert_eq!(
        store
            .system_history
            .receipts(SystemHistoryDomain::Cpu)
            .len(),
        1
    );
    assert_eq!(
        store
            .system_history
            .receipts(SystemHistoryDomain::Memory)
            .len(),
        1
    );
}

#[test]
fn watermark_tracks_length_and_latest_revision_without_cloning() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    // Empty history: no samples, no revision.
    assert_eq!(store.system_history.cpu_usage().watermark(), (0, None));

    let cpu = CpuTelemetryObservation::current(observed_cpu(12.0, 10), 10, Vec::new());
    ingestor
        .ingest_correlated_cpu(stamp(7), &cpu)
        .expect("accepted");
    assert_eq!(store.system_history.cpu_usage().watermark(), (1, Some(7)));
    ingestor
        .ingest_correlated_cpu(stamp(9), &cpu)
        .expect("accepted");
    assert_eq!(store.system_history.cpu_usage().watermark(), (2, Some(9)));
}

/// Per-core temperature and frequency ride the same accepted CPU event as
/// per-core usage: values append per logical core, and an unresolvable sensor
/// (or a stale/unavailable domain) appends an explicit gap — never a
/// fabricated value. This is the data path the per-core metric switcher on
/// the CPU page consumes.
#[test]
fn per_core_temperature_and_frequency_histories_append_and_gap() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let two_cores = CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(30.0, 10),
            core_usage_group: available_group([30.0, 40.0], 10),
            per_core_frequency_group: available_group([2_400, 3_200], 10),
            per_core_temperature_group: ScalarObservationGroup::partial(
                vec![
                    ScalarObservationSlot::Current(45.0),
                    ScalarObservationSlot::Unavailable(FailureKind::Unsupported),
                ],
                10,
                FailureKind::Unsupported,
            ),
            ..Default::default()
        }),
        10,
        Vec::new(),
    );
    ingestor
        .ingest_correlated_cpu(stamp(1), &two_cores)
        .expect("first CPU event appends");

    let values = |history: &CorrelatedMetricHistory<f32>| {
        history
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>()
    };
    let temperatures = store.system_history.cpu_core_temperature();
    assert_eq!(temperatures.len(), 2, "one ring per logical core");
    assert_eq!(values(&temperatures[0]), [Some(45.0)]);
    assert_eq!(
        values(&temperatures[1]),
        [None],
        "an unresolvable per-core sensor must be a gap, not a fabricated value"
    );
    let frequencies = store.system_history.cpu_core_frequency_mhz();
    let freq_values = |history: &CorrelatedMetricHistory<u64>| {
        history
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>()
    };
    assert_eq!(frequencies.len(), 2);
    assert_eq!(freq_values(&frequencies[0]), [Some(2_400)]);
    assert_eq!(freq_values(&frequencies[1]), [Some(3_200)]);

    // A stale follow-up event appends gaps across every per-core family.
    let stale = CpuTelemetryObservation::stale(
        two_cores
            .last_known_value()
            .cloned()
            .expect("fixture has a last-known value"),
        10,
        FailureKind::TemporarilyUnavailable,
        Vec::new(),
    );
    ingestor
        .ingest_correlated_cpu(stamp(2), &stale)
        .expect("stale event appends gaps");
    assert_eq!(
        values(&store.system_history.cpu_core_temperature()[0]),
        [Some(45.0), None]
    );
    assert_eq!(
        freq_values(&store.system_history.cpu_core_frequency_mhz()[1]),
        [Some(3_200), None]
    );
    assert_eq!(
        store.system_history.cpu_core_usage()[0]
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(30.0), None]
    );
}

#[test]
fn correlated_per_core_history_has_a_hard_outer_cardinality_bound() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(1);
    let reported = taskmanager_core::MAX_TRACKED_LOGICAL_CPUS + 1;
    let cpu = CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            core_usage_group: available_group(vec![1.0; reported], 10),
            ..Default::default()
        }),
        10,
        Vec::new(),
    );
    ingestor
        .ingest_correlated_cpu(stamp(1), &cpu)
        .expect("oversized provider vector is accepted without multiplying storage");

    assert_eq!(
        cpu.last_known_value()
            .expect("current CPU value")
            .current_core_usage_len(),
        reported
    );
    assert_eq!(
        store.system_history.cpu_core_usage().len(),
        taskmanager_core::MAX_TRACKED_LOGICAL_CPUS
    );
    assert_eq!(
        store.system_history.cpu_core_temperature().len(),
        taskmanager_core::MAX_TRACKED_LOGICAL_CPUS
    );
    assert_eq!(
        store.system_history.cpu_core_frequency_mhz().len(),
        taskmanager_core::MAX_TRACKED_LOGICAL_CPUS
    );
}

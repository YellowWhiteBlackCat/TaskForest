//! Device generation and optional-enrichment history regressions.

use super::*;

#[test]
fn absent_device_writes_a_gap_and_reappearance_resets_generation_history() {
    let device_id = "disk:wwid:fixture";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let present = StorageTelemetryObservation::current(
        vec![healthy_disk(device_id, 1, 42.0)],
        10,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 1),
    );
    ingestor
        .ingest_correlated_storage(stamp(1), &present)
        .expect("present generation should append");

    let absent = StorageTelemetryObservation::current(
        Vec::new(),
        20,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Absent, 1),
    );
    ingestor
        .ingest_correlated_storage(stamp(2), &absent)
        .expect("confirmed absence should append a gap");
    assert_eq!(
        store
            .system_history
            .storage_activity(&DeviceId::new(device_id))
            .expect("retained absent history")
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(42.0), None]
    );

    let reappeared = StorageTelemetryObservation::current(
        vec![healthy_disk(device_id, 2, 0.0)],
        30,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 2),
    );
    let report = ingestor
        .ingest_correlated_storage(stamp(3), &reappeared)
        .expect("new generation should append");
    let history = store
        .system_history
        .storage_activity(&DeviceId::new(device_id))
        .expect("reappeared history");

    assert_eq!(report.reset_device_histories, 1);
    assert_eq!(history.generation(), 2);
    assert_eq!(history.samples()[0].value, Some(0.0));
    assert_eq!(history.samples().len(), 1);
}

#[test]
fn storage_rate_keeps_domain_gaps_but_never_bridges_device_generations() {
    let device_id = "disk:wwid:rate-fixture";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let present = StorageTelemetryObservation::current(
        vec![healthy_disk_with_rate(device_id, 1, 7, 5)],
        10,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 1),
    );
    ingestor
        .ingest_correlated_storage(stamp(1), &present)
        .expect("present generation should append its observed rate");

    let absent = StorageTelemetryObservation::current(
        Vec::new(),
        20,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Absent, 1),
    );
    ingestor
        .ingest_correlated_storage(stamp(2), &absent)
        .expect("confirmed absence should append a rate gap");

    let reappeared = StorageTelemetryObservation::current(
        vec![healthy_disk_with_rate(device_id, 2, 4, 3)],
        30,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 2),
    );
    ingestor
        .ingest_correlated_storage(stamp(3), &reappeared)
        .expect("new generation should append independently");

    let device_history = store
        .system_history
        .storage_rate(&DeviceId::new(device_id))
        .expect("reappeared storage-rate history");
    assert_eq!(device_history.generation(), 2);
    assert_eq!(
        device_history
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(7)],
        "a new physical generation must not inherit the old device curve"
    );
    assert_eq!(
        store
            .system_history
            .storage_rate_total()
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(12), None, Some(7)],
        "the host-wide curve keeps the accepted domain timeline and its gap"
    );

    // Iced and TUI hold clones of this exact renderer-neutral projection. The
    // same accepted event trajectory must therefore produce byte-identical
    // finite/gap classification and the same post-reattach device curve.
    let iced_reader = crate::live_graph::LiveGraphHistory::from_store(store.clone(), 60);
    let tui_reader = crate::live_graph::LiveGraphHistory::from_store(store, 60);
    let classify = |samples: Vec<f32>| {
        samples
            .into_iter()
            .map(|sample| sample.is_finite().then_some(sample))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        classify(iced_reader.series(crate::live_graph::MetricSeries::DiskBytesPerSec)),
        [Some(12.0), None, Some(7.0)]
    );
    assert_eq!(
        classify(tui_reader.series(crate::live_graph::MetricSeries::DiskBytesPerSec)),
        [Some(12.0), None, Some(7.0)]
    );
    assert_eq!(
        iced_reader.disk_bytes_per_sec_for(device_id, 2),
        tui_reader.disk_bytes_per_sec_for(device_id, 2)
    );
    assert_eq!(
        iced_reader.disk_bytes_per_sec_for(device_id, 2),
        [7.0],
        "both frontend readers see only the reattached generation"
    );
}

#[test]
fn aggregate_io_rates_gap_on_partial_devices_but_empty_inventory_is_zero() {
    let disk_ok = "disk:aggregate:ok";
    let disk_gap = "disk:aggregate:gap";
    let network_ok = "network:aggregate:ok";
    let network_gap = "network:aggregate:gap";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let lifecycle_map = |first: &str, second: &str, observed_at_ms| {
        BTreeMap::from([
            (
                DeviceId::new(first),
                lifecycle(DevicePresence::Present, 1, observed_at_ms),
            ),
            (
                DeviceId::new(second),
                lifecycle(DevicePresence::Present, 1, observed_at_ms),
            ),
        ])
    };
    ingestor
        .ingest_correlated_storage(
            stamp_at(1, 10),
            &StorageTelemetryObservation::current(
                vec![
                    healthy_disk_with_rate(disk_ok, 1, 7, 5),
                    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                        .device_id(disk_gap.to_owned())
                        .device_generation(DeviceGeneration::new(1))
                        .device_state(DeviceState::healthy(10))
                        .build(),
                ],
                10,
                Vec::new(),
                Vec::new(),
                lifecycle_map(disk_ok, disk_gap, 10),
            ),
        )
        .expect("mixed storage observation");
    let network = |device_id: &str, rx, tx| {
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id(Arc::from(device_id))
            .device_generation(DeviceGeneration::new(1))
            .device_state(DeviceState::healthy(10))
            .scalar_observations(NetworkScalarObservations {
                rx_bytes_per_sec: rx,
                tx_bytes_per_sec: tx,
                ..Default::default()
            })
            .build()
    };
    ingestor
        .ingest_correlated_network(
            stamp_at(1, 10),
            &NetworkTelemetryObservation::current(
                vec![
                    network(
                        network_ok,
                        ScalarObservation::available(2, 10),
                        ScalarObservation::available(3, 10),
                    ),
                    network(
                        network_gap,
                        ScalarObservation::available(4, 10),
                        ScalarObservation::default(),
                    ),
                ],
                10,
                Vec::new(),
                Vec::new(),
                lifecycle_map(network_ok, network_gap, 10),
            ),
        )
        .expect("mixed network observation");
    ingestor
        .ingest_correlated_storage(
            stamp_at(2, 20),
            &StorageTelemetryObservation::current(
                Vec::new(),
                20,
                Vec::new(),
                Vec::new(),
                BTreeMap::new(),
            ),
        )
        .expect("authoritative empty storage inventory");
    ingestor
        .ingest_correlated_network(
            stamp_at(2, 20),
            &NetworkTelemetryObservation::current(
                Vec::new(),
                20,
                Vec::new(),
                Vec::new(),
                BTreeMap::new(),
            ),
        )
        .expect("authoritative empty network inventory");

    assert_eq!(
        store
            .system_history
            .storage_rate_total()
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [None, Some(0)]
    );
    assert_eq!(
        store
            .system_history
            .network_rate_total()
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [None, Some(0)]
    );
}

#[test]
fn smart_temperature_history_is_scoped_to_disk_identity() {
    let disk_a = "disk:wwid:temperature-a";
    let disk_b = "disk:wwid:temperature-b";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let observation = |observed_at_ms, temperature_a, temperature_b| {
        let disk = |device_id: &str, temperature_c| {
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id(device_id.to_owned())
                .device_generation(DeviceGeneration::new(1))
                .device_state(DeviceState::healthy(observed_at_ms))
                .smart_availability(taskmanager_core::SmartAvailability::Available)
                .smart_state(DeviceState::healthy(observed_at_ms))
                .smart_temperature_c(Some(temperature_c))
                .build()
        };
        StorageTelemetryObservation::current(
            vec![disk(disk_a, temperature_a), disk(disk_b, temperature_b)],
            observed_at_ms,
            Vec::new(),
            Vec::new(),
            BTreeMap::from([
                (
                    DeviceId::new(disk_a),
                    lifecycle(DevicePresence::Present, 1, observed_at_ms),
                ),
                (
                    DeviceId::new(disk_b),
                    lifecycle(DevicePresence::Present, 1, observed_at_ms),
                ),
            ]),
        )
    };
    ingestor
        .ingest_correlated_storage(stamp_at(1, 10), &observation(10, 31.0, 61.0))
        .expect("first two-disk temperature observation");
    ingestor
        .ingest_correlated_storage(stamp_at(2, 20), &observation(20, 32.0, 62.0))
        .expect("second two-disk temperature observation");

    let reader = crate::live_graph::LiveGraphHistory::from_store(store.clone(), 8);
    assert_eq!(reader.disk_temperature_c_for(disk_a, 1), [31.0, 32.0]);
    assert_eq!(reader.disk_temperature_c_for(disk_b, 1), [61.0, 62.0]);
    assert_eq!(
        store
            .system_history
            .storage_temperature_c(&DeviceId::new(disk_a))
            .expect("disk A history")
            .generation(),
        1
    );
}

#[test]
fn smart_temperature_cache_and_failed_refresh_append_gaps() {
    let device_id = "disk:wwid:smart-freshness";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let observation = |storage_observed_at_ms, smart_state, temperature_c| {
        StorageTelemetryObservation::current(
            vec![
                taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                    .device_id(device_id.to_owned())
                    .device_generation(DeviceGeneration::new(1))
                    .device_state(DeviceState::healthy(storage_observed_at_ms))
                    .smart_availability(taskmanager_core::SmartAvailability::Available)
                    .smart_state(smart_state)
                    .smart_temperature_c(Some(temperature_c))
                    .build(),
            ],
            storage_observed_at_ms,
            Vec::new(),
            Vec::new(),
            BTreeMap::from([(
                DeviceId::new(device_id),
                lifecycle(DevicePresence::Present, 1, storage_observed_at_ms),
            )]),
        )
    };
    ingestor
        .ingest_correlated_storage(
            stamp_at(1, 105),
            &observation(105, DeviceState::healthy(100), 31.0),
        )
        .expect("independent SMART sample predating the storage batch");
    ingestor
        .ingest_correlated_storage(
            stamp_at(2, 110),
            &observation(110, DeviceState::healthy(100), 31.0),
        )
        .expect("later storage tick with the same SMART success timestamp");
    ingestor
        .ingest_correlated_storage(
            stamp_at(3, 125),
            &observation(125, DeviceState::healthy(120), 32.0),
        )
        .expect("new independent SMART success timestamp");

    let partial = StorageTelemetryObservation::partial(
        vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id(device_id.to_owned())
                .device_generation(DeviceGeneration::new(1))
                .device_state(DeviceState::healthy(130))
                .smart_availability(taskmanager_core::SmartAvailability::Available)
                .smart_state(DeviceState {
                    status: DeviceStatus::Stale,
                    last_success_ms: Some(120),
                })
                .smart_temperature_c(Some(32.0))
                .build(),
        ],
        130,
        FailureKind::ProviderFault,
        Vec::new(),
        Vec::new(),
        BTreeMap::from([(
            DeviceId::new(device_id),
            lifecycle(DevicePresence::Present, 1, 130),
        )]),
    );
    ingestor
        .ingest_correlated_storage(stamp_at(4, 130), &partial)
        .expect("partial storage observation remains an accepted gap");

    let samples = store
        .system_history
        .storage_temperature_c(&DeviceId::new(device_id))
        .expect("temperature history exists")
        .samples();
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(31.0), None, Some(32.0), None],
        "an old cached SMART scalar must not become a new measured point"
    );
    assert_eq!(samples[0].measured_at_ms, Some(100));
    assert_eq!(samples[1].measured_at_ms, None);
    assert_eq!(samples[2].measured_at_ms, Some(120));
    assert_eq!(samples[3].measured_at_ms, None);
}

#[test]
fn mismatched_device_generation_never_bridges_history() {
    let device_id = "gpu:pci:0000:01:00.0";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(3);
    let mut gpu = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(77.0, 10),
        ..Default::default()
    });
    gpu.device_id = device_id.to_owned();
    gpu.device_generation = DeviceGeneration::new(1);
    gpu.device_state = DeviceState::healthy(10);
    let observation = GpuTelemetryObservation::current(
        vec![gpu],
        10,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 2),
    );

    let report = ingestor
        .ingest_correlated_gpu(stamp(1), &observation)
        .expect("accepted domain still records its receipt");

    assert_eq!(report.rejected_device_values, 1);
    assert!(
        store
            .system_history
            .gpu_usage(&DeviceId::new(device_id))
            .is_none()
    );
    assert_eq!(
        store
            .system_history
            .receipts(SystemHistoryDomain::Gpu)
            .len(),
        1
    );
}

#[test]
fn gpu_history_uses_typed_current_truth_and_records_stale_as_a_gap() {
    let device_id = "gpu:pci:0000:03:00.0";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let observation = |scalar_observations, observed_at_ms| {
        let mut gpu = GpuMetrics::from_observations(scalar_observations);
        gpu.device_id = device_id.to_owned();
        gpu.device_generation = DeviceGeneration::new(1);
        gpu.device_state = DeviceState::healthy(observed_at_ms);
        GpuTelemetryObservation::current(
            vec![gpu],
            observed_at_ms,
            Vec::new(),
            Vec::new(),
            lifecycles(device_id, DevicePresence::Present, 1),
        )
    };
    ingestor
        .ingest_correlated_gpu(
            stamp(1),
            &observation(
                GpuScalarObservations {
                    utilization_pct: ScalarObservation::available(0.0, 10),
                    ..Default::default()
                },
                10,
            ),
        )
        .expect("typed idle sample should append");
    ingestor
        .ingest_correlated_gpu(
            stamp(2),
            &observation(
                GpuScalarObservations {
                    utilization_pct: ScalarObservation::available(0.0, 10)
                        .transition_failure(FailureKind::PermissionDenied),
                    ..Default::default()
                },
                20,
            ),
        )
        .expect("stale scalar should append a gap");
    ingestor
        .ingest_correlated_gpu(
            stamp(3),
            &observation(
                GpuScalarObservations {
                    utilization_pct: ScalarObservation::available(42.0, 30),
                    ..Default::default()
                },
                30,
            ),
        )
        .expect("recovered scalar should append");

    let samples = store
        .system_history
        .gpu_usage(&DeviceId::new(device_id))
        .expect("GPU history exists")
        .samples()
        .into_iter()
        .map(|sample| sample.value)
        .collect::<Vec<_>>();
    assert_eq!(samples, [Some(0.0), None, Some(42.0)]);
    let typed_samples = store
        .system_history
        .gpu_metrics(&DeviceId::new(device_id))
        .expect("typed GPU history exists")
        .samples()
        .into_iter()
        .map(|sample| sample.value.and_then(|point| point.utilization_pct))
        .collect::<Vec<_>>();
    assert_eq!(
        typed_samples,
        [Some(0.0), None, Some(42.0)],
        "permission-denied refresh must be a typed history gap"
    );
    assert_eq!(
        store
            .system_history
            .gpu_usage_mean()
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(0.0), None, Some(42.0)],
        "the renderer-neutral aggregate must preserve the same accepted-event gaps"
    );
}

#[test]
fn gpu_typed_history_keeps_scalar_units_and_missing_fields_as_none() {
    let device_id = "gpu:pci:0000:04:00.0";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let mut gpu = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(37.5, 10),
        temperature_c: ScalarObservation::available(61.0, 10),
        memory_used_bytes: ScalarObservation::available(3 * 1024, 10),
        memory_total_bytes: ScalarObservation::available(8 * 1024, 10),
        dedicated_vram_used_bytes: ScalarObservation::available(2 * 1024, 10),
        dedicated_vram_total_bytes: ScalarObservation::available(4 * 1024, 10),
        power_w: ScalarObservation::available(42.25, 10),
        frequency_mhz: ScalarObservation::available(1_800, 10),
        ..Default::default()
    });
    gpu.device_id = device_id.to_owned();
    gpu.device_generation = DeviceGeneration::new(1);
    gpu.device_state = DeviceState::healthy(10);
    gpu.engines = vec![
        GpuEngine {
            name: "Render/3D".into(),
            kind: GpuEngineKind::Render,
            usage_pct: 0.0,
        },
        GpuEngine {
            name: "Video Decode".into(),
            kind: GpuEngineKind::VideoDecode,
            usage_pct: 12.5,
        },
    ];
    let observation = GpuTelemetryObservation::current(
        vec![gpu],
        10,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 1),
    );

    ingestor
        .ingest_correlated_gpu(stamp(1), &observation)
        .expect("typed GPU point should append");

    let point = store
        .system_history
        .gpu_metrics(&DeviceId::new(device_id))
        .expect("typed GPU history exists")
        .samples()[0]
        .value
        .expect("current GPU point exists");
    assert_eq!(point.utilization_pct, Some(37.5));
    assert_eq!(point.temperature_c, Some(61.0));
    assert_eq!(point.memory_used_bytes, Some(3 * 1024));
    assert_eq!(point.memory_total_bytes, Some(8 * 1024));
    assert_eq!(point.dedicated_memory_used_bytes, Some(2 * 1024));
    assert_eq!(point.dedicated_memory_total_bytes, Some(4 * 1024));
    assert_eq!(point.power_w, Some(42.25));
    assert_eq!(point.frequency_mhz, Some(1_800));
    assert_eq!(point.shared_memory_used_bytes, None);
    assert_eq!(point.idle_residency_pct, None);

    let engine_samples = store
        .system_history
        .gpu_engine_metrics(&DeviceId::new(device_id))
        .expect("typed engine history exists")
        .samples();
    let engine_point = engine_samples[0]
        .value
        .as_ref()
        .expect("current engine point exists");
    assert_eq!(
        engine_point
            .engines
            .iter()
            .map(|engine| (engine.name.as_str(), engine.kind, engine.utilization_pct))
            .collect::<Vec<_>>(),
        [
            ("Render/3D", GpuEngineKind::Render, 0.0),
            ("Video Decode", GpuEngineKind::VideoDecode, 12.5),
        ]
    );
}

#[test]
fn every_domain_has_an_independent_typed_ingestion_lane() {
    let device_id = "net:mac:00:11:22:33:44:55";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);
    let memory =
        MemoryTelemetryObservation::current(observed_memory(100, 0, 0, 0, 10), 10, Vec::new());
    let network = NetworkTelemetryObservation::current(
        vec![
            taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                .device_id(Arc::from(device_id))
                .device_generation(DeviceGeneration::new(1))
                .device_state(DeviceState {
                    status: DeviceStatus::Healthy,
                    last_success_ms: Some(10),
                })
                .scalar_observations(NetworkScalarObservations {
                    rx_bytes_per_sec: ScalarObservation::available(2, 10),
                    tx_bytes_per_sec: ScalarObservation::available(3, 10),
                    ..Default::default()
                })
                .build(),
        ],
        10,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 1),
    );
    let storage = StorageTelemetryObservation::current(
        Vec::new(),
        10,
        Vec::new(),
        Vec::new(),
        BTreeMap::new(),
    );
    let gpu = GpuTelemetryObservation::unavailable(
        FailureKind::Unsupported,
        Vec::new(),
        Vec::new(),
        BTreeMap::new(),
    );

    ingestor
        .ingest_correlated_memory(stamp(1), &memory)
        .expect("memory lane");
    ingestor
        .ingest_correlated_network(stamp(1), &network)
        .expect("network lane");
    ingestor
        .ingest_correlated_storage(stamp(1), &storage)
        .expect("storage lane");
    ingestor
        .ingest_correlated_gpu(stamp(1), &gpu)
        .expect("GPU gap lane");

    assert_eq!(
        store.system_history.memory_usage().samples()[0].value,
        Some(0.0)
    );
    assert_eq!(store.system_history.swap_usage().samples()[0].value, None);
    assert_eq!(
        store
            .system_history
            .network_rate(&DeviceId::new(device_id))
            .expect("network identity")
            .samples()[0]
            .value,
        Some(5)
    );
    assert_eq!(
        store.system_history.network_rate_total().samples()[0].value,
        Some(5)
    );
    ingestor
        .ingest_correlated_unavailable(
            stamp(2),
            SystemHistoryDomain::Network,
            FailureKind::TimedOut,
        )
        .expect("terminal network failure should advance every network curve");
    assert_eq!(
        store
            .system_history
            .network_rate_total()
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(5), None],
        "terminal failures are gaps, never fabricated zero throughput"
    );
    assert_eq!(
        store.system_history.receipts(SystemHistoryDomain::Storage)[0].state,
        storage.state()
    );
    assert_eq!(
        store.system_history.receipts(SystemHistoryDomain::Gpu)[0].state,
        gpu.state()
    );
}

#[test]
fn current_authoritative_lifecycle_prunes_expired_device_history() {
    let device_id = "disk:wwid:expired";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);
    let present = StorageTelemetryObservation::current(
        vec![healthy_disk(device_id, 1, 1.0)],
        10,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 1),
    );
    ingestor
        .ingest_correlated_storage(stamp(1), &present)
        .expect("present device");
    let expired = StorageTelemetryObservation::current(
        Vec::new(),
        20,
        Vec::new(),
        Vec::new(),
        BTreeMap::new(),
    );
    let report = ingestor
        .ingest_correlated_storage(stamp(2), &expired)
        .expect("authoritative empty lifecycle");

    assert_eq!(report.pruned_device_histories, 1);
    assert!(
        store
            .system_history
            .storage_activity(&DeviceId::new(device_id))
            .is_none()
    );
}

#[test]
fn valid_metric_survives_unhealthy_optional_enrichment_state() {
    let device_id = "disk:wwid:smart-denied";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);
    let mut disk = healthy_disk(device_id, 1, 25.0);
    disk.device_state = DeviceState {
        status: DeviceStatus::PermissionDenied,
        last_success_ms: Some(9),
    };
    let observation = StorageTelemetryObservation::partial(
        vec![disk],
        10,
        FailureKind::PermissionDenied,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 1),
    );

    ingestor
        .ingest_correlated_storage(stamp(1), &observation)
        .expect("current partial domain keeps its valid activity metric");

    assert_eq!(
        store
            .system_history
            .storage_activity(&DeviceId::new(device_id))
            .expect("typed identity history")
            .samples()[0]
            .value,
        Some(25.0)
    );
}

/// Classify a live-graph window the way the frontend renderers consume it:
/// finite samples stay values, explicit gaps become `None`.
fn classify_window(samples: &[f32]) -> Vec<Option<f32>> {
    samples
        .iter()
        .map(|sample| sample.is_finite().then_some(*sample))
        .collect()
}

// The interim host-wide activity mean ring (and its test) was removed with
// the scope model: disk active time is a device-domain fact, the host leg is
// an explicit ChartSeriesError::MissingDeviceIdentity, and the per-device
// gap/finite semantics below remain the authority. Scope routing itself is
// proven in tests/headless/live_graph.rs.
#[test]
fn disk_active_time_live_window_is_generation_scoped_and_honestly_empty() {
    let device_id = "disk:wwid:active-window";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let observation = |generation, activity, observed_at_ms| {
        StorageTelemetryObservation::current(
            vec![healthy_disk(device_id, generation, activity)],
            observed_at_ms,
            Vec::new(),
            Vec::new(),
            lifecycles(device_id, DevicePresence::Present, generation),
        )
    };
    ingestor
        .ingest_correlated_storage(stamp_at(1, 10), &observation(1, 40.0, 10))
        .expect("first active-time observation");
    ingestor
        .ingest_correlated_storage(stamp_at(2, 20), &observation(1, 60.0, 20))
        .expect("second active-time observation");

    // Iced and TUI read clones of the one renderer-neutral projection: the
    // same accepted trajectory must produce identical windows.
    let iced_reader = crate::live_graph::LiveGraphHistory::from_store(store.clone(), 60);
    let tui_reader = crate::live_graph::LiveGraphHistory::from_store(store, 60);
    assert_eq!(
        iced_reader.disk_active_time_pct_for(device_id, 1),
        tui_reader.disk_active_time_pct_for(device_id, 1)
    );
    assert_eq!(
        classify_window(&iced_reader.disk_active_time_pct_for(device_id, 1)),
        [Some(40.0), Some(60.0)]
    );

    // A device identity the store has never accepted resolves to an empty
    // window — honest absence, never a fabricated flat 0%.
    assert!(
        iced_reader
            .disk_active_time_pct_for("disk:wwid:never-seen", 1)
            .is_empty()
    );

    // A new physical generation starts a fresh curve; the reader never mixes
    // the retired generation's samples into the new window.
    ingestor
        .ingest_correlated_storage(stamp_at(3, 30), &observation(2, 5.0, 30))
        .expect("reattached generation observation");
    assert_eq!(
        classify_window(&iced_reader.disk_active_time_pct_for(device_id, 2)),
        [Some(5.0)]
    );
}

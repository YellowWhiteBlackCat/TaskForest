//! Scope-aware chart series routing over the live graph read model.

use std::collections::BTreeMap;
use std::sync::Arc;

use taskmanager_core::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, DeviceGeneration, DeviceId,
    DeviceLifecycle, DevicePresence, DeviceState, DiskMetrics, GpuMetrics, GpuScalarObservations,
    GpuTelemetryObservation, NetworkMetrics, NetworkScalarObservations,
    NetworkTelemetryObservation, ScalarObservation, StorageTelemetryObservation,
};
use taskmanager_test_support::{DiskMetricsFixtureBuilder, NetworkMetricsFixtureBuilder};

use super::{
    ChartSeriesError, ChartSeriesQuery, DeviceDomain, LiveGraphHistory, MetricSeries, SeriesScope,
};
use crate::{CorrelatedTelemetryStamp, TelemetryStore};

fn stamp_at(revision: u64, completed_at_ms: u64) -> CorrelatedTelemetryStamp {
    CorrelatedTelemetryStamp::from_accepted_event(revision, completed_at_ms)
        .expect("test revisions are non-zero")
}

fn lifecycle(generation: u64, observed_at_ms: u64) -> DeviceLifecycle {
    DeviceLifecycle {
        presence: DevicePresence::Present,
        state: DeviceState::healthy(observed_at_ms),
        generation,
        first_seen_ms: Some(observed_at_ms),
        last_seen_ms: Some(observed_at_ms),
        absent_since_ms: None,
    }
}

fn present_lifecycles(entries: &[(&str, u64)]) -> BTreeMap<DeviceId, DeviceLifecycle> {
    entries
        .iter()
        .map(|(device_id, generation)| {
            (
                DeviceId::new((*device_id).to_owned()),
                lifecycle(*generation, 10),
            )
        })
        .collect()
}

fn disk(
    device_id: &str,
    generation: u64,
    activity: Option<f32>,
    read_bytes_per_sec: Option<u64>,
    write_bytes_per_sec: Option<u64>,
) -> DiskMetrics {
    DiskMetricsFixtureBuilder::new()
        .device_id(device_id.to_owned())
        .device_generation(DeviceGeneration::new(generation))
        .device_state(DeviceState::healthy(10))
        .scalar_observations(taskmanager_core::DiskScalarObservations {
            active_time_pct: activity
                .map(|value| ScalarObservation::available(value, 10))
                .unwrap_or_default(),
            read_bytes_per_sec: read_bytes_per_sec
                .map(|value| ScalarObservation::available(value, 10))
                .unwrap_or_default(),
            write_bytes_per_sec: write_bytes_per_sec
                .map(|value| ScalarObservation::available(value, 10))
                .unwrap_or_default(),
            ..Default::default()
        })
        .build()
}

fn adapter(device_id: &str, generation: u64, rx: Option<u64>, tx: Option<u64>) -> NetworkMetrics {
    NetworkMetricsFixtureBuilder::new()
        .device_id(Arc::from(device_id))
        .device_generation(DeviceGeneration::new(generation))
        .device_state(DeviceState::healthy(10))
        .scalar_observations(NetworkScalarObservations {
            rx_bytes_per_sec: rx
                .map(|value| ScalarObservation::available(value, 10))
                .unwrap_or_default(),
            tx_bytes_per_sec: tx
                .map(|value| ScalarObservation::available(value, 10))
                .unwrap_or_default(),
            ..Default::default()
        })
        .build()
}

fn gpu(device_id: &str, generation: u64, utilization: Option<f32>) -> GpuMetrics {
    let mut gpu = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: utilization
            .map(|value| ScalarObservation::available(value, 10))
            .unwrap_or_default(),
        ..Default::default()
    });
    gpu.device_id = device_id.to_owned();
    gpu.device_generation = DeviceGeneration::new(generation);
    gpu.device_state = DeviceState::healthy(10);
    gpu
}

fn cpu(usage: f32) -> CpuTelemetryObservation {
    CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(usage, 10),
            ..Default::default()
        }),
        10,
        Vec::new(),
    )
}

/// Classify a live-graph window the way renderers consume it: finite samples
/// stay values, explicit gaps become `None`.
fn classify(samples: &[f32]) -> Vec<Option<f32>> {
    samples
        .iter()
        .map(|sample| sample.is_finite().then_some(*sample))
        .collect()
}

#[test]
fn host_and_device_legs_route_through_one_entry() {
    let disk_a = "disk:wwid:route-a";
    let disk_b = "disk:wwid:route-b";
    let nic = "network:route-nic";
    let gpu_id = "gpu:pci:route-gpu";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    ingestor
        .ingest_correlated_cpu(stamp_at(1, 10), &cpu(42.0))
        .expect("cpu observation");
    ingestor
        .ingest_correlated_storage(
            stamp_at(1, 10),
            &StorageTelemetryObservation::current(
                vec![
                    disk(disk_a, 1, Some(40.0), Some(3), Some(4)),
                    disk(disk_b, 1, None, Some(5), Some(6)),
                ],
                10,
                Vec::new(),
                Vec::new(),
                present_lifecycles(&[(disk_a, 1), (disk_b, 1)]),
            ),
        )
        .expect("storage observation");
    ingestor
        .ingest_correlated_network(
            stamp_at(1, 10),
            &NetworkTelemetryObservation::current(
                vec![adapter(nic, 1, Some(2), Some(3))],
                10,
                Vec::new(),
                Vec::new(),
                present_lifecycles(&[(nic, 1)]),
            ),
        )
        .expect("network observation");
    ingestor
        .ingest_correlated_gpu(
            stamp_at(1, 10),
            &GpuTelemetryObservation::current(
                vec![gpu(gpu_id, 1, Some(77.0))],
                10,
                Vec::new(),
                Vec::new(),
                present_lifecycles(&[(gpu_id, 1)]),
            ),
        )
        .expect("gpu observation");
    let reader = LiveGraphHistory::from_store(store, 8);

    // Host legs: host rings and host aggregates of the same accepted fact.
    assert_eq!(
        classify(
            &reader
                .resolve_series(ChartSeriesQuery::host(MetricSeries::CpuUsagePercent))
                .expect("host cpu query")
        ),
        [Some(42.0)]
    );
    assert_eq!(
        classify(
            &reader
                .resolve_series(ChartSeriesQuery::host(MetricSeries::DiskBytesPerSec))
                .expect("host disk query")
        ),
        [Some(18.0)],
        "the host leg is the summed aggregate (3+4+5+6), not a device window"
    );
    assert_eq!(
        classify(
            &reader
                .resolve_series(ChartSeriesQuery::host(MetricSeries::NetworkBytesPerSec))
                .expect("host network query")
        ),
        [Some(5.0)]
    );
    assert_eq!(
        classify(
            &reader
                .resolve_series(ChartSeriesQuery::host(MetricSeries::GpuUsagePercent))
                .expect("host gpu query")
        ),
        [Some(77.0)],
        "the host leg is the gpu usage mean of the accepted observation"
    );

    // Device legs: per-device rings of the same accepted observations, equal
    // to the legacy per-device accessors they now wrap.
    assert_eq!(
        reader
            .resolve_series(ChartSeriesQuery::device(
                MetricSeries::DiskBytesPerSec,
                &DeviceId::new(disk_a),
                1,
            ))
            .expect("device disk query"),
        reader.disk_bytes_per_sec_for(disk_a, 1)
    );
    assert_eq!(
        classify(&reader.disk_bytes_per_sec_for(disk_a, 1)),
        [Some(7.0)]
    );
    assert_eq!(
        reader
            .resolve_series(ChartSeriesQuery::device(
                MetricSeries::NetworkBytesPerSec,
                &DeviceId::new(nic),
                1,
            ))
            .expect("device network query"),
        reader.network_bytes_per_sec_for(nic, 1)
    );
    assert_eq!(
        reader
            .resolve_series(ChartSeriesQuery::device(
                MetricSeries::GpuUsagePercent,
                &DeviceId::new(gpu_id),
                1,
            ))
            .expect("device gpu query"),
        reader.gpu_usage_pct_for(gpu_id, 1)
    );

    // The device-only activity series keeps per-device availability: disk A
    // carries its measured 40%, disk B's field-level failure stays an honest
    // gap, and an identity the store never accepted stays absent.
    assert_eq!(
        classify(
            &reader
                .resolve_series(ChartSeriesQuery::device(
                    MetricSeries::DiskActiveTimePct,
                    &DeviceId::new(disk_a),
                    1,
                ))
                .expect("device activity query")
        ),
        [Some(40.0)]
    );
    assert_eq!(
        classify(
            &reader
                .resolve_series(ChartSeriesQuery::device(
                    MetricSeries::DiskActiveTimePct,
                    &DeviceId::new(disk_b),
                    1,
                ))
                .expect("device activity query for the gap disk")
        ),
        [None],
        "a field-level failure must not fabricate 0% into the device window"
    );
    assert!(
        reader
            .resolve_series(ChartSeriesQuery::device(
                MetricSeries::DiskActiveTimePct,
                &DeviceId::new("disk:wwid:never-seen"),
                1,
            ))
            .expect("unknown device resolves to an empty window")
            .is_empty()
    );

    // Every host-capable series keeps the legacy host entry as an equal
    // thin wrapper over the same dispatch (classified: a gap window is
    // `NaN`, which never equals itself by value).
    for series in MetricSeries::ALL {
        if matches!(
            series.scope(),
            SeriesScope::Host | SeriesScope::HostAndDevice(_)
        ) {
            assert_eq!(
                classify(
                    &reader
                        .resolve_series(ChartSeriesQuery::host(series))
                        .expect("host-capable series resolves in its host domain")
                ),
                classify(&reader.series(series)),
                "legacy host entry must stay the same window for {series:?}"
            );
        }
    }
}

#[test]
fn wrong_domain_queries_reject_instead_of_redirecting() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    ingestor
        .ingest_correlated_cpu(stamp_at(1, 10), &cpu(42.0))
        .expect("cpu observation");
    let reader = LiveGraphHistory::from_store(store, 8);
    let any_device = DeviceId::new("disk:wwid:some-identity");

    // A device-domain series has no host window: explicit rejection, and the
    // legacy infallible entry carries the documented empty window.
    assert_eq!(
        reader.resolve_series(ChartSeriesQuery::host(MetricSeries::DiskActiveTimePct)),
        Err(ChartSeriesError::MissingDeviceIdentity {
            series: MetricSeries::DiskActiveTimePct,
            domain: DeviceDomain::Storage,
        })
    );
    assert!(
        reader.series(MetricSeries::DiskActiveTimePct).is_empty(),
        "the legacy host entry reports the device-only series as no host window"
    );

    // Every host-only series rejects a device identity instead of resolving
    // it against a per-device ring.
    for series in MetricSeries::ALL {
        if series.scope() == SeriesScope::Host {
            assert_eq!(
                reader.resolve_series(ChartSeriesQuery::device(series, &any_device, 1)),
                Err(ChartSeriesError::DeviceIdentityOnHostSeries { series }),
                "host-only series {series:?} must reject a device identity"
            );
        }
    }
}

#[test]
fn every_variant_declares_its_scope_and_slots_derive_from_all() {
    for (series, scope) in [
        (MetricSeries::CpuUsagePercent, SeriesScope::Host),
        (MetricSeries::MemoryUsagePercent, SeriesScope::Host),
        (MetricSeries::CpuTemperatureC, SeriesScope::Host),
        (MetricSeries::CpuFrequencyMhz, SeriesScope::Host),
        (MetricSeries::CpuPowerW, SeriesScope::Host),
        (
            MetricSeries::DiskBytesPerSec,
            SeriesScope::HostAndDevice(DeviceDomain::Storage),
        ),
        (
            MetricSeries::NetworkBytesPerSec,
            SeriesScope::HostAndDevice(DeviceDomain::Network),
        ),
        (
            MetricSeries::GpuUsagePercent,
            SeriesScope::HostAndDevice(DeviceDomain::Gpu),
        ),
        (
            MetricSeries::DiskActiveTimePct,
            SeriesScope::Device(DeviceDomain::Storage),
        ),
    ] {
        assert_eq!(series.scope(), scope, "scope of {series:?}");
    }

    // Slot ordinals are the single derived ordering: every variant maps back
    // onto itself through ALL, and no two variants share a slot.
    let all = MetricSeries::ALL;
    for series in all {
        let slot = series.slot();
        assert!(
            slot < all.len(),
            "series {series:?} must appear in ALL to have a slot"
        );
        assert!(
            matches!(all.get(slot), Some(slot_series) if *slot_series == series),
            "ALL[slot({series:?})] must be {series:?} again"
        );
    }
    for index in 0..all.len() {
        for other in (index + 1)..all.len() {
            assert_ne!(
                all[index].slot(),
                all[other].slot(),
                "slots of {:?} and {:?} must stay distinct",
                all[index],
                all[other]
            );
        }
    }
}

#[test]
fn device_leg_resolves_only_the_current_device_generation() {
    let device_id = "disk:wwid:scoped-generation";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let observation = |generation, activity, observed_at_ms| {
        StorageTelemetryObservation::current(
            vec![disk(
                device_id,
                generation,
                Some(activity),
                Some(1),
                Some(1),
            )],
            observed_at_ms,
            Vec::new(),
            Vec::new(),
            present_lifecycles(&[(device_id, generation)]),
        )
    };
    ingestor
        .ingest_correlated_storage(stamp_at(1, 10), &observation(1, 40.0, 10))
        .expect("first generation activity observation");
    ingestor
        .ingest_correlated_storage(stamp_at(2, 20), &observation(2, 5.0, 20))
        .expect("reattached generation activity observation");

    let reader = LiveGraphHistory::from_store(store, 8);
    let window = reader
        .resolve_series(ChartSeriesQuery::device(
            MetricSeries::DiskActiveTimePct,
            &DeviceId::new(device_id),
            2,
        ))
        .expect("device activity query");
    assert_eq!(classify(&window), [Some(5.0)]);
    assert_eq!(window, reader.disk_active_time_pct_for(device_id, 2));
}

/// The disk leak probe. The batch-boundary transient the GPU fix (54753a57)
/// closed for typed points, proven for the storage rings: the projection row
/// has advanced to generation 2 while the store still holds the generation-1
/// ring (the reattach observation has not been ingested yet). Every disk
/// read edge — summed, both split directions, and the activity ring through
/// the series resolver — must refuse the stale ring and the unbound `0`
/// alike, while the ring's own generation still serves its samples.
#[test]
fn disk_series_reads_break_the_window_across_a_generation_boundary() {
    let device_id = "disk:wwid:leak-probe";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    ingestor
        .ingest_correlated_storage(
            stamp_at(1, 10),
            &StorageTelemetryObservation::current(
                vec![disk(device_id, 1, Some(40.0), Some(3), Some(4))],
                10,
                Vec::new(),
                Vec::new(),
                present_lifecycles(&[(device_id, 1)]),
            ),
        )
        .expect("generation-1 disk observation");

    let reader = LiveGraphHistory::from_store(store, 8);
    // The viewed row sits at generation 2; the ring was reset for 1.
    assert!(
        reader.disk_bytes_per_sec_for(device_id, 2).is_empty(),
        "a stale ring generation must not leak into the new device view"
    );
    assert!(
        reader.disk_read_bytes_per_sec_for(device_id, 2).is_empty(),
        "the read-direction companion must refuse the same stale ring"
    );
    assert!(
        reader.disk_write_bytes_per_sec_for(device_id, 2).is_empty(),
        "the write-direction companion must refuse the same stale ring"
    );
    assert!(
        reader.disk_active_time_pct_for(device_id, 2).is_empty(),
        "the activity ring must refuse the same stale ring"
    );
    assert!(
        reader
            .resolve_series(ChartSeriesQuery::device(
                MetricSeries::DiskBytesPerSec,
                &DeviceId::new(device_id),
                2,
            ))
            .expect("scope-valid device query")
            .is_empty(),
        "the series resolver must carry the same generation discipline"
    );
    // An unbound projection row (`0`) inherits no ring's samples.
    assert!(
        reader.disk_bytes_per_sec_for(device_id, 0).is_empty(),
        "an unbound generation must not inherit any ring's samples"
    );
    // The ring's own generation still serves its accepted samples.
    assert_eq!(
        classify(&reader.disk_bytes_per_sec_for(device_id, 1)),
        [Some(7.0)]
    );
    assert_eq!(
        classify(&reader.disk_read_bytes_per_sec_for(device_id, 1)),
        [Some(3.0)]
    );
    assert_eq!(
        classify(&reader.disk_write_bytes_per_sec_for(device_id, 1)),
        [Some(4.0)]
    );
}

/// The network leak probe — the adapter twin of the disk probe: the summed
/// lane and both split directions refuse a row/ring generation mismatch and
/// an unbound `0`, and serve only the ring's own generation.
#[test]
fn network_series_reads_break_the_window_across_a_generation_boundary() {
    let device_id = "network:leak-probe";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    ingestor
        .ingest_correlated_network(
            stamp_at(1, 10),
            &NetworkTelemetryObservation::current(
                vec![adapter(device_id, 1, Some(5), Some(6))],
                10,
                Vec::new(),
                Vec::new(),
                present_lifecycles(&[(device_id, 1)]),
            ),
        )
        .expect("generation-1 network observation");

    let reader = LiveGraphHistory::from_store(store, 8);
    assert!(
        reader.network_bytes_per_sec_for(device_id, 2).is_empty(),
        "a stale ring generation must not leak into the new adapter view"
    );
    assert!(
        reader.network_rx_bytes_per_sec_for(device_id, 2).is_empty(),
        "the receive-direction companion must refuse the same stale ring"
    );
    assert!(
        reader.network_tx_bytes_per_sec_for(device_id, 2).is_empty(),
        "the transmit-direction companion must refuse the same stale ring"
    );
    assert!(
        reader.network_bytes_per_sec_for(device_id, 0).is_empty(),
        "an unbound generation must not inherit any ring's samples"
    );
    assert_eq!(
        classify(&reader.network_bytes_per_sec_for(device_id, 1)),
        [Some(11.0)]
    );
    assert_eq!(
        classify(&reader.network_rx_bytes_per_sec_for(device_id, 1)),
        [Some(5.0)]
    );
    assert_eq!(
        classify(&reader.network_tx_bytes_per_sec_for(device_id, 1)),
        [Some(6.0)]
    );
}

/// The GPU utilization and engine legs join the typed-point read's
/// generation discipline: one leak probe covers all three per-device GPU
/// read edges — the utilization ring, the per-engine ring, and the already
/// generation-scoped typed-point window — so no leg can regress alone.
#[test]
fn gpu_device_legs_share_one_generation_discipline() {
    let device_id = "gpu:leak-probe";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let mut probe = gpu(device_id, 1, Some(55.0));
    probe.engines = vec![taskmanager_core::GpuEngine {
        name: "Graphics".to_owned(),
        usage_pct: 62.0,
        ..Default::default()
    }];
    ingestor
        .ingest_correlated_gpu(
            stamp_at(1, 10),
            &GpuTelemetryObservation::current(
                vec![probe],
                10,
                Vec::new(),
                Vec::new(),
                present_lifecycles(&[(device_id, 1)]),
            ),
        )
        .expect("generation-1 gpu observation");

    let reader = LiveGraphHistory::from_store(store, 8);
    for stale in [2, 0] {
        assert!(
            reader.gpu_usage_pct_for(device_id, stale).is_empty(),
            "the utilization leg must refuse generation {stale}"
        );
        assert!(
            reader
                .gpu_engine_usage_pct_for(device_id, stale, "Graphics")
                .is_empty(),
            "the engine leg must refuse generation {stale}"
        );
        assert!(
            reader
                .gpu_metric_point_series_for(device_id, stale, |point| point.utilization_pct)
                .is_empty(),
            "the typed-point leg must refuse generation {stale}"
        );
    }
    assert_eq!(
        classify(&reader.gpu_usage_pct_for(device_id, 1)),
        [Some(55.0)]
    );
    let utilization_points =
        reader.gpu_metric_point_series_for(device_id, 1, |point| point.utilization_pct);
    assert_eq!(utilization_points.len(), 1);
    assert_eq!(utilization_points[0], 55.0);
    assert_eq!(
        classify(&reader.gpu_engine_usage_pct_for(device_id, 1, "Graphics")),
        [Some(62.0)]
    );
}

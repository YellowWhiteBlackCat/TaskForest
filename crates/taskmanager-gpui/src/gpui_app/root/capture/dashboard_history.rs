//! Deterministic typed facts for the dashboard capture history fixture.

use std::collections::BTreeMap;
use std::sync::Arc;

use taskmanager_core::core::{
    CpuMetrics, CpuScalarObservations, DeviceGeneration, DeviceId, DeviceLifecycle, DevicePresence,
    DeviceState, DiskMetrics, DiskScalarObservations, MemoryMetrics, NetworkAdapterType,
    NetworkMetrics, NetworkScalarObservations, NetworkWirelessObservations, ScalarObservation,
};
use taskmanager_telemetry_store::{
    CorrelatedSystemTelemetryHistory, CorrelatedSystemTelemetryIngestor, CorrelatedTelemetryStamp,
    SystemHistoryDomain,
};

/// Feed deterministic capture facts through the production correlation-gated store.
pub(super) fn seed(
    history: &CorrelatedSystemTelemetryHistory,
    ingestor: &CorrelatedSystemTelemetryIngestor,
    newest_timestamp_ms: u64,
) -> bool {
    const SAMPLE_COUNT: u64 = 241;
    const STEP_MS: u64 = 15_000;
    const WINDOW_MS: u64 = 60 * 60 * 1_000;
    const DISK_ID: &str = "capture:disk:dashboard";
    const NETWORK_ID: &str = "capture:network:dashboard";

    let base_revision = [
        SystemHistoryDomain::Cpu,
        SystemHistoryDomain::Memory,
        SystemHistoryDomain::Storage,
        SystemHistoryDomain::Network,
    ]
    .into_iter()
    .filter_map(|domain| {
        history
            .receipts(domain)
            .last()
            .map(|receipt| receipt.stamp.revision())
    })
    .max()
    .unwrap_or(0);
    let first_timestamp_ms = newest_timestamp_ms.saturating_sub(WINDOW_MS);

    for index in 0..SAMPLE_COUNT {
        let Some(revision) = base_revision.checked_add(index.saturating_add(1)) else {
            return false;
        };
        let timestamp_ms = first_timestamp_ms.saturating_add(index.saturating_mul(STEP_MS));
        let Some(stamp) = CorrelatedTelemetryStamp::from_accepted_event(revision, timestamp_ms)
        else {
            return false;
        };
        if ingestor
            .ingest_correlated_cpu(
                stamp,
                &taskmanager_core::core::CpuTelemetryObservation::current(
                    cpu(index, timestamp_ms),
                    timestamp_ms,
                    Vec::new(),
                ),
            )
            .is_err()
        {
            return false;
        }
        if ingestor
            .ingest_correlated_memory(
                stamp,
                &taskmanager_core::core::MemoryTelemetryObservation::current(
                    memory(index),
                    timestamp_ms,
                    Vec::new(),
                ),
            )
            .is_err()
        {
            return false;
        }

        let (disks, disk_lifecycles) = storage(index, timestamp_ms, DISK_ID);
        if ingestor
            .ingest_correlated_storage(
                stamp,
                &taskmanager_core::core::StorageTelemetryObservation::current(
                    disks,
                    timestamp_ms,
                    Vec::new(),
                    Vec::new(),
                    disk_lifecycles,
                ),
            )
            .is_err()
        {
            return false;
        }

        let (networks, network_lifecycles) = network(index, timestamp_ms, NETWORK_ID);
        if ingestor
            .ingest_correlated_network(
                stamp,
                &taskmanager_core::core::NetworkTelemetryObservation::current(
                    networks,
                    timestamp_ms,
                    Vec::new(),
                    Vec::new(),
                    network_lifecycles,
                ),
            )
            .is_err()
        {
            return false;
        }
    }
    true
}

pub(super) fn cpu(index: u64, observed_at_ms: u64) -> CpuMetrics {
    let phase = u16::try_from(index % 24).map_or(0.0, f32::from);
    let value = 24.0 + phase * 1.9;
    CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(value, observed_at_ms),
        ..Default::default()
    })
}

pub(super) fn memory(index: u64) -> MemoryMetrics {
    MemoryMetrics::from_observations(
        taskmanager_core::core::metrics::MemoryScalarObservations {
            total_bytes: ScalarObservation::available(1_000, index),
            used_bytes: ScalarObservation::available(
                480_u64.saturating_add((index % 16).saturating_mul(7)),
                index,
            ),
            ..Default::default()
        },
        Default::default(),
    )
}

pub(super) fn storage(
    index: u64,
    observed_at_ms: u64,
    device_id: &str,
) -> (Vec<DiskMetrics>, BTreeMap<DeviceId, DeviceLifecycle>) {
    let total = (index % 12).saturating_mul(4).saturating_mul(1024 * 1024);
    let read = total / 2;
    let write = total.saturating_sub(read);
    (
        vec![{
            let mut disk = DiskMetrics::default();
            disk.device_id = device_id.to_owned();
            disk.device_generation = DeviceGeneration::new(1);
            disk.device_state = DeviceState::healthy(observed_at_ms);
            disk.apply_scalar_observations(DiskScalarObservations {
                read_bytes_per_sec: ScalarObservation::available(read, observed_at_ms),
                write_bytes_per_sec: ScalarObservation::available(write, observed_at_ms),
                ..Default::default()
            });
            disk
        }],
        lifecycle(device_id, observed_at_ms),
    )
}

pub(super) fn network(
    index: u64,
    observed_at_ms: u64,
    device_id: &str,
) -> (Vec<NetworkMetrics>, BTreeMap<DeviceId, DeviceLifecycle>) {
    let total = (index % 20).saturating_mul(18).saturating_mul(100_000);
    let rx = total / 2;
    let tx = total.saturating_sub(rx);
    (
        vec![{
            let mut network = NetworkMetrics::default();
            network.device_id = Arc::from(device_id);
            network.device_generation = DeviceGeneration::new(1);
            network.device_state = DeviceState::healthy(observed_at_ms);
            network.apply_observations(
                NetworkAdapterType::Unknown,
                NetworkScalarObservations {
                    rx_bytes_per_sec: ScalarObservation::available(rx, observed_at_ms),
                    tx_bytes_per_sec: ScalarObservation::available(tx, observed_at_ms),
                    ..Default::default()
                },
                NetworkWirelessObservations::default(),
            );
            network
        }],
        lifecycle(device_id, observed_at_ms),
    )
}

fn lifecycle(device_id: &str, observed_at_ms: u64) -> BTreeMap<DeviceId, DeviceLifecycle> {
    BTreeMap::from([(
        DeviceId::new(device_id.to_owned()),
        DeviceLifecycle {
            presence: DevicePresence::Present,
            state: DeviceState::healthy(observed_at_ms),
            generation: 1,
            first_seen_ms: Some(observed_at_ms),
            last_seen_ms: Some(observed_at_ms),
            absent_since_ms: None,
        },
    )])
}

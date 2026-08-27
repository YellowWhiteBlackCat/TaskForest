// test-intent: behavior
//! Split-direction storage/network rate series: read/write and rx/tx enter
//! their OWN per-device rings from the SAME accepted observation as the summed
//! lane, keep per-direction availability (a missing counter is an explicit
//! gap, never a fabricated zero), share the summed lane's generation reset and
//! terminal-failure semantics, and are deliberately absent from the
//! persistence mirror — the persisted vocabulary keeps carrying the summed
//! series, so already-written history files and their readers stay valid.

use std::sync::{Arc, Mutex};

use taskmanager_core::{
    DeviceGeneration, DeviceId, DeviceState, DiskMetrics, FailureKind, HistoricalSample,
    HistoryMetric, HistoryRecordSink, HistorySeriesKey, NetworkScalarObservations,
    NetworkTelemetryObservation, ScalarObservation, StorageTelemetryObservation,
};

use super::*;
use crate::TelemetryStore;

/// A disk whose read/write rates are set independently, so one direction can
/// be left `Unknown` (an explicit gap) while the other stays measured.
fn split_disk(
    device_id: &str,
    generation: u64,
    read: ScalarObservation<u64>,
    write: ScalarObservation<u64>,
    observed_at_ms: u64,
) -> DiskMetrics {
    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id(device_id.to_owned())
        .device_generation(DeviceGeneration::new(generation))
        .device_state(DeviceState::healthy(observed_at_ms))
        .scalar_observations(taskmanager_core::DiskScalarObservations {
            read_bytes_per_sec: read,
            write_bytes_per_sec: write,
            ..Default::default()
        })
        .build()
}

fn storage_observation(
    device_id: &str,
    generation: u64,
    read: ScalarObservation<u64>,
    write: ScalarObservation<u64>,
    observed_at_ms: u64,
) -> StorageTelemetryObservation {
    StorageTelemetryObservation::current(
        vec![split_disk(
            device_id,
            generation,
            read,
            write,
            observed_at_ms,
        )],
        observed_at_ms,
        Vec::new(),
        Vec::new(),
        lifecycles(
            device_id,
            taskmanager_core::DevicePresence::Present,
            generation,
        ),
    )
}

fn split_network(
    device_id: &str,
    generation: u64,
    rx: ScalarObservation<u64>,
    tx: ScalarObservation<u64>,
    observed_at_ms: u64,
) -> NetworkTelemetryObservation {
    let network = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .device_id(std::sync::Arc::from(device_id))
        .device_generation(DeviceGeneration::new(generation))
        .device_state(DeviceState::healthy(observed_at_ms))
        .scalar_observations(NetworkScalarObservations {
            rx_bytes_per_sec: rx,
            tx_bytes_per_sec: tx,
            ..Default::default()
        })
        .build();
    NetworkTelemetryObservation::current(
        vec![network],
        observed_at_ms,
        Vec::new(),
        Vec::new(),
        lifecycles(
            device_id,
            taskmanager_core::DevicePresence::Present,
            generation,
        ),
    )
}

/// Map a graph window to the finite/gap classification the charts consume, so
/// assertions read as measured facts instead of NaN bit patterns.
fn classify(samples: Vec<f32>) -> Vec<Option<f32>> {
    samples
        .into_iter()
        .map(|sample| sample.is_finite().then_some(sample))
        .collect()
}

#[test]
fn storage_split_rates_keep_directional_availability_and_generation_resets() {
    let device_id = "disk:wwid:split-storage";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let available = |value: u64, at_ms: u64| ScalarObservation::available(value, at_ms);

    // Tick 1: both directions measured (7 read, 5 write).
    ingestor
        .ingest_correlated_storage(
            stamp_at(1, 10),
            &storage_observation(device_id, 1, available(7, 10), available(5, 10), 10),
        )
        .expect("both-direction tick");
    // Tick 2: write went Unknown — read keeps its evidence, write is a gap,
    // and the summed lane (which needs both) gaps too.
    ingestor
        .ingest_correlated_storage(
            stamp_at(2, 20),
            &storage_observation(
                device_id,
                1,
                available(4, 20),
                ScalarObservation::default(),
                20,
            ),
        )
        .expect("read-only tick");
    // Tick 3: terminal provider failure — every lane advances with a gap.
    ingestor
        .ingest_correlated_unavailable(
            stamp_at(3, 30),
            crate::SystemHistoryDomain::Storage,
            FailureKind::TimedOut,
        )
        .expect("terminal storage failure");

    let values = |history: Option<crate::DeviceMetricHistory<u64>>| {
        history
            .expect("split history exists")
            .samples()
            .into_iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        values(
            store
                .system_history
                .storage_read_rate(&DeviceId::new(device_id))
        ),
        [Some(7), Some(4), None],
        "read keeps its own measured evidence while write is unavailable"
    );
    assert_eq!(
        values(
            store
                .system_history
                .storage_write_rate(&DeviceId::new(device_id))
        ),
        [Some(5), None, None],
        "an unknown write counter is a gap, never a fabricated zero"
    );
    // The summed lane and host total keep their exact legacy timeline.
    assert_eq!(
        values(store.system_history.storage_rate(&DeviceId::new(device_id))),
        [Some(12), None, None]
    );
    assert_eq!(
        store
            .system_history
            .storage_rate_total()
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(12), None, None]
    );

    // Tick 4: a new device generation must reset BOTH split lanes exactly as
    // it resets the summed lane — a reattached disk never inherits the old
    // curve in any direction.
    ingestor
        .ingest_correlated_storage(
            stamp_at(4, 40),
            &storage_observation(device_id, 2, available(1, 40), available(2, 40), 40),
        )
        .expect("reattached generation tick");
    assert_eq!(
        values(
            store
                .system_history
                .storage_read_rate(&DeviceId::new(device_id))
        ),
        [Some(1)]
    );
    assert_eq!(
        values(
            store
                .system_history
                .storage_write_rate(&DeviceId::new(device_id))
        ),
        [Some(2)]
    );
    assert_eq!(
        values(store.system_history.storage_rate(&DeviceId::new(device_id))),
        [Some(3)]
    );

    // The renderer-neutral projection surfaces the split windows as NaN-gap
    // graph vectors, tail-limited by the visible capacity.
    let reader = crate::live_graph::LiveGraphHistory::from_store(store.clone(), 8);
    assert_eq!(
        classify(reader.disk_read_bytes_per_sec_for(device_id, 2)),
        [Some(1.0)]
    );
    assert_eq!(
        classify(reader.disk_write_bytes_per_sec_for(device_id, 2)),
        [Some(2.0)]
    );
    assert_eq!(
        classify(reader.disk_bytes_per_sec_for(device_id, 2)),
        [Some(3.0)],
        "the summed projection is unchanged for its existing consumers"
    );
}

#[test]
fn network_split_rates_keep_directional_availability_and_the_summed_lane() {
    let device_id = "net:mac:00:11:22:33:44:56";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let available = |value: u64, at_ms: u64| ScalarObservation::available(value, at_ms);

    ingestor
        .ingest_correlated_network(
            stamp_at(1, 10),
            &split_network(device_id, 1, available(2, 10), available(3, 10), 10),
        )
        .expect("both-direction tick");
    ingestor
        .ingest_correlated_network(
            stamp_at(2, 20),
            &split_network(
                device_id,
                1,
                ScalarObservation::default(),
                available(6, 20),
                20,
            ),
        )
        .expect("tx-only tick");
    ingestor
        .ingest_correlated_unavailable(
            stamp_at(3, 30),
            crate::SystemHistoryDomain::Network,
            FailureKind::TimedOut,
        )
        .expect("terminal network failure");

    let values = |history: Option<crate::DeviceMetricHistory<u64>>| {
        history
            .expect("split history exists")
            .samples()
            .into_iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        values(
            store
                .system_history
                .network_rx_rate(&DeviceId::new(device_id))
        ),
        [Some(2), None, None],
        "an unknown rx counter is a gap, never a fabricated zero"
    );
    assert_eq!(
        values(
            store
                .system_history
                .network_tx_rate(&DeviceId::new(device_id))
        ),
        [Some(3), Some(6), None],
        "tx keeps its own measured evidence while rx is unavailable"
    );
    assert_eq!(
        values(store.system_history.network_rate(&DeviceId::new(device_id))),
        [Some(5), None, None],
        "the summed per-NIC lane keeps its exact legacy timeline"
    );
    assert_eq!(
        store
            .system_history
            .network_rate_total()
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(5), None, None]
    );

    let reader = crate::live_graph::LiveGraphHistory::from_store(store, 8);
    assert_eq!(
        classify(reader.network_rx_bytes_per_sec_for(device_id, 1)),
        [Some(2.0), None, None]
    );
    assert_eq!(
        classify(reader.network_tx_bytes_per_sec_for(device_id, 1)),
        [Some(3.0), Some(6.0), None]
    );
    assert_eq!(
        classify(reader.network_bytes_per_sec_for(device_id, 1)),
        [Some(5.0), None, None],
        "the summed projection is unchanged for its existing consumers"
    );
}

/// Captures every mirrored record so the persisted vocabulary can be pinned.
#[derive(Default)]
struct CapturingSink {
    records: Mutex<Vec<(HistorySeriesKey, HistoricalSample)>>,
}

impl HistoryRecordSink for CapturingSink {
    fn record_sample(&self, key: HistorySeriesKey, sample: HistoricalSample) {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((key, sample));
    }
}

/// The split lanes are a live-graph projection: the persistence mirror keeps
/// recording ONLY the series it always has (storage activity, the summed
/// per-NIC `network-rate-bps`; storage's rate lane was never persisted). No
/// new series keys reach the sink, so already-written history files and their
/// replay readers stay valid without any migration.
#[test]
fn split_lanes_are_not_mirrored_to_the_persisted_vocabulary() {
    let sink = Arc::new(CapturingSink::default());
    let (_store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let ingestor = ingestor.with_record_sink(sink.clone());
    let available = |value: u64, at_ms: u64| ScalarObservation::available(value, at_ms);

    ingestor
        .ingest_correlated_storage(
            stamp_at(1, 10),
            &storage_observation(
                "disk:wwid:persisted",
                1,
                available(7, 10),
                available(5, 10),
                10,
            ),
        )
        .expect("storage tick with sink attached");
    ingestor
        .ingest_correlated_network(
            stamp_at(1, 10),
            &split_network(
                "net:mac:00:11:22:33:44:57",
                1,
                available(2, 10),
                available(3, 10),
                10,
            ),
        )
        .expect("network tick with sink attached");

    let records = sink
        .records
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let metrics = records
        .iter()
        .map(|(key, _)| key.metric())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        metrics,
        std::collections::HashSet::from([
            HistoryMetric::StorageActivityPct,
            HistoryMetric::NetworkRateBps
        ]),
        "the persisted metric set is exactly the pre-split vocabulary"
    );
    let network_rate_samples = records
        .iter()
        .filter(|(key, _)| key.metric() == HistoryMetric::NetworkRateBps)
        .map(|(_, sample)| sample.value)
        .collect::<Vec<_>>();
    assert_eq!(
        network_rate_samples,
        [Some(5.0)],
        "the summed per-NIC sample is persisted exactly as before"
    );
}

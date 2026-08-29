use super::*;
use taskmanager_core::core::{GpuScalarObservations, GpuTelemetryObservation, ScalarObservation};
use taskmanager_telemetry_store::{CorrelatedTelemetryStamp, TelemetryStore};

fn stamp(revision: u64) -> CorrelatedTelemetryStamp {
    CorrelatedTelemetryStamp::from_accepted_event(revision, revision * 10)
        .expect("test revisions are non-zero")
}

fn gpu_observation(utilization: Option<f32>) -> GpuTelemetryObservation {
    let mut gpu = GpuMetrics::new("gpu:cache", "Fixture GPU");
    gpu.device_generation = DeviceGeneration::new(1);
    let mut observations = GpuScalarObservations::default();
    if let Some(value) = utilization {
        observations.utilization_pct = ScalarObservation::available(value, 10);
    }
    gpu.apply_scalar_observations(observations);
    GpuTelemetryObservation::current(
        vec![gpu],
        10,
        Vec::new(),
        Vec::new(),
        std::collections::BTreeMap::from([(
            DeviceId::new("gpu:cache".to_owned()),
            taskmanager_core::core::DeviceLifecycle {
                presence: taskmanager_core::core::DevicePresence::Present,
                state: taskmanager_core::core::DeviceState::healthy(1),
                generation: 1,
                first_seen_ms: Some(1),
                last_seen_ms: Some(1),
                absent_since_ms: None,
            },
        )]),
    )
}

#[test]
fn device_sample_cache_reuses_until_the_ring_advances() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    ingestor
        .ingest_correlated_gpu(stamp(1), &gpu_observation(Some(42.0)))
        .expect("accepted");

    let generation = DeviceGeneration::new(1);
    let first = gpu_usage_samples(&store.system_history, "gpu:cache", generation);
    assert_eq!(&*first, &[42.0][..]);
    let second = gpu_usage_samples(&store.system_history, "gpu:cache", generation);
    assert!(
        std::rc::Rc::ptr_eq(&first, &second),
        "an unchanged ring must reuse the cached sample vector"
    );

    // A new accepted sample changes the watermark → fresh projection.
    ingestor
        .ingest_correlated_gpu(stamp(2), &gpu_observation(Some(50.0)))
        .expect("accepted");
    let third = gpu_usage_samples(&store.system_history, "gpu:cache", generation);
    assert!(!std::rc::Rc::ptr_eq(&first, &third));
    assert_eq!(&*third, &[42.0, 50.0][..]);
}

// ── split-direction throughput families (read/write, rx/tx) ────────────────

use std::collections::BTreeMap;
use taskmanager_core::core::{
    DevicePresence, DiskScalarObservations, NetworkScalarObservations, NetworkTelemetryObservation,
    StorageTelemetryObservation,
};

fn split_disk(
    device_id: &str,
    generation: u64,
    read: ScalarObservation<u64>,
    write: ScalarObservation<u64>,
    observed_at_ms: u64,
) -> StorageTelemetryObservation {
    let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id(device_id.to_owned())
        .device_generation(DeviceGeneration::new(generation))
        .device_state(taskmanager_core::core::DeviceState::healthy(observed_at_ms))
        .scalar_observations(DiskScalarObservations {
            read_bytes_per_sec: read,
            write_bytes_per_sec: write,
            ..Default::default()
        })
        .build();
    StorageTelemetryObservation::current(
        vec![disk],
        observed_at_ms,
        Vec::new(),
        Vec::new(),
        split_lifecycles(device_id, generation, observed_at_ms),
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
        .device_state(taskmanager_core::core::DeviceState::healthy(observed_at_ms))
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
        split_lifecycles(device_id, generation, observed_at_ms),
    )
}

fn split_lifecycles(
    device_id: &str,
    generation: u64,
    observed_at_ms: u64,
) -> BTreeMap<DeviceId, taskmanager_core::core::DeviceLifecycle> {
    BTreeMap::from([(
        DeviceId::new(device_id),
        taskmanager_core::core::DeviceLifecycle {
            presence: DevicePresence::Present,
            state: taskmanager_core::core::DeviceState::healthy(observed_at_ms),
            generation,
            first_seen_ms: Some(observed_at_ms),
            last_seen_ms: Some(observed_at_ms),
            absent_since_ms: None,
        },
    )])
}

fn classify(samples: &[f32]) -> Vec<Option<f32>> {
    samples
        .iter()
        .map(|sample| sample.is_finite().then_some(*sample))
        .collect()
}

// test-intent: behavior
/// The split-direction disk windows project from their OWN rings in decimal
/// MB/s coordinates, keep per-direction gaps, reuse the cached vector until
/// the ring advances, and never alias the summed lane or each other even
/// though all three families share device, generation, length, and revision
/// at every tick.
#[test]
fn disk_split_windows_keep_direction_identity_and_cache_reuse() {
    let device_id = "disk:wwid:split-cache";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let available = |value: u64, at_ms: u64| ScalarObservation::available(value, at_ms);
    let generation = DeviceGeneration::new(1);

    // Tick 1: both directions measured; tick 2: write counter unknown.
    ingestor
        .ingest_correlated_storage(
            stamp(1),
            &split_disk(
                device_id,
                1,
                available(2_000_000, 10),
                available(500_000, 10),
                10,
            ),
        )
        .expect("both-direction disk tick");
    ingestor
        .ingest_correlated_storage(
            stamp(2),
            &split_disk(
                device_id,
                1,
                available(4_000_000, 20),
                ScalarObservation::default(),
                20,
            ),
        )
        .expect("read-only disk tick");

    let read = storage_read_rate_samples(&store.system_history, device_id, generation);
    let write = storage_write_rate_samples(&store.system_history, device_id, generation);
    let sum = storage_rate_samples(&store.system_history, device_id, generation);
    assert_eq!(
        classify(&read),
        vec![Some(2.0), Some(4.0)],
        "read stays measured across both ticks (MB/s coordinates)"
    );
    assert_eq!(
        classify(&write),
        vec![Some(0.5), None],
        "an unknown write counter is that direction's gap, never a fabricated zero"
    );
    assert_eq!(
        classify(&sum),
        vec![Some(2.5), None],
        "the summed lane keeps its own timeline"
    );

    // Identical watermark across all three families → each cached vector is
    // reused as-is, and the three families never serve one shared entry.
    let read_again = storage_read_rate_samples(&store.system_history, device_id, generation);
    let write_again = storage_write_rate_samples(&store.system_history, device_id, generation);
    let sum_again = storage_rate_samples(&store.system_history, device_id, generation);
    assert!(std::rc::Rc::ptr_eq(&read, &read_again));
    assert!(std::rc::Rc::ptr_eq(&write, &write_again));
    assert!(std::rc::Rc::ptr_eq(&sum, &sum_again));

    // A generation change drops every direction's window together.
    ingestor
        .ingest_correlated_storage(
            stamp(3),
            &split_disk(
                device_id,
                2,
                available(1_000_000, 30),
                available(1_000_000, 30),
                30,
            ),
        )
        .expect("new-generation disk tick");
    let read_new =
        storage_read_rate_samples(&store.system_history, device_id, DeviceGeneration::new(2));
    assert_eq!(classify(&read_new), vec![Some(1.0)]);
    assert_eq!(
        classify(&storage_read_rate_samples(
            &store.system_history,
            device_id,
            generation,
        )),
        Vec::<Option<f32>>::new(),
        "the stale generation's window is empty, not a stale trace"
    );
}

// test-intent: behavior
/// The rx/tx windows mirror the disk split semantics on the network family:
/// per-direction gaps, MB/s coordinates, and cache reuse until the ring
/// advances.
#[test]
fn network_split_windows_keep_direction_identity() {
    let device_id = "net:mac:split-cache";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let available = |value: u64, at_ms: u64| ScalarObservation::available(value, at_ms);
    let generation = DeviceGeneration::new(1);

    ingestor
        .ingest_correlated_network(
            stamp(1),
            &split_network(
                device_id,
                1,
                available(3_000_000, 10),
                available(6_000_000, 10),
                10,
            ),
        )
        .expect("both-direction network tick");
    ingestor
        .ingest_correlated_network(
            stamp(2),
            &split_network(
                device_id,
                1,
                ScalarObservation::default(),
                available(12_000_000, 20),
                20,
            ),
        )
        .expect("tx-only network tick");

    let rx = network_rx_rate_samples(&store.system_history, device_id, generation);
    let tx = network_tx_rate_samples(&store.system_history, device_id, generation);
    let sum = network_rate_samples(&store.system_history, device_id, generation);
    assert_eq!(
        classify(&rx),
        vec![Some(3.0), None],
        "an unknown rx counter is that direction's gap"
    );
    assert_eq!(classify(&tx), vec![Some(6.0), Some(12.0)]);
    assert_eq!(classify(&sum), vec![Some(9.0), None]);

    let rx_again = network_rx_rate_samples(&store.system_history, device_id, generation);
    assert!(std::rc::Rc::ptr_eq(&rx, &rx_again));
    assert!(!std::rc::Rc::ptr_eq(&rx, &tx));
    assert!(!std::rc::Rc::ptr_eq(&rx, &sum));
}

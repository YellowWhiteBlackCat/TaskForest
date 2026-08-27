use super::*;
use crate::core::{FailureKind, ProcessScalarObservations, ScalarObservation};

impl ProcessHistorySample {
    const fn measured(cpu: f32, memory: f32, disk_read: f32, disk_write: f32) -> Self {
        Self {
            cpu: Some(cpu),
            memory: Some(memory),
            disk_read: Some(disk_read),
            disk_write: Some(disk_write),
        }
    }
}

fn sample(value: f32) -> ProcessHistorySample {
    ProcessHistorySample::measured(value, value * 1e6, value, value * 2.0)
}

#[test]
fn one_trajectory_guards_capacity_window_identity_and_stale_pruning() {
    let mut store = ProcessHistoryStore::default();
    let pid = 42;
    let first_identity = 100;

    for value in 1..=PROCESS_HISTORY_MAX_SAMPLES {
        store.begin_refresh(Duration::ZERO);
        let snapshot = store.record(pid, Some(first_identity), sample(value as f32));
        assert_eq!(snapshot.cpu.len(), value);
        store.finish_refresh();
    }
    store.begin_refresh(Duration::ZERO);
    let capped = store.record(pid, Some(first_identity), sample(999.0));
    assert_eq!(capped.cpu.len(), PROCESS_HISTORY_MAX_SAMPLES);
    assert_eq!(capped.cpu.first(), Some(&2.0));
    assert_eq!(capped.cpu.last(), Some(&999.0));

    store.begin_refresh(Duration::from_secs(60));
    let boundary = store.record(pid, Some(first_identity), sample(1_000.0));
    // The sample exactly 60s old is still inside the inclusive time window,
    // but the hard 121-sample capacity remains authoritative and evicts it
    // before appending the new point.
    assert_eq!(boundary.cpu.first(), Some(&3.0));
    store.begin_refresh(Duration::from_millis(60_001));
    let evicted = store.record(pid, Some(first_identity), sample(1_001.0));
    assert_eq!(evicted.cpu, vec![1_000.0, 1_001.0]);

    store.begin_refresh(Duration::from_secs(61));
    let replaced = store.record(pid, Some(200), sample(2_000.0));
    assert_eq!(replaced.cpu, vec![2_000.0]);
    store.begin_refresh(Duration::from_secs(62));
    let unknown = store.record(pid, None, sample(3_000.0));
    assert_eq!(unknown.cpu, vec![3_000.0]);
    store.begin_refresh(Duration::from_secs(63));
    let still_unknown = store.record(pid, None, sample(4_000.0));
    assert_eq!(still_unknown.cpu, vec![4_000.0]);

    for age in 1..=PROCESS_HISTORY_STALE_TICKS {
        store.begin_refresh(Duration::from_secs(63 + age));
        store.finish_refresh();
        assert!(store.rings.contains_key(&pid));
    }
    store.begin_refresh(Duration::from_secs(67));
    store.finish_refresh();
    assert!(!store.rings.contains_key(&pid));
}

#[test]
fn typed_unavailable_channels_remain_gaps_while_measured_zero_survives() {
    let mut process = ProcessItem::new(7, "fixture");
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(600, 10),
        cpu_percentage: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        memory_bytes: ScalarObservation::available(4096, 10),
        disk_read_bytes_per_sec: ScalarObservation::unavailable(
            FailureKind::TemporarilyUnavailable,
        ),
        disk_write_bytes_per_sec: ScalarObservation::available(0, 10),
        ..ProcessScalarObservations::default()
    });

    let mut store = ProcessHistoryStore::default();
    store.begin_refresh(Duration::ZERO);
    let history = store.record(
        process.pid,
        process.current_start_token(),
        ProcessHistorySample::from_process(&process),
    );

    assert!(history.cpu.is_empty());
    assert_eq!(history.memory, vec![4096.0]);
    assert!(history.disk.is_empty());
    assert!(history.disk_read.is_empty());
    assert_eq!(history.disk_write, vec![0.0]);
}

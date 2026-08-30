use super::*;
use taskmanager_core::core::process::ProcessLiveKey;

fn identity(pid: u32) -> ProcessLiveKey {
    ProcessLiveKey::from_parts(pid, u64::from(pid)).expect("fixture identity")
}

/// An identity change re-points the per-process window and clears every series,
/// so the sparklines can never blend two processes.
#[test]
fn process_history_resets_on_pid_change_and_rejects_non_finite() {
    let mut history = ProcessPerfHistory::new(60);
    history.push(identity(100), Some(10.0), Some(1_024), Some(5), Some(7));
    assert_eq!(history.identity(), Some(identity(100)));
    assert_eq!(history.cpu_samples(), vec![10.0]);

    history.push(identity(100), Some(20.0), None, None, None);
    assert_eq!(history.cpu_samples(), vec![10.0, 20.0]);
    let memory = history.memory_samples();
    assert_eq!(memory.len(), 2, "None keeps the time slot");
    assert_eq!(memory[0], 1_024.0);
    assert!(memory[1].is_nan(), "the missing sample remains a gap");

    history.push(identity(200), Some(50.0), Some(2_048), None, None);
    assert_eq!(history.identity(), Some(identity(200)));
    assert_eq!(history.cpu_samples(), vec![50.0], "old identity cleared");
    assert_eq!(history.memory_samples(), vec![2_048.0]);
    let disk_read = history.disk_read_samples();
    assert_eq!(disk_read.len(), 1);
    assert!(disk_read[0].is_nan(), "the missing sample remains a gap");

    // A new identity with an empty optional channel still records its time slot.
    let mut fresh = ProcessPerfHistory::new(10);
    fresh.push(identity(1), Some(1.0), None, None, None);
    assert_eq!(fresh.identity(), Some(identity(1)));
    assert_eq!(fresh.cpu_samples(), vec![1.0]);
}

/// Seeding from the provider windows reproduces them oldest-first and
/// re-points the identity (G-14): whatever the ring held for a previous
/// process is gone, and the provider tail becomes the rendered history.
#[test]
fn seed_from_provider_repoints_and_replays_the_newest_tail() {
    let mut history = ProcessPerfHistory::new(10);
    history.push(identity(100), Some(1.0), Some(10), Some(1), Some(1));
    assert_eq!(history.identity(), Some(identity(100)));

    let cpu: Vec<f32> = (0..14).map(|value| value as f32).collect();
    let memory: Vec<f32> = vec![1_000.0, 2_000.0, f32::NAN, 4_000.0];
    history.seed_from_provider(identity(200), 10, &cpu, &memory, &[], &[7.0, 9.0]);

    assert_eq!(
        history.identity(),
        Some(identity(200)),
        "seed re-points the tracked identity"
    );
    // The newest 10 of the 14-sample cpu window, oldest-first.
    assert_eq!(
        history.cpu_samples(),
        (4..14).map(|value| value as f32).collect::<Vec<_>>()
    );
    // A non-finite provider sample remains a gap while finite ones keep order.
    let memory = history.memory_samples();
    assert_eq!(memory.len(), 4);
    assert_eq!(&memory[..2], &[1_000.0, 2_000.0]);
    assert!(memory[2].is_nan());
    assert_eq!(memory[3], 4_000.0);
    assert_eq!(history.disk_write_samples(), vec![7.0, 9.0]);
    assert!(
        history.disk_read_samples().is_empty(),
        "empty window seeds nothing"
    );
    assert!(!history.is_empty());
}

/// A capacity clamp applies to the seed too, and an all-empty seed leaves
/// the honest empty ring (mac/win provider fallback).
#[test]
fn seed_from_provider_clamps_capacity_and_stays_empty_without_provider_data() {
    let mut history = ProcessPerfHistory::new(60);
    history.seed_from_provider(identity(300), 100_000, &[1.0, 2.0], &[3.0], &[4.0], &[5.0]);
    assert!(
        history.capacity() <= MAX_HISTORY_CAPACITY,
        "a hostile capacity cannot allocate an unbounded ring"
    );
    assert_eq!(history.cpu_samples().len(), 2);

    let mut fallback = ProcessPerfHistory::new(60);
    fallback.push(identity(400), Some(1.0), Some(1), Some(1), Some(1));
    fallback.seed_from_provider(identity(400), 60, &[], &[], &[], &[]);
    assert!(
        fallback.is_empty(),
        "an empty provider seed clears the ring, never fabricates history"
    );
}

/// Seeding and then live-sampling appends: the provider history stays at
/// the head of the series and the live sample lands at the tail (the
/// extension contract of G-14).
#[test]
fn live_sampling_extends_a_seeded_ring_in_arrival_order() {
    let mut history = ProcessPerfHistory::new(10);
    history.seed_from_provider(identity(500), 10, &[10.0, 20.0, 30.0], &[], &[], &[]);
    history.push(identity(500), Some(40.0), Some(999), None, None);
    assert_eq!(history.cpu_samples(), vec![10.0, 20.0, 30.0, 40.0]);
    assert_eq!(history.memory_samples(), vec![999.0]);
}

//! Process affinity mutation capability and lane-isolation contracts.

use super::*;

#[test]
fn slow_batch_control_cannot_block_affinity_mutation_lane() {
    let process_control_started = Arc::new(AtomicBool::new(false));
    let handle = spawn_complete(fake_registry(FakeProvider {
        process_control_delay: Duration::from_millis(200),
        process_control_started: process_control_started.clone(),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let target = frozen_process(42);
    let batch_id = submit_process_control(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_CONTROL,
        ProcessControlRequest::ExecuteBatch(taskmanager_core::ProcessBatchIntent {
            action: taskmanager_core::ProcessBatchAction::Suspend,
            scope: Default::default(),
            targets: vec![target.clone()],
        }),
    );
    for _ in 0..100 {
        if process_control_started.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(process_control_started.load(Ordering::Acquire));

    let started = Instant::now();
    let affinity_id = submit_process_affinity_control(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_AFFINITY_CONTROL,
        ProcessAffinityControlRequest {
            target,
            cpus: vec![0, 1],
        },
    );
    let affinity = wait_event(&handle);
    assert_eq!(affinity.request_id, affinity_id);
    assert_eq!(affinity.capability, CapabilityId::PROCESS_AFFINITY_CONTROL);
    assert_eq!(
        affinity.provider,
        Some(fixture_process_provider(&affinity.capability))
    );
    assert!(matches!(
        affinity.outcome,
        Ok(PlatformEvent::Processes(
            ProcessEvent::AffinityApplied { .. }
        ))
    ));
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "affinity mutation queued behind batch control for {:?}",
        started.elapsed()
    );
    let batch = wait_event(&handle);
    assert_eq!(batch.request_id, batch_id);
    assert_eq!(
        batch.provider,
        Some(fixture_process_provider(&batch.capability))
    );
}

use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessBatchTargetResult,
};

use super::ProcessBatchWorker;

#[test]
fn worker_round_trips_a_rejected_target_result() {
    let worker = ProcessBatchWorker::new();
    // pid beyond the kernel pid space: /proc/<pid> can never exist on any
    // host, so the start-token validation must reject without signalling.
    let ghost = FrozenProcessIdentity::from_authoritative_parts(u32::MAX, "no-such-process", 1, 1)
        .expect("fixture identity");
    worker
        .submit(ProcessBatchIntent {
            action: ProcessBatchAction::Resume,
            scope: Default::default(),
            targets: vec![ghost],
        })
        .expect("worker accepts one intent");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let result = loop {
        if let Some(result) = worker.try_recv() {
            break result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not answer within 5s"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert_eq!(result.targets.len(), 1);
    assert!(
        !matches!(result.targets[0].1, ProcessBatchTargetResult::Applied),
        "a pid outside the host pid space must never be signalled; got {:?}",
        result.targets[0].1
    );
}

#[test]
fn worker_accepts_one_intent_at_a_time() {
    let worker = ProcessBatchWorker::new();
    let ghost = FrozenProcessIdentity::from_authoritative_parts(u32::MAX, "ghost-a", 1, 1)
        .expect("fixture identity");
    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::End,
        scope: Default::default(),
        targets: vec![ghost],
    };

    // The bounded lane holds one in-flight request; a second submit while
    // the first is still processing is either queued (the worker drained
    // it) or rejected Busy — never lost and never accepted twice.
    let first = worker.submit(intent.clone());
    let second = worker.submit(intent);
    assert!(first.is_ok(), "first intent must enter the bounded lane");
    if let Err(error) = second {
        assert_eq!(
            error,
            super::ProcessBatchSubmitError::Busy,
            "overflow must be the typed Busy, not a silent drop"
        );
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while worker.try_recv().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not answer within 5s"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

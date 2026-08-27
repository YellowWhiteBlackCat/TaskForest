use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::bounded;
use taskmanager_application::{
    HistoryReplayCompletionOutcome, HistoryReplayController, HistoryReplayErrorKind,
};
use taskmanager_core::{HistoricalSample, HistoryMetric, HistorySeriesKey};
use taskmanager_history_store::{PersistentHistoryStore, RecordSampleOutcome, RetentionPolicy};

use super::*;

#[test]
fn worker_completion_is_correlated_and_command_backpressure_is_typed() {
    let (started_tx, started_rx) = bounded(1);
    let (release_tx, release_rx) = bounded(1);
    let loader = Arc::new(move |_request, _now_ms| {
        let _ = started_tx.try_send(());
        let _ = release_rx.recv();
        Ok(Arc::from([]))
    });
    let coordinator = HistoryReplayCoordinator::start_with_loader(loader).expect("start worker");
    let mut blocked_client = coordinator.client();
    let mut queued_client = coordinator.client();
    let mut rejected_client = coordinator.client();
    let mut controller = HistoryReplayController::default();

    let first = controller.open().expect("open replay");
    blocked_client
        .try_request(first)
        .expect("accept first query");
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker entered loader");

    for _ in 0..HISTORY_REPLAY_COMMAND_CAPACITY {
        queued_client
            .try_request(controller.refresh().expect("refresh open replay"))
            .expect("fill bounded command lane");
    }
    let error = rejected_client
        .try_request(controller.refresh().expect("refresh open replay"))
        .expect_err("full command lane must reject without blocking");
    assert_eq!(error.kind(), HistoryReplayErrorKind::Backpressure);

    release_tx.send(()).expect("release loader");
    let completion = blocked_client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("correlated completion");
    assert_eq!(completion.request, first);
    assert!(matches!(
        completion.outcome,
        HistoryReplayCompletionOutcome::Loaded(ref rows) if rows.is_empty()
    ));

    drop(release_tx);
    drop(rejected_client);
    drop(queued_client);
    drop(blocked_client);
    drop(coordinator);
}

#[test]
fn client_completion_lane_has_an_independent_hard_bound() {
    let coordinator =
        HistoryReplayCoordinator::start_with_loader(Arc::new(|_, _| Ok(Arc::from([]))))
            .expect("start worker");
    let mut client = coordinator.client();
    let mut controller = HistoryReplayController::default();
    let _ = controller.open().expect("open replay");

    for _ in 0..HISTORY_REPLAY_COMPLETION_CAPACITY {
        client
            .try_request(controller.refresh().expect("refresh open replay"))
            .expect("accept within client bound");
    }
    let error = client
        .try_request(controller.refresh().expect("refresh open replay"))
        .expect_err("client must reject before its completion lane can overflow");
    assert_eq!(error.kind(), HistoryReplayErrorKind::Backpressure);
}

#[test]
fn blocked_read_cannot_block_the_bounded_shutdown_seam() {
    let (started_tx, started_rx) = bounded(1);
    let (release_tx, release_rx) = bounded(1);
    let coordinator = HistoryReplayCoordinator::start_with_loader(Arc::new(move |_, _| {
        let _ = started_tx.try_send(());
        let _ = release_rx.recv();
        Ok(Arc::from([]))
    }))
    .expect("start worker");
    let mut client = coordinator.client();
    let mut controller = HistoryReplayController::default();
    let request = controller.open().expect("open replay");
    client.try_request(request).expect("submit blocked query");
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker entered blocked loader");

    let (dropped_tx, dropped_rx) = bounded(1);
    let drop_join = std::thread::spawn(move || {
        drop(client);
        drop(coordinator);
        let _ = dropped_tx.try_send(());
    });
    dropped_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("last-handle drop must not wait forever for read I/O");
    drop_join.join().expect("bounded drop thread");

    release_tx.send(()).expect("release detached reader");
}

#[test]
fn storage_query_is_published_as_bounded_honest_rows() {
    static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let fixture_id = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp/app-host-history-replay")
        .join(format!("{}-{fixture_id}", std::process::id()));
    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(u64::MAX, u64::MAX),
        |_| false,
    )
    .expect("open history fixture");
    let key = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
    for sample in [
        HistoricalSample {
            revision: 1,
            completed_at_ms: 1_000,
            measured_at_ms: Some(1_000),
            value: Some(41.0),
        },
        HistoricalSample {
            revision: 2,
            completed_at_ms: 2_000,
            measured_at_ms: None,
            value: None,
        },
    ] {
        assert_eq!(
            store.try_record_sample(key.clone(), sample),
            RecordSampleOutcome::Accepted
        );
    }
    store.flush(2_000).expect("flush history fixture");
    drop(store);

    let mut controller = HistoryReplayController::default();
    let request = controller.open().expect("open replay");
    let rows = query_rows(&HistoryQuery::new(&root), request, 2_000).expect("query fixture");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].peak_value, Some(41.0));
    assert_eq!(rows[0].observed, 1);
    assert_eq!(rows[0].gaps, 1);
    assert_eq!(rows[0].samples.len(), 2);
    assert_eq!(rows[0].sample_times_ms.as_ref(), &[1_000, 2_000]);
    assert!(rows[0].samples[1].is_nan());

    std::fs::remove_dir_all(&root).expect("remove exact history fixture");
}

#[test]
fn loader_panic_resolves_requests_and_never_masquerades_as_backpressure() {
    let (entered_tx, entered_rx) = bounded(1);
    let (release_tx, release_rx) = bounded(1);
    let loader = Arc::new(move |_request, _now_ms| {
        let _ = entered_tx.try_send(());
        let _ = release_rx.recv();
        panic!("fixture loader fault");
    });
    let coordinator = HistoryReplayCoordinator::start_with_loader(loader).expect("start worker");
    let mut client = coordinator.client();
    let mut controller = HistoryReplayController::default();
    let faulted = controller.open().expect("open replay");
    client.try_request(faulted).expect("submit faulted query");
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker entered the faulting loader");

    // Fill the client's completion budget behind the blocked query so a
    // leaking implementation stays pinned at the bound and reports
    // Backpressure instead of the dead lane's typed stop.
    for _ in 1..HISTORY_REPLAY_COMPLETION_CAPACITY {
        client
            .try_request(controller.refresh().expect("refresh open replay"))
            .expect("queue behind the faulted query");
    }
    release_tx.send(()).expect("release the faulting loader");

    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("faulted query still resolves with a terminal completion");
    assert_eq!(completion.request, faulted);
    let error = match completion.outcome {
        HistoryReplayCompletionOutcome::Failed(error) => error,
        outcome => panic!("faulted query must not report {outcome:?}"),
    };
    assert_eq!(error.kind(), HistoryReplayErrorKind::WorkerStopped);
    assert!(error.detail().contains("fixture loader fault"));

    // The dead lane admits no zombie work: admission converges on the typed
    // stop even while stranded credits still occupy the completion budget.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let error = client
            .try_request(controller.refresh().expect("refresh open replay"))
            .expect_err("dead lane must reject admission");
        if error.kind() == HistoryReplayErrorKind::WorkerStopped {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "lane never reported its typed stop (last error: {error:?})"
        );
        std::thread::yield_now();
    }
    assert!(client.drain().is_empty());
    drop(client);
    drop(coordinator);
}

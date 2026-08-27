use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::bounded;
use taskmanager_application::snapshot_export::{
    SnapshotExportController, SnapshotExportErrorKind, SnapshotExportOutcome,
    SnapshotExportPayload, SnapshotExportState, SnapshotExportTarget,
};
use taskmanager_core::{ProcessItem, SystemSnapshot};

use super::*;

fn request(
    target: SnapshotExportTarget,
) -> taskmanager_application::snapshot_export::SnapshotExportRequest {
    let mut controller = SnapshotExportController::new();
    controller
        .begin(SnapshotExportPayload::new(
            SystemSnapshot::default(),
            Arc::<[ProcessItem]>::from([ProcessItem::new(7, "worker")]),
            target,
        ))
        .expect("request")
}

fn test_directory(label: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp")
        .join(format!(
            "taskforest-snapshot-export-{label}-{}-{stamp}",
            std::process::id()
        ))
}

#[test]
fn app_host_worker_writes_all_three_artifacts_off_the_caller_thread() {
    let directory = test_directory("real-write");
    std::fs::create_dir(&directory).expect("create isolated directory");
    let base = directory.join("snapshot");
    let coordinator = SnapshotExportCoordinator::start().expect("worker");
    let mut client = coordinator.client();
    let mut controller = SnapshotExportController::new();
    let request = controller
        .begin(SnapshotExportPayload::new(
            SystemSnapshot::default(),
            Arc::<[ProcessItem]>::from([ProcessItem::new(7, "worker")]),
            SnapshotExportTarget::base_path(&base),
        ))
        .expect("request");
    client.try_submit(request.clone()).expect("accepted");
    let _ = controller.mark_running(request.id());
    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("completion");
    client.outstanding = client.outstanding.saturating_sub(1);
    assert_eq!(
        controller.complete(completion),
        taskmanager_application::snapshot_export::SnapshotExportDisposition::Applied
    );
    assert!(matches!(
        controller.state(),
        SnapshotExportState::Ready { .. }
    ));
    assert!(
        std::fs::read_to_string(base.with_extension("json"))
            .expect("json")
            .contains("snapshot")
    );
    assert!(
        std::fs::read_to_string(base.with_extension("csv"))
            .expect("csv")
            .contains("worker")
    );
    assert!(
        std::fs::read_to_string(base.with_extension("html"))
            .expect("html")
            .contains("worker")
    );
    drop(client);
    drop(coordinator);
    std::fs::remove_dir_all(&directory).expect("remove isolated directory");
}

#[test]
fn non_regular_destination_is_a_typed_worker_failure() {
    let directory = test_directory("failure");
    std::fs::create_dir(&directory).expect("create isolated directory");
    let base = directory.join("snapshot");
    std::fs::create_dir(base.with_extension("json")).expect("conflicting directory");
    let coordinator = SnapshotExportCoordinator::start().expect("worker");
    let mut client = coordinator.client();
    let request = request(SnapshotExportTarget::base_path(&base));
    client.try_submit(request).expect("accepted");
    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("completion");
    assert!(matches!(
        completion.outcome,
        SnapshotExportOutcome::Failed(ref error)
            if error.kind() == SnapshotExportErrorKind::Inspect
    ));
    drop(client);
    drop(coordinator);
    std::fs::remove_dir_all(&directory).expect("remove isolated directory");
}

#[test]
fn bounded_command_lane_rejects_without_waiting_for_filesystem_work() {
    let (started_tx, started_rx) = bounded(1);
    let (release_tx, release_rx) = bounded(1);
    let coordinator = SnapshotExportCoordinator::start_with_exporter(Arc::new(move |_| {
        let _ = started_tx.try_send(());
        let _ = release_rx.recv();
        SnapshotExportOutcome::Ready {
            base: Arc::from("fixture"),
        }
    }))
    .expect("worker");
    let mut blocked = coordinator.client();
    let mut queued = coordinator.client();
    let mut rejected = coordinator.client();
    blocked
        .try_submit(request(SnapshotExportTarget::current_directory("blocked")))
        .expect("first accepted");
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker blocked");
    for index in 0..SNAPSHOT_EXPORT_COMMAND_CAPACITY {
        queued
            .try_submit(request(SnapshotExportTarget::current_directory(format!(
                "queued-{index}"
            ))))
            .expect("fill lane");
    }
    let error = rejected
        .try_submit(request(SnapshotExportTarget::current_directory("rejected")))
        .expect_err("full lane rejects");
    assert_eq!(error.kind(), SnapshotExportErrorKind::Backpressure);
    release_tx.send(()).expect("release worker");
}

#[test]
fn exporter_panic_resolves_the_request_and_types_the_dead_lane() {
    let coordinator = SnapshotExportCoordinator::start_with_exporter(Arc::new(|_| {
        panic!("fixture exporter fault");
    }))
    .expect("worker");
    let mut client = coordinator.client();
    let faulted = request(SnapshotExportTarget::current_directory("fault"));
    client.try_submit(faulted.clone()).expect("accepted");
    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("faulted export still resolves with a terminal completion");
    assert_eq!(completion.request, faulted.id());
    let error = match completion.outcome {
        SnapshotExportOutcome::Failed(error) => error,
        outcome => panic!("faulted export must not report {outcome:?}"),
    };
    assert_eq!(error.kind(), SnapshotExportErrorKind::WorkerStopped);
    assert!(error.detail().contains("fixture exporter fault"));

    // Admission converges on the dead lane's typed stop. A brief window
    // between the terminal completion and the exit-flag publication can still
    // admit into the doomed lane, so keep probing until the stop is visible.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match client.try_submit(request(SnapshotExportTarget::current_directory("after"))) {
            Err(error) if error.kind() == SnapshotExportErrorKind::WorkerStopped => break,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "lane never reported its typed stop (last error: {error:?})"
                );
                std::thread::yield_now();
            }
            Ok(()) => {
                assert!(
                    Instant::now() < deadline,
                    "lane never reported its typed stop"
                );
                std::thread::yield_now();
            }
        }
    }
    assert!(client.drain().is_empty());
    drop(client);
    drop(coordinator);
}

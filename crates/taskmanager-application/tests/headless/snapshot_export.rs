use std::collections::VecDeque;
use std::sync::Arc;

use super::*;

#[derive(Debug, Default)]
struct FakePort {
    submitted: Vec<SnapshotExportRequest>,
    completions: VecDeque<SnapshotExportCompletion>,
}

impl SnapshotExportPort for FakePort {
    fn try_submit(&mut self, request: SnapshotExportRequest) -> Result<(), SnapshotExportError> {
        self.submitted.push(request);
        Ok(())
    }

    fn drain(&mut self) -> Vec<SnapshotExportCompletion> {
        self.completions.drain(..).collect()
    }
}

fn payload(label: &str) -> SnapshotExportPayload {
    SnapshotExportPayload::new(
        SystemSnapshot::default(),
        Arc::<[ProcessItem]>::from([]),
        SnapshotExportTarget::current_directory(label),
    )
}

#[test]
fn queued_running_ready_lifecycle_is_correlated() {
    let mut controller = SnapshotExportController::new();
    let request = controller.begin(payload("one")).expect("request");
    assert!(matches!(controller.state(), SnapshotExportState::Queued(_)));
    assert_eq!(
        controller.mark_running(request.id()),
        SnapshotExportDisposition::Applied
    );
    assert!(matches!(
        controller.state(),
        SnapshotExportState::Running(_)
    ));
    assert_eq!(
        controller.complete(SnapshotExportCompletion {
            request: request.id(),
            outcome: SnapshotExportOutcome::Ready {
                base: Arc::from("/tmp/one"),
            },
        }),
        SnapshotExportDisposition::Applied
    );
    assert!(matches!(
        controller.state(),
        SnapshotExportState::Ready { base, .. } if base.as_ref() == "/tmp/one"
    ));
}

#[test]
fn active_request_rejects_reentry_and_submission_failure_is_terminal() {
    let mut controller = SnapshotExportController::new();
    let request = controller.begin(payload("one")).expect("request");
    assert!(matches!(
        controller.begin(payload("two")),
        Err(SnapshotExportStartError::Busy(id)) if id == request.id()
    ));
    let error = SnapshotExportError::new(SnapshotExportErrorKind::Backpressure, "full");
    assert_eq!(
        controller.fail_submission(request.id(), error.clone()),
        SnapshotExportDisposition::Applied
    );
    assert!(matches!(
        controller.state(),
        SnapshotExportState::Failed { error: actual, .. } if actual == &error
    ));
}

#[test]
fn invalid_current_directory_target_is_rejected_before_request_allocation() {
    let mut controller = SnapshotExportController::new();
    assert!(matches!(
        controller.begin(payload("../escape")),
        Err(SnapshotExportStartError::InvalidTarget)
    ));
    assert!(matches!(controller.state(), SnapshotExportState::Closed));
    let request = controller
        .begin(payload("valid-stem"))
        .expect("valid request");
    assert_eq!(request.id().get(), 1);
}

#[test]
fn snapshot_targets_keep_explicit_paths_distinct_from_local_names() {
    assert!(SnapshotExportTarget::current_directory("snapshot").is_valid());
    for stem in [
        "",
        ".",
        "..",
        "nested/snapshot",
        r"nested\snapshot",
        "NUL",
        "COM1.txt",
        "trailing-space ",
    ] {
        assert!(!SnapshotExportTarget::current_directory(stem).is_valid());
    }
    assert!(!SnapshotExportTarget::current_directory(&"a".repeat(256)).is_valid());
    assert!(SnapshotExportTarget::current_directory(&"a".repeat(255)).is_valid());
    assert!(SnapshotExportTarget::current_directory("name.with.multiple.extensions").is_valid());
    assert!(SnapshotExportTarget::base_path("../explicit/snapshot").is_valid());
}

#[test]
fn session_turns_invalid_target_into_a_typed_inspection_failure() {
    let mut session = SnapshotExportSession::new(FakePort::default());
    let error = session
        .submit(SnapshotExportPayload::new(
            SystemSnapshot::default(),
            Arc::<[ProcessItem]>::from([]),
            SnapshotExportTarget::current_directory("../escape"),
        ))
        .expect_err("invalid current-directory target");
    assert!(matches!(
        error,
        SnapshotExportSubmitError::Rejected(error)
            if error.kind() == SnapshotExportErrorKind::Inspect
    ));
    assert!(matches!(session.state(), SnapshotExportState::Closed));
    assert!(session.port.submitted.is_empty());
}

#[test]
fn close_makes_late_completion_inert_and_next_request_wins() {
    let mut controller = SnapshotExportController::new();
    let first = controller.begin(payload("one")).expect("first");
    let _ = controller.mark_running(first.id());
    controller.close();
    assert_eq!(
        controller.complete(SnapshotExportCompletion {
            request: first.id(),
            outcome: SnapshotExportOutcome::Ready {
                base: Arc::from("late"),
            },
        }),
        SnapshotExportDisposition::LateIgnored
    );
    let second = controller.begin(payload("two")).expect("second");
    assert!(second.id() > first.id());
}

#[test]
fn duplicate_terminal_cannot_replace_first_terminal() {
    let mut controller = SnapshotExportController::new();
    let request = controller.begin(payload("one")).expect("request");
    let _ = controller.mark_running(request.id());
    assert_eq!(
        controller.complete(SnapshotExportCompletion {
            request: request.id(),
            outcome: SnapshotExportOutcome::Ready {
                base: Arc::from("first"),
            },
        }),
        SnapshotExportDisposition::Applied
    );
    assert_eq!(
        controller.complete(SnapshotExportCompletion {
            request: request.id(),
            outcome: SnapshotExportOutcome::Failed(SnapshotExportError::new(
                SnapshotExportErrorKind::Commit,
                "duplicate",
            )),
        }),
        SnapshotExportDisposition::DuplicateIgnored
    );
    assert!(matches!(
        controller.state(),
        SnapshotExportState::Ready { base, .. } if base.as_ref() == "first"
    ));
}

#[test]
fn error_detail_is_bounded_and_kind_is_stable() {
    let error = SnapshotExportError::new(SnapshotExportErrorKind::Stage, "x".repeat(800));
    assert_eq!(
        error.detail().chars().count(),
        MAX_SNAPSHOT_EXPORT_ERROR_CHARS
    );
    assert_eq!(error.kind().code(), "stage");
    assert_eq!(SnapshotExportErrorKind::Backpressure.code(), "backpressure");
    assert_eq!(
        SnapshotExportErrorKind::WorkerStopped.code(),
        "worker_stopped"
    );
}

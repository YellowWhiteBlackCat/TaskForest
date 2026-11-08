use std::path::PathBuf;
use std::sync::Arc;

use taskforest_contract::{WindowCaptureBackend, WindowCaptureFailureKind};

use super::*;

fn target(name: &str) -> WindowCaptureTarget {
    WindowCaptureTarget::path(PathBuf::from("/tmp").join(name))
}

struct ScriptedPort {
    completions: Vec<WindowCaptureCompletion>,
    failed: bool,
}

impl WindowCapturePort for ScriptedPort {
    fn try_submit(&mut self, request: WindowCaptureRequest) -> Result<(), WindowCaptureError> {
        if self.failed {
            return Err(WindowCaptureError::new(
                WindowCaptureErrorKind::Backpressure,
                "queue full",
            ));
        }
        // Complete immediately so the drain owner has a real state to decide.
        self.completions.push(WindowCaptureCompletion {
            request: request.id(),
            outcome: WindowCaptureOutcome::Ready {
                destination: Arc::from("/tmp/one.png"),
                width: 1920,
                height: 1080,
                backend: WindowCaptureBackend::SpectacleActiveWindow,
            },
        });
        Ok(())
    }

    fn drain(&mut self) -> Vec<WindowCaptureCompletion> {
        std::mem::take(&mut self.completions)
    }
}

#[test]
fn the_intent_owner_decides_submit_and_drain_notices_for_every_frontend() {
    let mut session = WindowCaptureSession::new(ScriptedPort {
        completions: Vec::new(),
        failed: false,
    });

    // No installed client is an honest absence, not a failed request.
    let absent = request_current_window_capture::<ScriptedPort>(None, None);
    assert!(!absent.submitted);
    assert_eq!(absent.notice, WindowCaptureNotice::Unavailable);

    let queued = request_current_window_capture(Some(&mut session), None);
    assert!(queued.submitted);
    assert_eq!(queued.notice, WindowCaptureNotice::Queued);
    assert_eq!(queued.notice.severity(), WindowCaptureNoticeSeverity::Info);
    assert_eq!(
        queued.notice.lifecycle(),
        WindowCaptureNoticeLifecycle::Short
    );

    // The busy decision stays the owner's: a second request before the first
    // completion is drained must not be reimplemented differently per frontend.
    let busy = request_current_window_capture(Some(&mut session), None);
    assert!(!busy.submitted);
    assert_eq!(busy.notice, WindowCaptureNotice::Busy);
    assert_eq!(busy.notice.severity(), WindowCaptureNoticeSeverity::Warning);

    let (changed, notice) = drain_window_capture_completions(Some(&mut session));
    assert!(changed);
    assert_eq!(
        notice,
        Some(WindowCaptureNotice::Ready {
            destination: Arc::from("/tmp/one.png"),
            width: 1920,
            height: 1080,
        })
    );
    // Ready has its own success copy, decided by the shell feedback owner.
    assert_eq!(
        notice.as_ref().map(|item| item.severity()),
        Some(WindowCaptureNoticeSeverity::Success)
    );

    // A second drain has nothing left: no fabricated frame change either.
    assert_eq!(
        drain_window_capture_completions(Some(&mut session)),
        (false, None)
    );
}

#[test]
fn a_rejected_submit_becomes_a_failed_notice_with_the_port_detail() {
    let mut session = WindowCaptureSession::new(ScriptedPort {
        completions: Vec::new(),
        failed: true,
    });
    let outcome = request_current_window_capture(Some(&mut session), None);
    assert!(!outcome.submitted);
    assert_eq!(
        outcome.notice,
        WindowCaptureNotice::Failed("queue full".to_owned())
    );
    assert_eq!(
        outcome.notice.severity(),
        WindowCaptureNoticeSeverity::Error
    );
    assert_eq!(
        outcome.notice.lifecycle(),
        WindowCaptureNoticeLifecycle::Long
    );
}

#[test]
fn queued_running_ready_lifecycle_is_correlated() {
    let mut controller = WindowCaptureController::new();
    let request = controller.begin(target("one.png")).expect("request");
    assert!(matches!(controller.state(), WindowCaptureState::Queued(_)));
    assert_eq!(
        controller.mark_running(request.id()),
        WindowCaptureDisposition::Applied
    );
    assert_eq!(
        controller.complete(WindowCaptureCompletion {
            request: request.id(),
            outcome: WindowCaptureOutcome::Ready {
                destination: Arc::from("/tmp/one.png"),
                width: 1,
                height: 1,
                backend: WindowCaptureBackend::SpectacleActiveWindow,
            },
        }),
        WindowCaptureDisposition::Applied
    );
    assert!(matches!(
        controller.state(),
        WindowCaptureState::Ready { destination, width: 1, height: 1, .. }
            if destination.as_ref() == "/tmp/one.png"
    ));
}

#[test]
fn active_request_rejects_reentry_and_submission_failure_is_terminal() {
    let mut controller = WindowCaptureController::new();
    let request = controller.begin(target("one.png")).expect("request");
    assert!(matches!(
        controller.begin(target("two.png")),
        Err(WindowCaptureStartError::Busy(id)) if id == request.id()
    ));
    let error = WindowCaptureError::new(WindowCaptureErrorKind::Backpressure, "full");
    assert_eq!(
        controller.fail_submission(request.id(), error.clone()),
        WindowCaptureDisposition::Applied
    );
    assert!(matches!(
        controller.state(),
        WindowCaptureState::Failed { error: actual, .. } if actual == &error
    ));
}

#[test]
fn close_makes_late_completion_inert_and_next_request_wins() {
    let mut controller = WindowCaptureController::new();
    let first = controller.begin(target("one.png")).expect("first");
    let _ = controller.mark_running(first.id());
    controller.close();
    assert_eq!(
        controller.complete(WindowCaptureCompletion {
            request: first.id(),
            outcome: WindowCaptureOutcome::Ready {
                destination: Arc::from("late.png"),
                width: 1,
                height: 1,
                backend: WindowCaptureBackend::SpectacleActiveWindow,
            },
        }),
        WindowCaptureDisposition::LateIgnored
    );
    let second = controller.begin(target("two.png")).expect("second");
    assert!(second.id() > first.id());
}

#[test]
fn duplicate_terminal_cannot_replace_first_terminal() {
    let mut controller = WindowCaptureController::new();
    let request = controller.begin(target("one.png")).expect("request");
    let _ = controller.mark_running(request.id());
    assert_eq!(
        controller.complete(WindowCaptureCompletion {
            request: request.id(),
            outcome: WindowCaptureOutcome::Ready {
                destination: Arc::from("first.png"),
                width: 1,
                height: 1,
                backend: WindowCaptureBackend::SpectacleActiveWindow,
            },
        }),
        WindowCaptureDisposition::Applied
    );
    assert_eq!(
        controller.complete(WindowCaptureCompletion {
            request: request.id(),
            outcome: WindowCaptureOutcome::Failed(WindowCaptureError::new(
                WindowCaptureErrorKind::Commit,
                "duplicate",
            )),
        }),
        WindowCaptureDisposition::DuplicateIgnored
    );
    assert!(matches!(
        controller.state(),
        WindowCaptureState::Ready { destination, .. } if destination.as_ref() == "first.png"
    ));
}

#[test]
fn request_space_exhaustion_fails_closed_without_leaving_an_active_state() {
    let mut controller = WindowCaptureController::new();
    controller.next_request = None;

    assert!(matches!(
        controller.begin(target("exhausted.png")),
        Err(WindowCaptureStartError::RequestSpaceExhausted)
    ));
    assert!(matches!(controller.state(), WindowCaptureState::Closed));
}

#[test]
fn target_and_error_contracts_are_bounded_and_typed() {
    let path = PathBuf::from("/tmp/one.png");
    assert_eq!(
        WindowCaptureTarget::path(path.clone()).explicit_path(),
        Some(path.as_path())
    );
    assert!(
        WindowCaptureTarget::current_directory("one.png")
            .explicit_path()
            .is_none()
    );

    let error = WindowCaptureError::new(
        WindowCaptureErrorKind::Native(WindowCaptureFailureKind::ProviderUnavailable),
        "x".repeat(800),
    );
    assert_eq!(
        error.detail().chars().count(),
        MAX_WINDOW_CAPTURE_ERROR_CHARS
    );
    assert_eq!(error.kind().code(), "provider_unavailable");
    assert_eq!(
        WindowCaptureBackend::PipeWireScreenCast.code(),
        "pipewire-screencast"
    );
}

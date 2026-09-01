use std::path::PathBuf;
use std::sync::Arc;

use taskmanager_platform_contract::{WindowCaptureBackend, WindowCaptureFailureKind};

use super::*;

fn target(name: &str) -> WindowCaptureTarget {
    WindowCaptureTarget::path(PathBuf::from("/tmp").join(name))
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

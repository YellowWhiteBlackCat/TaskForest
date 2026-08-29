use super::super::*;
use taskmanager_application::{CorrelatedEvent, PlatformEventContext, ProcessEvent};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, OperationFailure, ProviderFailure, RequestId,
};

fn process_snapshot_batch(sequence: u64) -> PlatformEventBatch {
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(sequence).expect("fixture request id"),
            capability: CapabilityId::PROCESS_LIST,
            provider: Some(ProviderId::borrowed("fixture.processes")),
            sequence: EventSequence::new(sequence),
            observed_at_ms: sequence,
        },
        ProcessEvent::Snapshot(std::sync::Arc::new(Vec::new())),
    ));
    batch
}

#[test]
fn platform_activity_cannot_overwrite_a_settings_error_notice() {
    let mut app = ShellApp::new();
    app.report_notice(
        FeedbackSource::Settings,
        FeedbackSeverity::Error,
        FeedbackLifecycle::UntilReplaced,
        "settings failed",
    );

    app.apply_platform_batch(process_snapshot_batch(1));

    assert_eq!(app.feedback_text(), "settings failed");
    assert_eq!(app.feedback_activity(), "Live · 1 updates");
    let notice = app.feedback_notice().expect("sticky settings notice");
    assert_eq!(notice.source(), FeedbackSource::Settings);
    assert_eq!(notice.severity(), FeedbackSeverity::Error);
    assert_eq!(notice.lifecycle(), FeedbackLifecycle::UntilReplaced);
}

#[test]
fn explicit_notice_replacement_is_last_writer_and_keeps_typed_metadata() {
    let mut app = ShellApp::new();
    app.report_notice(
        FeedbackSource::Control,
        FeedbackSeverity::Error,
        FeedbackLifecycle::UntilReplaced,
        "control failed",
    );
    app.report_notice(
        FeedbackSource::Clipboard,
        FeedbackSeverity::Success,
        FeedbackLifecycle::SHORT,
        "copied",
    );

    let notice = app.feedback_notice().expect("replacement notice");
    assert_eq!(notice.text(), "copied");
    assert_eq!(notice.source(), FeedbackSource::Clipboard);
    assert_eq!(notice.severity(), FeedbackSeverity::Success);
    assert_eq!(notice.lifecycle(), FeedbackLifecycle::SHORT);
}

#[test]
fn transient_notice_expires_only_on_explicit_platform_batch_transitions() {
    let mut app = ShellApp::new();
    app.set_feedback_activity("live activity");
    app.report_notice(
        FeedbackSource::Clipboard,
        FeedbackSeverity::Success,
        FeedbackLifecycle::SHORT,
        "copied",
    );

    app.apply_platform_batch(PlatformEventBatch::default());
    assert_eq!(app.feedback_text(), "copied");
    assert_eq!(
        app.feedback_notice().map(FeedbackNotice::lifecycle),
        Some(FeedbackLifecycle::NEXT_PLATFORM_BATCH)
    );

    app.apply_platform_batch(PlatformEventBatch::default());
    assert!(app.feedback_notice().is_none());
    assert_eq!(app.feedback_text(), "live activity");
}

#[test]
fn platform_failure_becomes_a_sticky_typed_error_notice() {
    let mut app = ShellApp::new();
    let mut batch = PlatformEventBatch::default();
    batch.failures.push(OperationFailure {
        request_id: RequestId::new(7).expect("fixture request id"),
        capability: CapabilityId::SERVICES,
        sequence: EventSequence::new(7),
        kind: FailureKind::PermissionDenied,
        retry: ProviderFailure::from_kind(FailureKind::PermissionDenied).retry(),
        provider: Some(ProviderId::borrowed("fixture.services")),
        observed_at_ms: 7,
    });

    app.apply_platform_batch(batch);

    let notice = app.feedback_notice().expect("platform failure notice");
    assert_eq!(notice.source(), FeedbackSource::Platform);
    assert_eq!(notice.severity(), FeedbackSeverity::Error);
    assert_eq!(notice.lifecycle(), FeedbackLifecycle::UntilReplaced);
    assert!(notice.text().contains("PermissionDenied"));
}

#[test]
fn quit_is_one_way_idempotent_and_preserves_the_first_reason() {
    let mut app = ShellApp::new();
    assert!(!app.should_quit());
    assert_eq!(app.quit_reason(), None);

    assert_eq!(
        app.request_quit(QuitReason::Keyboard),
        QuitRequestOutcome::Requested(QuitReason::Keyboard)
    );
    assert_eq!(
        app.request_quit(QuitReason::Tray),
        QuitRequestOutcome::AlreadyRequested(QuitReason::Keyboard)
    );
    assert!(app.should_quit());
    assert_eq!(app.quit_reason(), Some(QuitReason::Keyboard));
}

/// The first-run Restart completion quits under its dedicated reason: the
/// replacement instance is already running, so the exit records no failure
/// or confirmation feedback — the quit state alone carries the lifecycle.
#[test]
fn restart_quit_is_recorded_without_failure_feedback() {
    let mut app = ShellApp::new();
    assert_eq!(
        app.request_quit(QuitReason::Restart),
        QuitRequestOutcome::Requested(QuitReason::Restart)
    );
    assert!(app.should_quit());
    assert_eq!(app.quit_reason(), Some(QuitReason::Restart));
    // A later interactive request cannot rewrite why shutdown began.
    assert_eq!(
        app.request_quit(QuitReason::Keyboard),
        QuitRequestOutcome::AlreadyRequested(QuitReason::Restart)
    );
    assert_eq!(app.feedback_notice(), None);
    assert_eq!(app.feedback_text(), app.feedback_activity());
}

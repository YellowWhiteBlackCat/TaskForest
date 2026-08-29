use super::*;
use taskmanager_application::{
    CorrelatedEvent, PlatformEffect, PlatformEventBatch, PlatformEventContext,
    SessionControlOutcome, SessionEvent,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};

/// Push one session-control outcome through the PUBLIC batch path (the
/// same way a real platform client delivers it) and return the shell.
fn shell_with_feedback(ok: bool) -> (ShellApp, taskmanager_theme::Theme) {
    let mut shell = taskmanager_shell::demo_app();
    shell.selected = 0;
    let effect = shell
        .request_session_control(SessionControlAction::Disconnect)
        .expect("the demo fixture has a selectable session");
    let PlatformEffect::SessionControl(target) = effect else {
        panic!("request must produce a session-control effect");
    };
    let mut batch = PlatformEventBatch::default();
    batch.session_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(2).expect("fixture request ID"),
            capability: CapabilityId::SESSIONS,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 5,
        },
        SessionEvent::Control(SessionControlOutcome {
            request_id: target.request_id,
            session_id: target.session_id,
            action: target.action,
            result: if ok {
                Ok(())
            } else {
                Err(FailureKind::PermissionDenied)
            },
        }),
    ));
    shell.apply_platform_batch(batch);
    (shell, taskmanager_theme::Theme::dark())
}

#[test]
fn session_feedback_line_renders_success_and_failure_typed_copy() {
    let shell = taskmanager_shell::demo_app();
    let theme = taskmanager_theme::Theme::dark();
    assert!(
        session_feedback_line(&theme, &shell).is_none(),
        "no outcome yet: the bar shows the selection hint, not feedback"
    );

    let (shell, _) = shell_with_feedback(true);
    assert!(
        shell.projection().session_control_feedback.is_some(),
        "the accepted outcome must be recorded for the action bar"
    );
    assert!(
        session_feedback_line(&theme, &shell).is_some(),
        "a successful outcome renders a feedback line"
    );

    let (shell, _) = shell_with_feedback(false);
    assert!(
        session_feedback_line(&theme, &shell).is_some(),
        "a failed outcome renders a feedback line too"
    );
}

#[test]
fn request_session_control_clears_stale_feedback_and_captures_the_target() {
    let (mut shell, _) = shell_with_feedback(true);
    assert!(shell.projection().session_control_feedback.is_some());
    shell.selected = 0;
    let effect = shell.request_session_control(SessionControlAction::Lock);
    assert!(
        effect.is_some(),
        "the demo fixture has a selectable session"
    );
    assert_eq!(
        shell.projection().session_control_feedback,
        None,
        "a new action expires the previous outcome's feedback"
    );
}

#[test]
fn empty_sessions_from_a_failed_source_report_the_typed_reason() {
    use taskmanager_application::{RefreshRequest, SourceNotice, source_notice};
    use taskmanager_core::core::identity::ProviderId;

    let mut shell = ShellApp::new();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sessions(Some(Vec::new())),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::SessionsSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("loginctl"),
            outcome: SourceOutcome::Unavailable(
                taskmanager_core::core::failure::FailureKind::MissingDependency,
            ),
            item_count: 0,
        }])),
    );
    assert_eq!(
        source_notice(
            shell
                .projection()
                .sessions_source
                .as_deref()
                .unwrap_or_default()
        ),
        Some(SourceNotice::Unavailable(
            taskmanager_core::core::failure::FailureKind::MissingDependency,
        ))
    );
    // The panel renders the honest reason, not "No sessions", and keeps
    // the non-retryable capability-change guidance instead of a dead
    // refresh loop.
    let app = crate::IcedApp::demo();
    let panel = source_state_panel(
        app.theme(),
        shell.projection().sessions_source.as_deref(),
        RefreshRequest::Sessions,
    );
    assert!(panel.is_some());
}

#[test]
fn empty_sessions_from_an_available_source_stay_a_genuine_empty_state() {
    use taskmanager_application::source_notice;
    use taskmanager_core::core::identity::ProviderId;

    let mut shell = ShellApp::new();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sessions(Some(Vec::new())),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::SessionsSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("loginctl"),
            outcome: SourceOutcome::Empty,
            item_count: 0,
        }])),
    );
    assert_eq!(
        source_notice(
            shell
                .projection()
                .sessions_source
                .as_deref()
                .unwrap_or_default()
        ),
        None
    );
}

#[test]
fn retryable_source_state_exposes_a_page_scoped_refresh_surface() {
    use taskmanager_application::{RefreshRequest, source_notice};
    use taskmanager_core::core::identity::ProviderId;

    let mut shell = ShellApp::new();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sessions(Some(Vec::new())),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::SessionsSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("loginctl"),
            outcome: SourceOutcome::Unavailable(
                taskmanager_core::core::failure::FailureKind::TimedOut,
            ),
            item_count: 0,
        }])),
    );
    let notice = source_notice(shell.projection().sessions_source.as_deref().unwrap_or(&[]))
        .expect("timeout must produce a source notice");
    assert!(notice.is_retryable());

    let app = crate::IcedApp::demo();
    assert!(
        source_state_panel(
            app.theme(),
            shell.projection().sessions_source.as_deref(),
            RefreshRequest::Sessions,
        )
        .is_some()
    );
    assert!(
        source_notice_banner(
            app.theme(),
            shell.projection().sessions_source.as_deref(),
            RefreshRequest::Sessions,
        )
        .is_some()
    );
}

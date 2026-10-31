use super::*;
use taskforest_contract::{CapabilityId, EventSequence, RequestId};
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::session::{SessionControlAction, SessionIncarnation, SessionItem};
use taskforest_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_application::{
    CorrelatedEvent, PlatformEffect, PlatformEventBatch, PlatformEventContext,
    SessionControlOutcome, SessionEvent,
};
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture::ProjectionSeedFact;
use taskmanager_shell::fixture::seed_projection_fact;
use taskmanager_theme::Theme;

/// Push one session-control outcome through the PUBLIC batch path (the
/// same way a real platform client delivers it) and return the shell.
fn shell_with_feedback(ok: bool) -> (ShellApp, Theme) {
    let mut shell = demo_app();
    let session = install_control_identity(&mut shell);
    shell.selected = 0;
    assert!(shell.select_session_control(&session, SessionControlAction::Disconnect));
    let effect = shell
        .confirm_session_control()
        .expect("the armed session identity confirms");
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
            identity: target.identity,
            action: target.action,
            result: if ok {
                Ok(())
            } else {
                Err(FailureKind::PermissionDenied)
            },
        }),
    ));
    shell.apply_platform_batch(batch);
    (shell, Theme::dark())
}

fn install_control_identity(shell: &mut ShellApp) -> SessionItem {
    let mut sessions = shell.projection().sessions.clone().unwrap_or_default();
    let session = sessions
        .first_mut()
        .expect("the demo fixture has a session");
    session.incarnation = Some(SessionIncarnation::LinuxLogind {
        boot_id: "fixture-boot".into(),
        created_monotonic_usec: 43,
    });
    let session = session.clone();
    seed_projection_fact(shell, ProjectionSeedFact::Sessions(Some(sessions)));
    session
}

#[test]
fn session_feedback_line_renders_success_and_failure_typed_copy() {
    let shell = demo_app();
    let theme = Theme::dark();
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
    let session = install_control_identity(&mut shell);
    shell.selected = 0;
    assert!(shell.select_session_control(&session, SessionControlAction::Lock));
    let effect = shell.confirm_session_control();
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
    use taskforest_core::core::identity::ProviderId;
    use taskmanager_application::{RefreshRequest, SourceNotice, source_notice};

    let mut shell = ShellApp::new();
    seed_projection_fact(&mut shell, ProjectionSeedFact::Sessions(Some(Vec::new())));
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::SessionsSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("loginctl"),
            outcome: SourceOutcome::Unavailable(FailureKind::MissingDependency),
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
        Some(SourceNotice::Unavailable(FailureKind::MissingDependency,))
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
    use taskforest_core::core::identity::ProviderId;
    use taskmanager_application::source_notice;

    let mut shell = ShellApp::new();
    seed_projection_fact(&mut shell, ProjectionSeedFact::Sessions(Some(Vec::new())));
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::SessionsSource(Some(vec![SourceStatus {
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
    use taskforest_core::core::identity::ProviderId;
    use taskmanager_application::{RefreshRequest, source_notice};

    let mut shell = ShellApp::new();
    seed_projection_fact(&mut shell, ProjectionSeedFact::Sessions(Some(Vec::new())));
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::SessionsSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("loginctl"),
            outcome: SourceOutcome::Unavailable(FailureKind::TimedOut),
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

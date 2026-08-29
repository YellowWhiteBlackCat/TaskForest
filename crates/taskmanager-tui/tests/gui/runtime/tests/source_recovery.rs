//! Page-scoped source recovery keyboard paths and retry policy.

use super::super::*;

use taskmanager_application::{AppPage, RefreshRequest};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

fn source(outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("fixture.services"),
        outcome,
        item_count: 0,
    }
}

#[test]
fn retry_key_targets_only_the_visible_inventory_page_and_retryable_failures() {
    let mut app = TuiApp::from_shell(taskmanager_shell::ShellApp::new());
    app.application.active_page = AppPage::Services;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(Vec::new())),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ServicesSource(Some(vec![source(
            SourceOutcome::Unavailable(FailureKind::TimedOut),
        )])),
    );

    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('r'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        effect,
        Some(PlatformEffect::Refresh(RefreshRequest::Services))
    );

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ServicesSource(Some(vec![source(
            SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        )])),
    );
    assert_eq!(
        handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Char('r'),
                KeyModifiers::NONE
            ),
        ),
        None,
        "permission failures require a capability change before refresh"
    );

    app.application.active_page = AppPage::Startup;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupEntries(Some(Vec::new())),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("fixture.startup"),
            outcome: SourceOutcome::Unavailable(FailureKind::TemporarilyUnavailable),
            item_count: 0,
        }])),
    );
    assert_eq!(
        handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Char('r'),
                KeyModifiers::NONE
            ),
        ),
        Some(PlatformEffect::Refresh(RefreshRequest::Startup))
    );
}

//! Source-status banners keep usable rows visible and explain recovery policy.

use super::frame_text;

use taskmanager_application::{AppPage, FailureKind, ProviderId, SourceOutcome, SourceStatus};

#[test]
fn partial_services_frame_keeps_rows_and_shows_page_scoped_retry_hint() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ServicesSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("fixture.services"),
            outcome: SourceOutcome::Partial(FailureKind::TimedOut),
            item_count: 5,
        }])),
    );

    let frame = frame_text(&app, 120, 36);
    assert!(frame.contains("Some data may be missing"));
    assert!(frame.contains("r Refresh"));
    assert!(frame.contains("NetworkManager.service"));
}

#[test]
fn permission_denied_services_frame_explains_capability_change_without_retry() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ServicesSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("fixture.services"),
            outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            item_count: 5,
        }])),
    );

    let frame = frame_text(&app, 120, 36);
    assert!(frame.contains("Data source unavailable"));
    assert!(frame.contains("Resolve the provider issue"));
    assert!(frame.contains("then refresh"));
    assert!(!frame.contains("r Refresh"));
}

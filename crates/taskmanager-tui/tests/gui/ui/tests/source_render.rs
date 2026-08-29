//! Source-status banners keep usable rows visible and explain recovery policy.

use super::frame_text;

use taskmanager_application::AppPage;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

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

/// An empty inventory from a FAILED source must not read as "no data": the
/// shared windowed-table primitive's state panel carries the typed source
/// reason (GPUI empty_state_failure parity), never the bare empty fallback.
#[test]
fn empty_services_from_a_failed_source_explain_the_failure_not_the_absence() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(Vec::new())),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ServicesSource(Some(vec![SourceStatus {
            provider: ProviderId::borrowed("fixture.services"),
            outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            item_count: 0,
        }])),
    );

    let frame = frame_text(&app, 120, 36);
    assert!(
        frame.contains("Data source unavailable"),
        "the typed source reason surfaces in the state panel"
    );
    assert!(
        !frame.contains("No services reported yet."),
        "a failed source must never read as an empty inventory"
    );
}

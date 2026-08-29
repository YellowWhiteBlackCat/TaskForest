//! Per-event row-snapshot parity for the canonical category tree.

use super::super::*;
use taskmanager_application::AppPage;
use taskmanager_core::core::metrics::ScalarObservation;

fn fixture_app() -> TuiApp {
    let mut processes = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1)
            .name("root".into())
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(2)
            .name("child".into())
            .parent_pid(Some(1))
            .build(),
    ];
    for process in &mut processes {
        process.apply_scalar_observations(
            taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(u64::from(process.pid), 42),
                ..Default::default()
            },
        );
    }
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(processes)),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();
    app
}

#[test]
fn slice_resolver_matches_rebuild_for_every_visible_cursor() {
    let mut app = fixture_app();
    for cursor in 0..app.process_rows_snapshot().len() {
        app.selected = cursor;
        let rows = app.process_rows_snapshot();
        let via_slice = app
            .selected_detail_process_rows(&rows)
            .map(|process| process.pid);
        let via_rebuild = app.selected_detail_process().map(|process| process.pid);
        assert_eq!(via_slice, via_rebuild, "cursor {cursor}");
    }
}

#[test]
fn one_shot_motion_updates_cursor_identity_and_insights_target() {
    let mut app = fixture_app();
    let effect = app.move_nonflat_selection_oneshot(1);
    assert_eq!(app.selected, 1);
    assert_eq!(
        app.application.selected_process.as_ref().map(|id| id.pid),
        Some(1)
    );
    assert!(matches!(effect, Some(PlatformEffect::ProcessInsights(_))));
    assert_eq!(app.last_insights_target.as_ref().map(|id| id.pid), Some(1));
}

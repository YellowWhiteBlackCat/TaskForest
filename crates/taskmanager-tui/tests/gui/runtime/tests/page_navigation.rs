//! Applications category-tree paging and inert TUI-only chords.

use super::super::*;
use taskmanager_application::{
    AppAction, AppPage, CorrelatedServiceEvent, PlatformEventBatch, ServiceEvent,
};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, PartialSourceSnapshot, RequestId,
};

fn process(pid: u32) -> ProcessItem {
    let mut process = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(format!("proc-{pid}"))
        .build();
    process.apply_scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid), 1),
        ..Default::default()
    });
    process
}

#[test]
fn page_keys_move_over_the_category_tree_and_reset_detail_scroll() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(
            (1..=25).map(process).collect(),
        )),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();
    app.detail_scroll = 4;

    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.selected, taskmanager_shell::PAGE_STEP);
    assert_eq!(app.detail_scroll, 0);
    assert!(matches!(effect, Some(PlatformEffect::ProcessInsights(_))));

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::End, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, app.visual_row_count() - 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Home, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, 0);
}

#[test]
fn non_application_table_pages_keep_their_own_flat_bounds() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::End, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, app.sorted_services().len().saturating_sub(1));
}

#[test]
fn flat_inventory_sort_keeps_the_selected_provider_identity() {
    for (page, table) in [
        (AppPage::Services, taskmanager_shell::InfoTable::Services),
        (AppPage::Startup, taskmanager_shell::InfoTable::Startup),
        (AppPage::Users, taskmanager_shell::InfoTable::Users),
    ] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(page));
        let count = match table {
            taskmanager_shell::InfoTable::Services => app.sorted_services().len(),
            taskmanager_shell::InfoTable::Startup => app.sorted_startup_entries().len(),
            taskmanager_shell::InfoTable::Users => app.sorted_sessions().len(),
        };
        assert!(count >= 2, "{page:?} fixture needs two inventory rows");
        app.selected = 1;
        let identity = match table {
            taskmanager_shell::InfoTable::Services => app
                .sorted_service_at(app.selected)
                .expect("service row")
                .id
                .as_str()
                .to_owned(),
            taskmanager_shell::InfoTable::Startup => app
                .sorted_startup_entry_at(app.selected)
                .expect("startup row")
                .id
                .as_str()
                .to_owned(),
            taskmanager_shell::InfoTable::Users => app
                .sorted_session_at(app.selected)
                .expect("session row")
                .id
                .to_string(),
        };

        app.cycle_info_sort_column_preserving_anchor(table);
        app.toggle_info_sort_direction_preserving_anchor(table);

        let still_selected = match table {
            taskmanager_shell::InfoTable::Services => app
                .sorted_service_at(app.selected)
                .is_some_and(|service| service.id.as_str() == identity.as_str()),
            taskmanager_shell::InfoTable::Startup => app
                .sorted_startup_entry_at(app.selected)
                .is_some_and(|entry| entry.id.as_str() == identity.as_str()),
            taskmanager_shell::InfoTable::Users => app
                .sorted_session_at(app.selected)
                .is_some_and(|session| session.id.as_str() == identity.as_str()),
        };
        assert!(
            still_selected,
            "{page:?} sort must preserve its row identity"
        );
    }
}

#[test]
fn services_refresh_reanchors_the_committed_provider_identity() {
    let service = |id: &str, name: &str| {
        ServiceItem::from_inventory(
            ServiceId::new(id),
            name,
            ServiceStatus::Active,
            "",
            "",
            "",
            "",
        )
    };
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            service("service:zeta", "zeta"),
            service("service:alpha", "alpha"),
        ])),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let selected_id = app
        .sorted_service_at(app.selected)
        .expect("initial service row")
        .id
        .clone();

    let batch = PlatformEventBatch {
        service_events: vec![CorrelatedServiceEvent {
            request_id: RequestId::MIN,
            capability: CapabilityId::SERVICES,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            event: ServiceEvent::Snapshot(PartialSourceSnapshot::new(
                vec![
                    service("service:alpha", "alpha"),
                    service("service:zeta", "zeta"),
                ],
                Vec::new(),
            )),
        }],
        ..PlatformEventBatch::default()
    };
    app.apply_platform_batch(batch);

    assert_eq!(
        app.sorted_service_at(app.selected)
            .map(|row| row.id.clone()),
        Some(selected_id),
        "a service refresh may reorder rows but must retain the selected provider id"
    );
    assert_eq!(app.selected, 1, "the retained zeta row moved to index one");
}

#[test]
fn f9_is_consumed_without_creating_a_terminal_sidebar() {
    let mut app = crate::demo_app();
    let before = app.input_scope();
    let effect = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::F(9), KeyModifiers::NONE),
    );
    assert!(effect.is_none());
    assert_eq!(app.input_scope(), before);
}

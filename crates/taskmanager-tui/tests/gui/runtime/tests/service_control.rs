//! Service-page action menu, the shared confirmation, the full control
//! round-trip through the submission queue, and the service-log panel keys.

use super::super::*;

use taskmanager_application::AppAction;

#[test]
fn enter_on_services_opens_the_action_menu_and_esc_closes_it() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(app.service_menu().is_none());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.service_menu().is_some(),
        "Enter must open the action menu"
    );
    let service = app
        .projection()
        .services
        .as_ref()
        .and_then(|services| services.first())
        .expect("demo services");
    assert_eq!(
        app.service_menu().map(|menu| menu.service.name.as_str()),
        Some(service.name.as_str()),
        "the menu freezes the selected row"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(app.service_menu().is_none());
}

#[test]
fn service_menu_select_opens_the_shared_confirmation_and_y_confirms() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // Down -> Stop, then Enter to pick the action.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.pending_service_control().is_some(),
        "picking an action must open the shared confirmation"
    );
    assert!(app.service_menu().is_none(), "the menu closes on pick");

    // y confirms: the platform request is produced with the frozen target.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('y'),
            KeyModifiers::NONE,
        ),
    );
    let Some(PlatformEffect::ServiceControl(target)) = effect else {
        panic!("confirm must produce a ServiceControl effect");
    };
    assert_eq!(target.action, taskmanager_application::ServiceAction::Stop);
    assert!(!target.service_id.as_str().is_empty());
    assert_eq!(app.pending_service_control(), None);
}

#[test]
fn service_confirmation_n_dismisses_without_a_platform_effect() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.pending_service_control().is_some());
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('n'),
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none(), "dismissal must not produce an effect");
    assert_eq!(app.pending_service_control(), None);
}

#[test]
fn service_control_round_trip_submits_through_queue_effect() {
    use std::sync::{Arc, Mutex};
    use taskmanager_application::{
        CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
        PlatformEvent, PlatformFacets, PlatformHandle, RequestEnvelope, RequestPort,
        ServiceControlRequest, ServiceFacets, SubmissionError,
    };

    #[derive(Default)]
    struct EmptyCapabilities;
    impl CapabilityCatalog for EmptyCapabilities {
        fn snapshot(&self) -> CapabilitySnapshot {
            CapabilitySnapshot::default()
        }
    }

    #[derive(Default)]
    struct EmptyEvents;
    impl EventPort for EmptyEvents {
        type Event = PlatformEvent;

        fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct RecordingServiceControl(Mutex<Vec<ServiceControlRequest>>);
    impl RequestPort for RecordingServiceControl {
        type Request = ServiceControlRequest;

        fn try_submit(
            &self,
            request: RequestEnvelope<Self::Request>,
        ) -> Result<(), SubmissionError> {
            self.0
                .lock()
                .expect("recorded requests")
                .push(request.payload);
            Ok(())
        }
    }

    let recorded = Arc::new(RecordingServiceControl::default());
    let mut client = PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default()
            .with_service(ServiceFacets::default().with_control(recorded.clone())),
    ));

    // select (menu) -> Request (confirmation) -> Confirm (effect) -> queue.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.pending_service_control().is_some(),
        "the picked action must open the confirmation"
    );
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('y'),
            KeyModifiers::NONE,
        ),
    )
    .expect("confirm must produce an effect");

    taskmanager_shell::queue_effect(&mut app, &mut client, effect);

    let submitted = recorded.0.lock().expect("recorded requests");
    assert_eq!(submitted.len(), 1, "exactly one request is submitted");
    let request = &submitted[0];
    assert_eq!(
        request.action,
        taskmanager_application::ServiceAction::Stop,
        "the menu's Stop pick must reach the provider"
    );
    assert!(
        !request.service_id.as_str().is_empty(),
        "the provider-issued target must be submitted"
    );
    assert_ne!(
        request.request_id.get(),
        0,
        "the latest-wins correlation id must be allocated"
    );
}

#[test]
fn service_log_open_and_panel_keys_drive_the_shared_state_machine() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('3'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Services);
    assert!(!app.shell.service_log.is_some());

    // `o` opens the stream for the selected service and returns the initial
    // follow request to queue.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('o'),
            KeyModifiers::NONE,
        ),
    );
    assert!(
        matches!(effect, Some(PlatformEffect::ServiceLogStream(_))),
        "open must return the initial stream request"
    );
    assert!(app.shell.service_log.is_some());

    // Panel chords drive the shell transitions; none return an effect.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyModifiers::NONE,
        ),
    );
    assert!(!app.shell.service_log.as_ref().unwrap().feed.follow);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('p'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.shell.service_log.as_ref().unwrap().feed.paused);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('l'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.service_log.as_ref().unwrap().feed.level,
        taskmanager_application::ServiceLogLevelFilter::Errors
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('t'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.service_log.as_ref().unwrap().feed.time,
        taskmanager_application::ServiceLogTimeFilter::LastHour
    );

    // `q` closes; the page switch closes it too (page-change hygiene).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('q'),
            KeyModifiers::NONE,
        ),
    );
    assert!(!app.shell.service_log.is_some());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('o'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.shell.service_log.is_some());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('1'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Performance);
    assert!(
        !app.shell.service_log.is_some(),
        "leaving Services must close the log stream"
    );
}

#[test]
fn paging_and_bound_keys_work_on_every_table_page() {
    let mut app = crate::demo_app();
    // Park on Services with a multi-row list, then page and jump.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('3'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Services);
    let service_count = app.table_row_count().unwrap_or(0);
    assert!(service_count >= 2, "demo services cover paging");

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::End, KeyModifiers::NONE),
    );
    assert_eq!(
        app.selected,
        service_count - 1,
        "End reaches the last service"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Home, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, 0, "Home returns to the first service");
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.selected,
        taskmanager_shell::PAGE_STEP.min(service_count - 1),
        "PageDown pages the service list by the shared step"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageUp,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.selected, 0, "PageUp pages back");

    // The same keys work on the Users and Startup table pages.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('6'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Users);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::End, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, app.table_row_count().unwrap_or(0) - 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('5'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Startup);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageUp,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.selected, 0,
        "Startup paging wraps back to the first row"
    );

    // Chorded variants are not wired on any page.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::End,
            KeyModifiers::CONTROL,
        ),
    );
    assert_eq!(app.selected, 0);
}

#[test]
fn paging_on_a_table_page_uses_that_pages_own_row_projection() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('3'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Services);
    let service_count = app.table_row_count().unwrap_or(0);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::End, KeyModifiers::NONE),
    );
    assert_eq!(
        app.selected,
        service_count - 1,
        "End on Services uses the flat service list"
    );
}

#[test]
fn s_key_sorts_the_inventory_tables_from_the_keyboard() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('3'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Services);

    // `s` on Services starts the table's sort cycle at the first column.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.services_sort,
        Some((
            taskmanager_shell::InfoSortCol::Name,
            taskmanager_shell::SortDir::Asc
        )),
        "s on Services starts the Name sort"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.services_sort,
        Some((
            taskmanager_shell::InfoSortCol::Status,
            taskmanager_shell::SortDir::Asc
        )),
        "the second s walks to the Status column"
    );

    // `S` flips the direction without changing the column.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('S'),
            KeyModifiers::SHIFT,
        ),
    );
    assert_eq!(
        app.shell.services_sort,
        Some((
            taskmanager_shell::InfoSortCol::Status,
            taskmanager_shell::SortDir::Desc
        )),
        "S flips the direction"
    );

    // The same keys sort the Users table from its own cycle (no Status).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('6'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Users);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.sessions_sort,
        Some((
            taskmanager_shell::InfoSortCol::Name,
            taskmanager_shell::SortDir::Asc
        )),
        "s on Users starts the Name sort"
    );
}

#[test]
fn s_key_on_the_applications_page_still_cycles_visible_process_columns() {
    // The inventory routing must not steal `s` from the Applications page.
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Applications);
    app.process_sort = (
        taskmanager_shell::SortCol::Cpu,
        taskmanager_shell::SortDir::Desc,
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.effective_sort_col(),
        taskmanager_shell::SortCol::Memory,
        "s on Applications walks the visible process columns as before"
    );
}

/// One synthetic service row: the provider-issued id is derived from the
/// name so the sorted-vs-provider order assertions below are unambiguous.
fn sorted_fixture_service(name: &str) -> taskmanager_application::ServiceItem {
    taskmanager_application::ServiceItem::from_inventory(
        taskmanager_application::ServiceId::new(format!("fixture.service:{name}")),
        name,
        taskmanager_application::ServiceStatus::Active,
        "",
        "",
        "",
        "",
    )
}

/// An active sort reorders the rendered rows; Enter's action menu and the
/// `o` log-open must resolve the SAME sorted projection the renderer paints
/// (the frozen target is the highlighted row), never the provider-order
/// vector the cursor does not index.
#[test]
fn menu_and_log_open_target_the_sorted_services_row() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    // Provider order [zeta, alpha]; the Name sort renders [alpha, zeta].
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            sorted_fixture_service("zeta.service"),
            sorted_fixture_service("alpha.service"),
        ])),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    // Fixture guard: the sort must actually differ from the provider order,
    // otherwise the assertions below could pass against the wrong vector.
    assert_eq!(
        app.projection()
            .services
            .as_ref()
            .and_then(|services| services.first())
            .map(|service| service.id.as_str()),
        Some("fixture.service:zeta.service")
    );

    // Enter on the highlighted first row freezes the SORTED first target.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.service_menu().map(|menu| menu.service.id.as_str()),
        Some("fixture.service:alpha.service"),
        "the menu must freeze the sorted (rendered) row, not the provider row"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );

    // The `o` log-open resolves through the same sorted projection.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('o'),
            KeyModifiers::NONE,
        ),
    );
    let Some(PlatformEffect::ServiceLogStream(request)) = effect else {
        panic!("open must return the initial stream request");
    };
    assert_eq!(
        request.query.service_id.as_str(),
        "fixture.service:alpha.service",
        "the log stream must follow the highlighted (sorted) row"
    );
}

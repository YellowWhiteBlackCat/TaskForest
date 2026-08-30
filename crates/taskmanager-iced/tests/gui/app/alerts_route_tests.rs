use super::*;
use crate::app::Message;
use taskmanager_core::core::process::ProcessLiveKey;

#[test]
fn typed_route_transitions_are_idempotent_and_shared_page_selection_closes_alerts() {
    let mut state = AlertsPageState::default();
    assert_eq!(state.route(), FrontendRoute::SharedPage);
    assert_eq!(
        state.transition(FrontendRouteEvent::OpenAlerts),
        FrontendRouteTransition::AlertsOpened
    );
    assert_eq!(
        state.transition(FrontendRouteEvent::OpenAlerts),
        FrontendRouteTransition::Unchanged
    );
    assert_eq!(
        state.transition(FrontendRouteEvent::SelectSharedPage),
        FrontendRouteTransition::AlertsClosed
    );
    assert_eq!(
        state.transition(FrontendRouteEvent::CloseAlerts),
        FrontendRouteTransition::Unchanged
    );
}

#[test]
fn modal_input_precedence_keeps_the_alerts_route_under_the_modal() {
    let mut app = crate::IcedApp::demo();
    app.open_alerts_page();
    assert!(app.alerts_page_open());

    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    assert!(app.settings_open());
    assert!(app.alerts_page_open());

    let _ = app.update(Message::CloseSettings);
    assert!(app.alerts_page_open());
}

#[test]
fn jump_to_process_selects_the_shared_route() {
    let mut app = crate::IcedApp::demo();
    let identity = app
        .shell
        .projection()
        .processes
        .as_ref()
        .and_then(|processes| processes.first())
        .and_then(ProcessLiveKey::from_process)
        .expect("demo process identity");
    app.open_alerts_page();

    let _ = app.update(Message::JumpToProcess { identity });

    assert!(!app.alerts_page_open());
    assert_eq!(
        app.shell.page(),
        taskmanager_application::AppPage::Applications
    );
}

#[test]
fn shared_confirmation_from_another_domain_runs_the_common_finish_systems() {
    let mut app = crate::IcedApp::demo();
    app.shell.application.active_page = taskmanager_application::AppPage::Applications;
    assert!(app.shell.select_row(0));
    let _ = app.update(Message::OpenSettings);
    assert!(app.settings_open());

    let _ = app.update(Message::RequestEndTask);
    assert!(app.shell.pending_end().is_some(), "demo selects a process");
    assert!(
        !app.settings_open(),
        "shared interaction convergence must run after the control reducer"
    );
}

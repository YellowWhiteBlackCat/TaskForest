use super::*;

#[test]
fn service_and_startup_row_menus_preserve_provider_identity() {
    let mut app = IcedApp::demo();

    let _ = app.update(Message::SelectPage(AppPage::Services));
    let _ = app.update(Message::OpenServiceRowMenu {
        visual_index: 0,
        source_index: 0,
    });
    assert_eq!(app.service_menu_index(), Some(0));
    let expected_service_id = app.service_menu_target().map(|service| service.id.clone());
    let mut reordered_services = app.shell.projection().services.clone().unwrap_or_default();
    reordered_services.reverse();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(reordered_services)),
    );
    let _ = crate::ui::view(&app);
    let _ = app.update(Message::RequestServiceAction {
        index: 0,
        action: taskmanager_core::core::services::ServiceAction::Stop,
    });
    assert!(app.service_menu_index().is_none());
    assert_eq!(
        app.shell
            .pending_service_control()
            .map(|target| target.service_id.clone()),
        expected_service_id
    );
    let _ = app.update(Message::DismissOverlay);

    let _ = app.update(Message::SelectPage(AppPage::Startup));
    let _ = app.update(Message::OpenStartupRowMenu { visual_index: 0 });
    let source_index = app
        .startup_menu_index()
        .expect("demo startup row should resolve to a provider entry");
    let expected_id = app.startup_menu_entry().map(|entry| entry.id.clone());
    assert!(expected_id.is_some());
    let mut reordered_startup = app
        .shell
        .projection()
        .startup_entries
        .clone()
        .unwrap_or_default();
    reordered_startup.reverse();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupEntries(Some(reordered_startup)),
    );
    let _ = crate::ui::view(&app);
    let _ = app.update(Message::RequestStartupControlFor {
        index: source_index,
        enabled: false,
    });
    assert!(app.startup_menu_index().is_none());
    assert_eq!(
        app.shell
            .pending_startup()
            .map(|request| request.entry.id.clone()),
        expected_id
    );
}

#[test]
fn users_menu_action_keeps_the_frozen_session_after_inventory_reorder() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Users));
    let _ = app.update(Message::OpenUserRowMenu(1));
    let expected_id = app
        .user_menu_session()
        .map(|session| session.id.clone())
        .expect("the menu must freeze a provider session");

    let mut reordered_sessions = app.shell.projection().sessions.clone().unwrap_or_default();
    reordered_sessions.reverse();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sessions(Some(reordered_sessions)),
    );

    let Some(taskmanager_application::PlatformEffect::SessionControl(target)) =
        app.request_user_menu_action(taskmanager_core::core::session::SessionControlAction::Lock)
    else {
        panic!("the frozen Users menu should emit a session-control effect");
    };
    assert_eq!(target.session_id.as_str(), expected_id);
    assert!(app.user_menu_session().is_none());
}

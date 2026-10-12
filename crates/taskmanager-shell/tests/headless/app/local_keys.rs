use crate::{InputDispatch, ShellApp, ShellKeyEvent, route_key};
use taskmanager_application::{
    AppAction, AppPage, CommandContext, CommandScope, KeyCode, Modifiers, SelectionDirection,
};

fn applications_shell() -> ShellApp {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let row_count = app.visible_process_count();
    assert!(row_count >= 3, "fixture must provide rows for navigation");
    assert!(app.select_row(row_count / 2));
    app
}

fn assert_same_selection(left: &ShellApp, right: &ShellApp) {
    assert_eq!(left.selected, right.selected);
    assert_eq!(left.selected_row, right.selected_row);
    assert_eq!(left.selected_rows, right.selected_rows);
    assert_eq!(
        left.selected_process_identity(),
        right.selected_process_identity()
    );
}

#[test]
fn applications_bare_row_keys_match_registered_process_list_actions() {
    let app = applications_shell();
    let context = CommandContext {
        scope: CommandScope::ProcessList,
        ..CommandContext::default()
    };
    let cases = [
        (KeyCode::ArrowUp, SelectionDirection::Previous),
        (KeyCode::ArrowDown, SelectionDirection::Next),
        (KeyCode::PageUp, SelectionDirection::PageUp),
        (KeyCode::PageDown, SelectionDirection::PageDown),
        (KeyCode::Home, SelectionDirection::First),
        (KeyCode::End, SelectionDirection::Last),
    ];

    for (key, direction) in cases {
        let event = ShellKeyEvent::new(key, Modifiers::NONE);
        let action = AppAction::MoveSelection(direction);
        assert_eq!(route_key(event, context), Some(action));

        let mut routed = app.clone();
        assert_eq!(routed.dispatch_key(event), InputDispatch::Consumed);
        let mut local = app.clone();
        assert_eq!(local.handle_local_key(event), InputDispatch::Consumed);
        assert_same_selection(&local, &routed);
    }
}

#[test]
fn other_inventory_pages_keep_shell_owned_row_navigation() {
    for page in [AppPage::Services, AppPage::Startup, AppPage::Users] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(page));
        let row_count = app
            .table_row_count()
            .expect("inventory pages expose a shell-owned table");
        assert!(row_count > 1, "fixture must provide multiple {page:?} rows");
        assert!(app.select_row(0));

        let event = ShellKeyEvent::new(KeyCode::ArrowDown, Modifiers::NONE);
        let mut shared_router = app.clone();
        assert_eq!(
            shared_router.dispatch_key(event),
            InputDispatch::Unhandled,
            "{page:?} has no ProcessList row-navigation binding"
        );
        assert_eq!(shared_router.selected, 0);

        assert_eq!(app.handle_local_key(event), InputDispatch::Consumed);
        assert_eq!(app.selected, 1, "{page:?} keeps local inventory navigation");
    }
}

#[test]
fn modified_row_keys_do_not_move_the_applications_selection() {
    let mut app = applications_shell();
    let before = app.clone();
    let cases = [
        (KeyCode::End, Modifiers::CONTROL),
        (KeyCode::PageUp, Modifiers::SHIFT),
        (KeyCode::ArrowDown, Modifiers::ALT),
        (KeyCode::Home, Modifiers::new(false, false, false, true)),
    ];

    for (key, modifiers) in cases {
        let event = ShellKeyEvent::new(key, modifiers);
        assert_eq!(app.handle_local_key(event), InputDispatch::Unhandled);
        assert_same_selection(&app, &before);
    }
}

#[test]
fn modal_help_suggestions_and_search_keep_navigation_precedence() {
    let event = ShellKeyEvent::new(KeyCode::ArrowDown, Modifiers::NONE);

    let mut modal = applications_shell();
    let selected_before = modal.selected;
    assert_eq!(
        modal.handle_local_key(ShellKeyEvent::new(KeyCode::Delete, Modifiers::NONE)),
        InputDispatch::Consumed
    );
    assert!(modal.pending_end().is_some());
    assert_eq!(modal.handle_local_key(event), InputDispatch::Consumed);
    assert_eq!(modal.selected, selected_before);
    assert!(modal.pending_end().is_some());

    let mut help = applications_shell();
    let selected_before = help.selected;
    help.toggle_help();
    assert_eq!(help.handle_local_key(event), InputDispatch::Consumed);
    assert_eq!(help.selected, selected_before);
    assert!(help.help_open());

    let mut suggestions = applications_shell();
    let selected_before = suggestions.selected;
    suggestions.toggle_suggestions();
    assert_eq!(suggestions.handle_local_key(event), InputDispatch::Consumed);
    assert_eq!(suggestions.selected, selected_before);
    assert!(suggestions.suggestions_open());

    let mut search = applications_shell();
    let selected_before = search.selected;
    search.open_search();
    assert_eq!(search.handle_local_key(event), InputDispatch::Consumed);
    assert_eq!(search.selected, selected_before);
    assert!(search.search_active());
    assert_eq!(search.dispatch_key(event), InputDispatch::Unhandled);
    assert_eq!(search.selected, selected_before);
}

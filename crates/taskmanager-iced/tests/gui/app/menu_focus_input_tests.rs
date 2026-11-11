// test-intent: behavior
//! Empty-menu keyboard routing must stay free of menu focus operations.

use super::*;
use taskmanager_application::{AppPage, KeyCode, Modifiers};
use taskmanager_shell::ShellKeyEvent;

#[test]
fn empty_process_actions_menu_leaves_arrow_routing_with_the_page() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    app.shell.clear_process_selection();
    let _ = app.update(Message::OpenProcessActionsMenu);
    app.input.focused_control = Some(FocusTarget::ProcessActionsTrigger);
    assert!(
        !app.shell.process_control_availability().is_ready(),
        "clearing the process selection removes the priority picker"
    );
    assert_eq!(
        app.context_menu_focus_menu_key(false),
        None,
        "the open action surface has no focusable menu item"
    );

    let focus_task = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::ArrowDown,
        Modifiers::NONE,
    ))));

    assert_eq!(
        focus_task.units(),
        0,
        "the empty menu does not schedule a focus operation"
    );
    assert!(app.process_actions_menu_open());
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::ProcessActionsTrigger)
    );
    assert!(
        app.shell.selected_row.is_some(),
        "the unclaimed arrow continues to Applications row navigation"
    );
}

// test-intent: behavior
//! Regression evidence for context-menu arrow traversal and its empty-target seam.

use crate::app::menus::next_context_menu_target;
use crate::app::{ContextMenuKind, FocusTarget, IcedApp, Message};
use taskmanager_application::AppPage;

#[test]
fn empty_menu_targets_never_select_a_focus_stop() {
    assert_eq!(next_context_menu_target(&[], None, false), None);
    assert_eq!(
        next_context_menu_target(&[], Some(FocusTarget::ProcessPrioritySelect), true),
        None
    );
}

#[test]
fn arrow_traversal_wraps_in_both_directions() {
    let targets = [
        FocusTarget::ProcessMenuEndTask,
        FocusTarget::ProcessMenuEndTree,
        FocusTarget::ProcessMenuKill,
    ];

    assert_eq!(
        next_context_menu_target(&targets, None, false),
        Some(FocusTarget::ProcessMenuEndTask)
    );
    assert_eq!(
        next_context_menu_target(&targets, None, true),
        Some(FocusTarget::ProcessMenuKill)
    );
    assert_eq!(
        next_context_menu_target(&targets, Some(FocusTarget::ProcessMenuKill), false),
        Some(FocusTarget::ProcessMenuEndTask)
    );
    assert_eq!(
        next_context_menu_target(&targets, Some(FocusTarget::ProcessMenuEndTask), true),
        Some(FocusTarget::ProcessMenuKill)
    );
}

#[test]
fn process_actions_menu_owns_one_real_focus_stop() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::OpenProcessActionsMenu);
    assert_eq!(
        app.context_menu_kind(),
        Some(ContextMenuKind::ProcessActions)
    );

    assert_eq!(
        app.context_menu_focus_menu_key(false),
        Some(FocusTarget::ProcessPrioritySelect)
    );
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::ProcessPrioritySelect)
    );
    assert_eq!(
        app.context_menu_focus_menu_key(true),
        Some(FocusTarget::ProcessPrioritySelect)
    );
}

#[test]
fn closed_menu_has_no_arrow_target() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    assert_eq!(app.context_menu_focus_menu_key(false), None);
}

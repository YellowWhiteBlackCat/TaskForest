// test-intent: behavior
//! Regression evidence for context-menu arrow traversal and its empty-target seam.

use crate::app::menus::next_context_menu_target;
use crate::app::{ContextMenuKind, FocusTarget, IcedApp, Message};
use taskforest_core::core::session::{SessionIncarnation, SessionItem};
use taskmanager_application::AppPage;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

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

#[test]
fn stale_users_menu_keeps_only_close_in_keyboard_traversal() {
    let mut app = IcedApp::demo();
    let mut sessions = app.shell.projection().sessions.clone().unwrap_or_default();
    let session = sessions
        .get_mut(1)
        .expect("the demo Users projection has a second session");
    session.incarnation = Some(SessionIncarnation::LinuxLogind {
        boot_id: "fixture-boot".into(),
        created_monotonic_usec: 42,
    });
    let menu_session = session.clone();
    sessions.push(SessionItem {
        incarnation: menu_session.incarnation.clone(),
        ..menu_session
    });
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Sessions(Some(sessions)));
    let _ = app.update(Message::SelectPage(AppPage::Users));
    let _ = app.update(Message::OpenUserRowMenu(1));
    assert_eq!(app.context_menu_kind(), Some(ContextMenuKind::User));
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::UserRowMenuClose),
        "a duplicated identity must not open on a dead control action"
    );

    for previous in [false, true] {
        assert_eq!(
            app.context_menu_focus_menu_key(previous),
            Some(FocusTarget::UserRowMenuClose)
        );
        assert_eq!(
            app.input.focused_control,
            Some(FocusTarget::UserRowMenuClose)
        );
    }
}

#[test]
fn current_users_menu_traverses_actions_and_wraps_to_close() {
    let mut app = IcedApp::demo();
    let mut sessions = app.shell.projection().sessions.clone().unwrap_or_default();
    let session = sessions
        .get_mut(1)
        .expect("the demo Users projection has a second session");
    session.incarnation = Some(SessionIncarnation::LinuxLogind {
        boot_id: "fixture-boot".into(),
        created_monotonic_usec: 42,
    });
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Sessions(Some(sessions)));
    let _ = app.update(Message::SelectPage(AppPage::Users));
    let _ = app.update(Message::OpenUserRowMenu(1));
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::UserRowMenuDisconnect)
    );
    assert_eq!(
        app.context_menu_focus_menu_key(false),
        Some(FocusTarget::UserRowMenuLock)
    );
    assert_eq!(
        app.context_menu_focus_menu_key(false),
        Some(FocusTarget::UserRowMenuClose)
    );
    assert_eq!(
        app.context_menu_focus_menu_key(false),
        Some(FocusTarget::UserRowMenuDisconnect)
    );
}

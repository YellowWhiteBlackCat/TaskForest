use super::*;
use crate::{AppPage, FocusDirection, RefreshRequest, SelectionDirection};

fn router() -> CommandRouter {
    match default_router() {
        Ok(router) => router,
        Err(error) => panic!("default bindings conflict: {error:?}"),
    }
}

#[test]
fn default_shortcuts_route_to_exhaustive_typed_actions() {
    let context = CommandContext {
        scope: CommandScope::ProcessList,
        overlay_present: false,
        process_selected: true,
        text_input_focused: false,
    };
    let cases = [
        (KeyCode::F, Modifiers::CONTROL, AppAction::FocusSearch),
        (
            KeyCode::PageUp,
            Modifiers::NONE,
            AppAction::MoveSelection(SelectionDirection::PageUp),
        ),
        (
            KeyCode::PageDown,
            Modifiers::NONE,
            AppAction::MoveSelection(SelectionDirection::PageDown),
        ),
        (
            KeyCode::ArrowUp,
            Modifiers::NONE,
            AppAction::MoveSelection(SelectionDirection::Previous),
        ),
        (
            KeyCode::ArrowDown,
            Modifiers::NONE,
            AppAction::MoveSelection(SelectionDirection::Next),
        ),
        (
            KeyCode::Tab,
            Modifiers::NONE,
            AppAction::MoveFocus(FocusDirection::Next),
        ),
        (
            KeyCode::Tab,
            Modifiers::SHIFT,
            AppAction::MoveFocus(FocusDirection::Previous),
        ),
        (
            KeyCode::Digit1,
            Modifiers::ALT,
            AppAction::SelectPage(AppPage::Performance),
        ),
        (
            KeyCode::Digit2,
            Modifiers::ALT,
            AppAction::SelectPage(AppPage::Applications),
        ),
        (
            KeyCode::Digit3,
            Modifiers::ALT,
            AppAction::SelectPage(AppPage::Services),
        ),
        (
            KeyCode::Digit4,
            Modifiers::ALT,
            AppAction::SelectPage(AppPage::System),
        ),
        (
            KeyCode::Digit5,
            Modifiers::ALT,
            AppAction::SelectPage(AppPage::Startup),
        ),
        (
            KeyCode::Digit6,
            Modifiers::ALT,
            AppAction::SelectPage(AppPage::Users),
        ),
        (
            KeyCode::Digit7,
            Modifiers::ALT,
            AppAction::SelectPage(AppPage::AppHistory),
        ),
        // Alt+8 routes to the frontend-owned alerts surface: the reducer
        // acknowledges the action and the owning frontend presents its
        // page (Iced intercepts the chord in its navigation layer).
        (KeyCode::Digit8, Modifiers::ALT, AppAction::OpenAlerts),
        (
            KeyCode::F5,
            Modifiers::NONE,
            AppAction::Refresh(RefreshRequest::Processes),
        ),
        (KeyCode::Delete, Modifiers::NONE, AppAction::RequestEndTask),
        (KeyCode::Enter, Modifiers::NONE, AppAction::OpenProperties),
        (KeyCode::A, Modifiers::CONTROL, AppAction::OpenSystemAbout),
        (KeyCode::Space, Modifiers::CONTROL, AppAction::TogglePause),
        (KeyCode::F9, Modifiers::NONE, AppAction::ToggleSidebar),
        (
            KeyCode::Home,
            Modifiers::NONE,
            AppAction::MoveSelection(SelectionDirection::First),
        ),
        (
            KeyCode::End,
            Modifiers::NONE,
            AppAction::MoveSelection(SelectionDirection::Last),
        ),
    ];

    for (key, modifiers, expected) in cases {
        assert_eq!(
            router().route(KeyChord::new(key, modifiers), context),
            Some(expected)
        );
    }
    // Home/End are list-navigation jumps, so they must not fire while the
    // search field owns the keyboard.
    assert_eq!(
        router().route(
            KeyChord::new(KeyCode::Home, Modifiers::NONE),
            CommandContext {
                text_input_focused: true,
                ..context
            }
        ),
        None
    );
    assert_eq!(
        router().route(
            KeyChord::new(KeyCode::End, Modifiers::NONE),
            CommandContext {
                text_input_focused: true,
                ..context
            }
        ),
        None
    );
    assert_eq!(
        router().route(
            KeyChord::new(KeyCode::Escape, Modifiers::NONE),
            CommandContext {
                overlay_present: true,
                ..context
            }
        ),
        Some(AppAction::DismissOverlay)
    );
    assert_eq!(
        router().route(
            KeyChord::new(KeyCode::A, Modifiers::CONTROL),
            CommandContext::default(),
        ),
        Some(AppAction::OpenSystemAbout)
    );
    assert_eq!(
        router().route(
            KeyChord::new(KeyCode::A, Modifiers::CONTROL),
            CommandContext {
                text_input_focused: true,
                ..CommandContext::default()
            },
        ),
        None
    );
}

#[test]
fn enter_confirms_a_dialog_instead_of_opening_properties() {
    let router = router();
    let enter = KeyChord::new(KeyCode::Enter, Modifiers::NONE);
    // With the list active and no overlay, Enter opens properties (unchanged).
    let list = CommandContext {
        scope: CommandScope::ProcessList,
        overlay_present: false,
        process_selected: true,
        text_input_focused: false,
    };
    assert_eq!(
        router.resolve_command(enter, list),
        Some(CommandId::OpenProperties)
    );
    // While a confirmation dialog is open the scope becomes Dialog, so Enter
    // confirms instead of driving the list beneath the overlay.
    let dialog = CommandContext {
        scope: CommandScope::Dialog,
        overlay_present: true,
        process_selected: true,
        text_input_focused: false,
    };
    assert_eq!(
        router.resolve_command(enter, dialog),
        Some(CommandId::Confirm)
    );
    assert_eq!(CommandId::Confirm.action(), AppAction::ConfirmEndTask);
}

#[test]
fn text_input_keeps_focus_escape_but_blocks_editing_conflicts_and_dangerous_actions() {
    let input = CommandContext {
        scope: CommandScope::ProcessList,
        text_input_focused: true,
        process_selected: true,
        ..CommandContext::default()
    };
    assert_eq!(
        router().route(KeyChord::new(KeyCode::Tab, Modifiers::NONE), input),
        Some(AppAction::MoveFocus(FocusDirection::Next))
    );
    assert_eq!(
        router().route(KeyChord::new(KeyCode::Tab, Modifiers::SHIFT), input),
        Some(AppAction::MoveFocus(FocusDirection::Previous))
    );
    assert_eq!(
        router().route(KeyChord::new(KeyCode::Delete, Modifiers::NONE), input),
        None
    );

    let no_selection = CommandContext {
        scope: CommandScope::ProcessList,
        ..CommandContext::default()
    };
    assert_eq!(
        router().route(KeyChord::new(KeyCode::Enter, Modifiers::NONE), no_selection),
        None
    );
}

#[test]
fn overlapping_bindings_are_rejected_but_disjoint_scopes_are_allowed() {
    let chord = KeyChord::new(KeyCode::F5, Modifiers::NONE);
    let global = CommandBinding::new(CommandId::Refresh, chord, CommandScope::Global);
    let local = CommandBinding::new(CommandId::PageDown, chord, CommandScope::ProcessList);
    assert!(matches!(
        CommandRouter::try_new([global, local]),
        Err(RouterError::Conflict(_))
    ));

    let process = CommandBinding::new(CommandId::Refresh, chord, CommandScope::ProcessList);
    let dialog = CommandBinding::new(CommandId::Dismiss, chord, CommandScope::Dialog);
    assert!(CommandRouter::try_new([process, dialog]).is_ok());
}

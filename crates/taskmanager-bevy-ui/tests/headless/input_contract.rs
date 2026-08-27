use super::*;
use taskmanager_application::{AppAction, AppPage, CommandContext, CommandScope, FocusDirection};

fn modifiers(control: bool, alt: bool, shift: bool) -> InputModifiers {
    InputModifiers {
        control,
        alt,
        shift,
        platform: false,
    }
}

#[test]
fn bevy_page_keys_use_the_shared_router_and_reject_wrong_chords() {
    let context = CommandContext {
        scope: CommandScope::Global,
        ..CommandContext::default()
    };
    assert_eq!(
        normalize_key(KeyCode::Digit2, modifiers(false, true, false), context),
        Some(AppAction::SelectPage(AppPage::Applications))
    );
    assert_eq!(
        normalize_key(KeyCode::Digit2, InputModifiers::default(), context),
        None
    );
}

#[test]
fn focus_traversal_and_process_shortcuts_honor_scope_and_ime_ownership() {
    let global = CommandContext {
        scope: CommandScope::Global,
        ..CommandContext::default()
    };
    assert_eq!(
        normalize_key(KeyCode::Tab, modifiers(false, false, true), global),
        Some(AppAction::MoveFocus(FocusDirection::Previous))
    );

    let process_list = CommandContext {
        scope: CommandScope::ProcessList,
        process_selected: true,
        ..CommandContext::default()
    };
    assert_eq!(
        normalize_key(KeyCode::Delete, InputModifiers::default(), process_list),
        Some(AppAction::RequestEndTask)
    );

    let ime_focused = CommandContext {
        text_input_focused: true,
        ..process_list
    };
    assert_eq!(
        normalize_key(KeyCode::Delete, InputModifiers::default(), ime_focused),
        None
    );
    assert_eq!(
        normalize_key(KeyCode::ArrowDown, InputModifiers::default(), ime_focused),
        None
    );
}

#[test]
fn semantic_addresses_and_ime_state_are_stable_across_rebuilds() {
    let first = stable_semantic_address("process-row", "pid:42");
    let second = stable_semantic_address("process-row", "pid:42");
    assert_eq!(first, second);
    assert_ne!(first, stable_semantic_address("process-row", "pid:43"));

    let mut ime = ImeOwnership::default();
    assert!(!ime.owns_keyboard(&first));
    ime.begin(first.clone());
    assert!(ime.owns_keyboard(&first));
    assert!(ime.composing());
    ime.finish_composition();
    assert!(!ime.composing());
    ime.clear();
    assert!(!ime.owns_keyboard(&first));
}

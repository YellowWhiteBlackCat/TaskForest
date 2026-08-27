use super::*;
use taskmanager_application::{CommandScope, FocusDirection};

#[test]
fn tab_routes_while_terminal_input_is_focused() {
    let action = route_key(
        ShellKeyEvent::new(KeyCode::Tab, Modifiers::NONE),
        CommandContext {
            scope: CommandScope::Shell,
            text_input_focused: true,
            ..CommandContext::default()
        },
    );
    assert_eq!(action, Some(AppAction::MoveFocus(FocusDirection::Next)));
}

#[test]
fn process_commands_keep_their_context_guard() {
    let event = ShellKeyEvent::new(KeyCode::Delete, Modifiers::NONE);
    assert_eq!(route_key(event, CommandContext::default()), None);
    assert_eq!(
        route_key(
            event,
            CommandContext {
                scope: CommandScope::ProcessList,
                process_selected: true,
                ..CommandContext::default()
            }
        ),
        Some(AppAction::RequestEndTask)
    );
}

#[test]
fn shell_local_bindings_are_unique_and_document_the_wired_chords() {
    let bindings = shell_local_bindings();
    assert!(!bindings.is_empty());
    for binding in bindings {
        assert!(!binding.shortcut.is_empty(), "shortcut must not be empty");
        assert!(!binding.label.is_empty(), "label must not be empty");
    }
    let mut shortcuts: Vec<&str> = bindings.iter().map(|binding| binding.shortcut).collect();
    shortcuts.sort_unstable();
    let unique = shortcuts.len();
    shortcuts.dedup();
    assert_eq!(
        shortcuts.len(),
        unique,
        "terminal-only shortcuts must be unique"
    );
    for expected in ["q", "?", "s", "S", "T"] {
        assert!(
            bindings.iter().any(|binding| binding.shortcut == expected),
            "expected terminal binding {expected} to be documented"
        );
    }
}

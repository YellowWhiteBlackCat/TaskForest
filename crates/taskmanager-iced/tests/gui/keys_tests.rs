use super::*;
use taskmanager_shell::route_key;

#[test]
fn fixed_keys_map_onto_the_shared_vocabulary() {
    assert_eq!(
        map_key(
            &Key::Named(Named::Escape),
            iced::keyboard::Modifiers::default()
        ),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Escape, Modifiers::NONE))
    );
    assert_eq!(
        map_key(
            &Key::Named(Named::ArrowDown),
            iced::keyboard::Modifiers::default()
        ),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::ArrowDown, Modifiers::NONE))
    );
    assert_eq!(
        map_key(&Key::Named(Named::F5), iced::keyboard::Modifiers::default()),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::F5, Modifiers::NONE))
    );
    assert_eq!(
        map_key(&Key::Named(Named::F9), iced::keyboard::Modifiers::default()),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::F9, Modifiers::NONE))
    );
    assert_eq!(
        map_key(
            &Key::Named(Named::Home),
            iced::keyboard::Modifiers::default()
        ),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Home, Modifiers::NONE))
    );
    assert_eq!(
        map_key(
            &Key::Named(Named::End),
            iced::keyboard::Modifiers::default()
        ),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::End, Modifiers::NONE))
    );
}

#[test]
fn help_and_expand_keys_reach_the_shared_vocabulary() {
    // F1 (help) and the arrow pair (tree/group expand-collapse) must
    // survive as fixed keys so the frontend's visual-navigation layer can
    // act on them (the shared router deliberately has no binding).
    assert_eq!(
        map_key(&Key::Named(Named::F1), iced::keyboard::Modifiers::default()),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::F1, Modifiers::NONE))
    );
    assert_eq!(
        map_key(
            &Key::Named(Named::ArrowLeft),
            iced::keyboard::Modifiers::default()
        ),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::ArrowLeft, Modifiers::NONE))
    );
    assert_eq!(
        map_key(
            &Key::Named(Named::ArrowRight),
            iced::keyboard::Modifiers::default()
        ),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::ArrowRight, Modifiers::NONE))
    );
}

#[test]
fn alt_digit_chords_select_pages() {
    let alt = iced::keyboard::Modifiers::ALT;
    assert_eq!(
        map_key(&Key::Character("2".into()), alt),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit2, Modifiers::ALT))
    );
    // Alt+7 must reach the App-history page (Digit7) — the shared
    // command_help advertises it, so the chord must not fall through to a
    // bare '7' character (the prior gap that made the help sheet lie).
    assert_eq!(
        map_key(&Key::Character("7".into()), alt),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit7, Modifiers::ALT))
    );
    assert_ne!(
        map_key(&Key::Character("7".into()), alt),
        IcedKey::Character('7', Modifiers::ALT)
    );
    // Alt+8 must reach the shared vocabulary too (the alerts route chord
    // the router registers as ShowAlerts).
    assert_eq!(
        map_key(&Key::Character("8".into()), alt),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit8, Modifiers::ALT))
    );
}

#[test]
fn ctrl_chords_and_characters_route() {
    let ctrl = iced::keyboard::Modifiers::CTRL;
    assert_eq!(
        map_key(&Key::Character("f".into()), ctrl),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::F, Modifiers::CONTROL))
    );
    assert_eq!(
        map_key(&Key::Character("c".into()), ctrl),
        IcedKey::Fixed(ShellKeyEvent::new(KeyCode::C, Modifiers::CONTROL))
    );
    // Plain characters reach the shell's local bindings / search input.
    assert_eq!(
        map_key(
            &Key::Character("q".into()),
            iced::keyboard::Modifiers::default()
        ),
        IcedKey::Character('q', Modifiers::NONE)
    );
    // Unmappable keys (modifier keys themselves) are ignored.
    assert_eq!(
        map_key(
            &Key::Named(Named::Control),
            iced::keyboard::Modifiers::default()
        ),
        IcedKey::Other
    );
}

#[test]
fn mapped_fixed_keys_survive_the_shared_router() {
    let event = ShellKeyEvent::new(KeyCode::Tab, Modifiers::NONE);
    let action = route_key(
        event,
        taskmanager_application::CommandContext {
            scope: taskmanager_application::CommandScope::Shell,
            text_input_focused: true,
            ..taskmanager_application::CommandContext::default()
        },
    );
    assert_eq!(
        action,
        Some(taskmanager_application::AppAction::MoveFocus(
            taskmanager_application::FocusDirection::Next
        ))
    );
}

//! iced keyboard events normalized into the shared shell key vocabulary
//! (ADR-027). Each frontend maps its native event shape into
//! `taskmanager-shell`'s [`ShellKeyEvent`]; this module is the iced half of
//! that mapping.

use iced::keyboard::Key;
use iced::keyboard::key::Named;
use taskmanager_application::{KeyCode, Modifiers};

use taskmanager_shell::ShellKeyEvent;

/// The normalized outcome of one iced key press: a fixed key that routes
/// through the shared vocabulary, a character for the shell's local
/// bindings/search input, or nothing (unmappable keys are ignored).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcedKey {
    /// A fixed key (arrows, page keys, enter, escape, …).
    Fixed(ShellKeyEvent),
    /// A printable character plus the shared modifier state.
    Character(char, Modifiers),
    /// A key with no shared meaning (e.g. modifier keys themselves).
    Other,
}

/// Map the shared modifier state from iced's bit flags.
#[must_use]
pub fn modifiers(iced: iced::keyboard::Modifiers) -> Modifiers {
    Modifiers::new(iced.control(), iced.alt(), iced.shift(), iced.logo())
}

/// Map one iced key event onto the shared vocabulary.
#[must_use]
pub fn map_key(key: &Key, iced_modifiers: iced::keyboard::Modifiers) -> IcedKey {
    let shared_modifiers = modifiers(iced_modifiers);
    let code = match key {
        Key::Named(Named::Enter) => KeyCode::Enter,
        Key::Named(Named::Escape) => KeyCode::Escape,
        Key::Named(Named::Tab) => KeyCode::Tab,
        Key::Named(Named::Backspace) => return IcedKey::Other, // handled per-frontend
        Key::Named(Named::Delete) => KeyCode::Delete,
        Key::Named(Named::ArrowUp) => KeyCode::ArrowUp,
        Key::Named(Named::ArrowDown) => KeyCode::ArrowDown,
        Key::Named(Named::ArrowLeft) => KeyCode::ArrowLeft,
        Key::Named(Named::ArrowRight) => KeyCode::ArrowRight,
        Key::Named(Named::PageUp) => KeyCode::PageUp,
        Key::Named(Named::PageDown) => KeyCode::PageDown,
        Key::Named(Named::Home) => KeyCode::Home,
        Key::Named(Named::End) => KeyCode::End,
        Key::Named(Named::F1) => KeyCode::F1,
        Key::Named(Named::F5) => KeyCode::F5,
        Key::Named(Named::F9) => KeyCode::F9,
        Key::Character(character) => {
            let mut chars = character.chars();
            let Some(character) = chars.next() else {
                return IcedKey::Other;
            };
            if chars.next().is_some() {
                return IcedKey::Other;
            }
            return match character {
                '1' if iced_modifiers.alt() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit1, shared_modifiers))
                }
                '2' if iced_modifiers.alt() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit2, shared_modifiers))
                }
                '3' if iced_modifiers.alt() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit3, shared_modifiers))
                }
                '4' if iced_modifiers.alt() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit4, shared_modifiers))
                }
                '5' if iced_modifiers.alt() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit5, shared_modifiers))
                }
                '6' if iced_modifiers.alt() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit6, shared_modifiers))
                }
                '7' if iced_modifiers.alt() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit7, shared_modifiers))
                }
                '8' if iced_modifiers.alt() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Digit8, shared_modifiers))
                }
                ' ' if iced_modifiers.control() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::Space, shared_modifiers))
                }
                'f' | 'F' if iced_modifiers.control() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::F, shared_modifiers))
                }
                'c' | 'C' if iced_modifiers.control() => {
                    IcedKey::Fixed(ShellKeyEvent::new(KeyCode::C, shared_modifiers))
                }
                _ => IcedKey::Character(character, shared_modifiers),
            };
        }
        _ => return IcedKey::Other,
    };
    IcedKey::Fixed(ShellKeyEvent::new(code, shared_modifiers))
}

#[cfg(test)]
#[path = "../tests/gui/keys_tests.rs"]
mod tests;

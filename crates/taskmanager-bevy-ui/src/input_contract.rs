//! Bevy-native input normalization with shared command and semantic identity.
//!
//! Native key values are translated once into the application router. The
//! shared command context carries text-input ownership, so list shortcuts are
//! rejected while an editor owns the keyboard. Rendered rows use stable
//! semantic IDs rather than toolkit handles.

use bevy::ecs::component::Component;
use bevy::input::keyboard::KeyCode;
use taskmanager_application::{AppAction, CommandContext, Modifiers};

use taskmanager_shell::{ShellKeyEvent, route_key};
use taskmanager_ui_contract::SemanticNodeId;

/// Modifier state at the Bevy boundary. It deliberately has the same four
/// axes as the shared command vocabulary, including platform/super.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct InputModifiers {
    pub(crate) control: bool,
    pub(crate) alt: bool,
    pub(crate) shift: bool,
    pub(crate) platform: bool,
}

impl InputModifiers {
    #[must_use]
    pub(crate) const fn shared(self) -> Modifiers {
        Modifiers::new(self.control, self.alt, self.shift, self.platform)
    }
}

/// Map the Bevy physical key vocabulary into the single application key
/// vocabulary. Unlisted keys remain frontend-local and are not guessed.
#[must_use]
pub(crate) fn shared_key(key: KeyCode) -> Option<taskmanager_application::KeyCode> {
    Some(match key {
        KeyCode::KeyF => taskmanager_application::KeyCode::F,
        KeyCode::KeyA => taskmanager_application::KeyCode::A,
        KeyCode::KeyC => taskmanager_application::KeyCode::C,
        KeyCode::Digit1 => taskmanager_application::KeyCode::Digit1,
        KeyCode::Digit2 => taskmanager_application::KeyCode::Digit2,
        KeyCode::Digit3 => taskmanager_application::KeyCode::Digit3,
        KeyCode::Digit4 => taskmanager_application::KeyCode::Digit4,
        KeyCode::Digit5 => taskmanager_application::KeyCode::Digit5,
        KeyCode::Digit6 => taskmanager_application::KeyCode::Digit6,
        KeyCode::Digit7 => taskmanager_application::KeyCode::Digit7,
        KeyCode::Digit8 => taskmanager_application::KeyCode::Digit8,
        KeyCode::PageUp => taskmanager_application::KeyCode::PageUp,
        KeyCode::PageDown => taskmanager_application::KeyCode::PageDown,
        KeyCode::ArrowUp => taskmanager_application::KeyCode::ArrowUp,
        KeyCode::ArrowDown => taskmanager_application::KeyCode::ArrowDown,
        KeyCode::Tab => taskmanager_application::KeyCode::Tab,
        KeyCode::F5 => taskmanager_application::KeyCode::F5,
        KeyCode::F9 => taskmanager_application::KeyCode::F9,
        KeyCode::Delete => taskmanager_application::KeyCode::Delete,
        KeyCode::Enter => taskmanager_application::KeyCode::Enter,
        KeyCode::Escape => taskmanager_application::KeyCode::Escape,
        KeyCode::Space => taskmanager_application::KeyCode::Space,
        KeyCode::Home => taskmanager_application::KeyCode::Home,
        KeyCode::End => taskmanager_application::KeyCode::End,
        _ => return None,
    })
}

/// Normalize one Bevy key event and route it through the shared command
/// table. The caller supplies the active scope and text/overlay facts; the
/// frontend never bypasses those enable rules with a local match arm.
#[must_use]
pub(crate) fn normalize_key(
    key: KeyCode,
    modifiers: InputModifiers,
    context: CommandContext,
) -> Option<AppAction> {
    let key = shared_key(key)?;
    route_key(ShellKeyEvent::new(key, modifiers.shared()), context)
}

/// Stable semantic identity for an input and accessibility node. The formal
/// route uses this marker on tree rows; the semantic module maps the same
/// identity into the native AccessKit projection.
#[must_use]
pub(crate) fn stable_semantic_address(namespace: &str, identity: &str) -> SemanticNodeId {
    SemanticNodeId::owned(format!("{namespace}:{identity}"))
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SemanticAddress(pub(crate) SemanticNodeId);

impl Default for SemanticAddress {
    fn default() -> Self {
        // `bsn!` needs a template value; every mounted row immediately
        // patches this sentinel with its typed stable identity.
        Self(stable_semantic_address("unset", "unset"))
    }
}

#[cfg(test)]
#[path = "../tests/headless/input_contract.rs"]
mod tests;

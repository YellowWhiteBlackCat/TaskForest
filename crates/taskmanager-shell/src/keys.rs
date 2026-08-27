//! Renderer-neutral shell key event and routing through the shared
//! conflict-checked command router (ADR-027). Frontends normalize their
//! native event into [`ShellKeyEvent`] before it reaches the router.

use taskmanager_application::{
    AppAction, CommandContext, KeyChord, KeyCode, Modifiers, default_router,
};

/// Renderer-neutral frontend key event. Each frontend normalizes its native
/// event into this value before it reaches the application command router.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellKeyEvent {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl ShellKeyEvent {
    #[must_use]
    pub const fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    #[must_use]
    pub const fn chord(self) -> KeyChord {
        KeyChord::new(self.key, self.modifiers)
    }
}

/// Route one frontend key through the same conflict-checked command table
/// used by every frontend.
#[must_use]
pub fn route_key(event: ShellKeyEvent, context: CommandContext) -> Option<AppAction> {
    default_router().ok()?.route(event.chord(), context)
}

/// A frontend-local keybinding: a chord plus a short label. These are handled
/// directly in each frontend's input loop rather than the shared command
/// router, because they have no entry in the shared `KeyCode` vocabulary
/// (`?`, `q`, `s`, …).
#[derive(Clone, Copy, Debug)]
pub struct LocalBinding {
    pub shortcut: &'static str,
    pub label: &'static str,
}

/// Every terminal-only keybinding the TUI actually wires. The help overlay
/// combines these with the router-derived shared bindings
/// ([`crate::presentation::command_help`]) so it can never advertise a chord that is not
/// genuinely reachable from this frontend.
#[must_use]
pub const fn shell_local_bindings() -> &'static [LocalBinding] {
    &[
        LocalBinding {
            shortcut: "q",
            label: "Quit TaskForest",
        },
        LocalBinding {
            shortcut: "?",
            label: "Toggle this help",
        },
        LocalBinding {
            shortcut: "s",
            label: "Cycle sort column",
        },
        LocalBinding {
            shortcut: "S",
            label: "Reverse sort direction",
        },
        LocalBinding {
            shortcut: "T",
            label: "Toggle threshold suggestions",
        },
    ]
}

#[cfg(test)]
#[path = "../tests/headless/shell_keys.rs"]
mod tests;

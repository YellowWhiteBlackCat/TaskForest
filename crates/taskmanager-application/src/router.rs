//! Context-aware, conflict-checked command routing.

use crate::command::spec::COMMAND_SPECS;
use crate::{AppAction, CommandId, KeyChord, KeyCode, Modifiers};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CommandScope {
    Global,
    #[default]
    Shell,
    ProcessList,
    Dialog,
}

/// Runtime facts used to enable commands without consulting toolkit state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandContext {
    pub scope: CommandScope,
    pub text_input_focused: bool,
    /// Independent routing fact derived from the active typed surface.
    pub overlay_present: bool,
    pub process_selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandBinding {
    pub command: CommandId,
    pub chord: KeyChord,
    pub scope: CommandScope,
}

impl CommandBinding {
    #[must_use]
    pub const fn new(command: CommandId, chord: KeyChord, scope: CommandScope) -> Self {
        Self {
            command,
            chord,
            scope,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandConflict {
    pub chord: KeyChord,
    pub first: CommandId,
    pub second: CommandId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouterError {
    Conflict(CommandConflict),
}

#[derive(Clone, Debug)]
pub struct CommandRouter {
    bindings: Vec<CommandBinding>,
}

impl CommandRouter {
    pub fn try_new(
        bindings: impl IntoIterator<Item = CommandBinding>,
    ) -> Result<Self, RouterError> {
        let bindings: Vec<_> = bindings.into_iter().collect();
        for (index, first) in bindings.iter().enumerate() {
            for second in &bindings[index + 1..] {
                if first.chord == second.chord && scopes_overlap(first.scope, second.scope) {
                    return Err(RouterError::Conflict(CommandConflict {
                        chord: first.chord,
                        first: first.command,
                        second: second.command,
                    }));
                }
            }
        }
        Ok(Self { bindings })
    }

    #[must_use]
    pub fn bindings(&self) -> &[CommandBinding] {
        &self.bindings
    }

    #[must_use]
    pub fn resolve_command(&self, chord: KeyChord, context: CommandContext) -> Option<CommandId> {
        self.bindings
            .iter()
            .find(|binding| {
                binding.chord == chord
                    && scope_matches(binding.scope, context.scope)
                    && command_enabled(binding.command, context)
            })
            .map(|binding| binding.command)
    }

    /// Resolve a chord directly to the typed action consumed by [`crate::reduce`].
    #[must_use]
    pub fn route(&self, chord: KeyChord, context: CommandContext) -> Option<AppAction> {
        self.resolve_command(chord, context).map(CommandId::action)
    }
}

fn scope_matches(binding: CommandScope, active: CommandScope) -> bool {
    binding == CommandScope::Global || binding == active
}

fn scopes_overlap(first: CommandScope, second: CommandScope) -> bool {
    first == CommandScope::Global || second == CommandScope::Global || first == second
}

fn command_enabled(command: CommandId, context: CommandContext) -> bool {
    command.spec().enable.allows(context)
}

/// The default binding surface, derived row-by-row from the single command
/// spec table ([`crate::command::spec::COMMAND_SPECS`]) — no literal array
/// length and no second per-command list.
const DEFAULT_BINDINGS: [CommandBinding; COMMAND_SPECS.len()] = {
    let mut bindings = [CommandBinding::new(
        CommandId::FocusSearch,
        KeyChord::new(KeyCode::F, Modifiers::NONE),
        CommandScope::Shell,
    ); COMMAND_SPECS.len()];
    let mut index = 0;
    while index < COMMAND_SPECS.len() {
        bindings[index] = COMMAND_SPECS[index].binding();
        index += 1;
    }
    bindings
};

#[must_use]
pub const fn default_bindings() -> &'static [CommandBinding] {
    &DEFAULT_BINDINGS
}

pub fn default_router() -> Result<CommandRouter, RouterError> {
    CommandRouter::try_new(DEFAULT_BINDINGS)
}

#[cfg(test)]
#[path = "../tests/headless/application_router_tests.rs"]
mod tests;

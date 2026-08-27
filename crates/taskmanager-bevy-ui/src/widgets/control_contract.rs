//! Typed, fail-closed control and confirmation state for Bevy widgets.
//!
//! A dialog freezes its target when opened. Navigation may change the chosen
//! verb, but it cannot re-read a live row and silently retarget the action.
//! Disabled choices remain visible and cannot produce an authorization.

#![allow(dead_code)]

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::{EntityEvent, Event};
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, Node, percent};

use crate::palette::UiPalette;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ControlTarget {
    Process(u32),
    Service(String),
    Startup(String),
    Session(String),
}

impl ControlTarget {
    #[must_use]
    pub(crate) fn stable_key(&self) -> String {
        match self {
            Self::Process(pid) => format!("process:{pid}"),
            Self::Service(name) => format!("service:{name}"),
            Self::Startup(name) => format!("startup:{name}"),
            Self::Session(name) => format!("session:{name}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ControlVerb {
    Terminate,
    ServiceStart,
    ServiceStop,
    ServiceRestart,
    StartupEnable,
    StartupDisable,
    SessionDisconnect,
    SessionLock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrozenControl {
    target: ControlTarget,
    verb: ControlVerb,
}

impl FrozenControl {
    #[must_use]
    pub(crate) fn target(&self) -> &ControlTarget {
        &self.target
    }

    #[must_use]
    pub(crate) const fn verb(&self) -> ControlVerb {
        self.verb
    }

    #[must_use]
    pub(crate) fn target_key(&self) -> String {
        self.target.stable_key()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ControlChoice {
    pub(crate) verb: ControlVerb,
    pub(crate) enabled: bool,
}

impl ControlChoice {
    #[must_use]
    pub(crate) const fn enabled(verb: ControlVerb) -> Self {
        Self {
            verb,
            enabled: true,
        }
    }

    #[must_use]
    pub(crate) const fn disabled(verb: ControlVerb) -> Self {
        Self {
            verb,
            enabled: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlInput {
    Up,
    Down,
    Confirm,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ControlOutcome {
    Confirmed(FrozenControl),
    Canceled(FrozenControl),
}

/// Stateful keyboard contract for a confirmation surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ControlSession {
    frozen_target: ControlTarget,
    initial_control: FrozenControl,
    choices: Vec<ControlChoice>,
    selection: usize,
}

impl ControlSession {
    #[must_use]
    pub(crate) fn new(target: ControlTarget, choices: Vec<ControlChoice>) -> Self {
        let initial_verb = choices
            .first()
            .map_or(ControlVerb::Terminate, |choice| choice.verb);
        Self {
            frozen_target: target.clone(),
            initial_control: FrozenControl {
                target,
                verb: initial_verb,
            },
            choices,
            selection: 0,
        }
    }

    #[must_use]
    pub(crate) fn target(&self) -> &ControlTarget {
        &self.frozen_target
    }

    #[must_use]
    pub(crate) const fn selection(&self) -> usize {
        self.selection
    }

    #[must_use]
    pub(crate) fn selected_choice(&self) -> Option<ControlChoice> {
        self.choices.get(self.selection).copied()
    }

    /// Advance the dialog without ever mutating its frozen target.
    pub(crate) fn advance(&mut self, input: ControlInput) -> Option<ControlOutcome> {
        match input {
            ControlInput::Up => {
                self.selection = self.selection.saturating_sub(1);
                None
            }
            ControlInput::Down => {
                self.selection = (self.selection + 1).min(self.choices.len().saturating_sub(1));
                None
            }
            ControlInput::Cancel => Some(ControlOutcome::Canceled(self.initial_control.clone())),
            ControlInput::Confirm => {
                let choice = self.selected_choice()?;
                if !choice.enabled {
                    return None;
                }
                Some(ControlOutcome::Confirmed(FrozenControl {
                    target: self.frozen_target.clone(),
                    verb: choice.verb,
                }))
            }
        }
    }
}

/// Targeted input event consumed by a future observer on the dialog root.
#[derive(Clone, Debug, EntityEvent)]
pub(crate) struct ControlInputEvent {
    pub(crate) entity: Entity,
    pub(crate) input: ControlInput,
}

/// Published outcome. The Applications tree mounts the surface marker; the
/// application layer remains the only authority that turns a typed result into
/// a platform request.
#[derive(Clone, Debug, Event)]
pub(crate) struct ControlResolved(pub(crate) ControlOutcome);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ControlSurface;

pub(crate) fn control_surface_scene(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100) }
        BackgroundColor({ palette.panel_fill })
        ControlSurface
    }
}

#[cfg(test)]
#[path = "../../tests/headless/control_contract.rs"]
mod tests;

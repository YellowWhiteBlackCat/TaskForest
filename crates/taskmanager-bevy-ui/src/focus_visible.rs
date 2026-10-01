//! Modality-aware high-contrast keyboard focus indication (CORE-08 / FocusVisible).
//!
//! Delivers true parity with GPUI (`RootView::InputModality`) and Iced (`input_modality::InputModality`):
//! interactive controls and navigation items only paint a high-contrast focus ring when navigating
//! via keyboard. Pointer clicks switch the modality to [`InputModality::Pointer`], suppressing the focus
//! ring immediately to avoid distracting outlines on mouse activation.
//!
//! # Architecture
//!
//! Strictly governed by an explicit discrete state machine ([`InputModalityState`]) with discrete three-state
//! transitions ([`InputModality`]: `Programmatic`, `Keyboard`, `Pointer`). Zero scattered booleans.
//! Modality transitions trigger the [`InputModalityChanged`] observer, which synchronizes [`FocusRingVisible`]
//! on focused entities without frame-by-frame polling.

use bevy::app::{App, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::message::MessageReader;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Changed, Has, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::picking::hover::PickingInteraction;
use bevy::ui::BorderColor;

use crate::window::WindowPalette;

/// The most recent origin capable of changing focus in the window.
///
/// Strictly mirrors the GPUI root tracker (`RootView::InputModality`) and
/// the Iced renderer-local tracker (`taskmanager_iced::input_modality::InputModality`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub(crate) enum InputModality {
    /// Initial state and focus changes initiated directly by application code.
    #[default]
    Programmatic = 0,
    /// A keyboard event was observed (any key or modifier).
    Keyboard = 1,
    /// A pointer event (press/click) was observed over the window surface.
    Pointer = 2,
}

impl InputModality {
    /// Strict focus-visible policy: only keyboard modality paints a focus ring,
    /// matching the shared theme palette contract where ring alpha encodes focus-visible.
    #[must_use]
    pub(crate) const fn shows_focus_ring(self) -> bool {
        matches!(self, Self::Keyboard)
    }
}

/// Explicit discrete state machine resource tracking the active input modality.
#[derive(Resource, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct InputModalityState {
    modality: InputModality,
}

impl InputModalityState {
    #[must_use]
    pub(crate) fn modality(&self) -> InputModality {
        self.modality
    }

    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub(crate) fn shows_focus_ring(&self) -> bool {
        self.modality.shows_focus_ring()
    }

    /// Transition to a new input modality.
    /// Returns true if the state changed, allowing caller to trigger an observer event.
    pub(crate) fn transition(&mut self, next: InputModality) -> bool {
        if self.modality == next {
            return false;
        }
        self.modality = next;
        true
    }
}

/// Observer event triggered when the input modality transitions between discrete states.
#[derive(Event, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InputModalityChanged {
    pub(crate) previous: InputModality,
    pub(crate) current: InputModality,
}

/// Marker component on interactive elements that participate in modality-aware focus rings.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRing;

/// Logical focus marker component placed on the active focused control.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusedControl;

/// Component mounted when a focus ring is actively visible on the entity.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRingVisible;

/// Query of focus-ring participants consumed by the observer below.
type FocusRingQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        Has<FocusedControl>,
        Option<&'static FocusRingVisible>,
        Option<&'static mut BorderColor>,
    ),
    With<FocusRing>,
>;

/// Observer responding to input modality transitions: updates active [`FocusRingVisible`] components
/// and syncs border colors on controls with [`FocusRing`].
pub(crate) fn on_input_modality_changed(
    event: On<InputModalityChanged>,
    palette: Option<Res<WindowPalette>>,
    mut query: FocusRingQuery,
    mut commands: Commands,
) {
    let shows_ring = event.event().current.shows_focus_ring();
    let ring_color = palette
        .as_ref()
        .map(|p| p.inner.focus_ring)
        .unwrap_or(bevy::color::Color::WHITE);
    let default_border = palette
        .as_ref()
        .map(|p| p.inner.border_color)
        .unwrap_or(bevy::color::Color::NONE);

    for (entity, has_focus, is_ring_visible, border_color) in &mut query {
        let should_show = has_focus && shows_ring;
        match (should_show, is_ring_visible.is_some()) {
            (true, false) => {
                commands.entity(entity).insert(FocusRingVisible);
                if let Some(mut border) = border_color {
                    *border = BorderColor::all(ring_color);
                }
            }
            (false, true) => {
                commands.entity(entity).remove::<FocusRingVisible>();
                if let Some(mut border) = border_color {
                    *border = BorderColor::all(default_border);
                }
            }
            _ => {}
        }
    }
}

/// System observing keyboard activity: any keyboard press transitions the modality to [`InputModality::Keyboard`].
pub(crate) fn focus_visible_keyboard_system(
    mut modality: ResMut<InputModalityState>,
    mut commands: Commands,
    mut key_events: MessageReader<KeyboardInput>,
) {
    for event in key_events.read() {
        if event.state == ButtonState::Pressed {
            let prev = modality.modality();
            if modality.transition(InputModality::Keyboard) {
                commands.trigger(InputModalityChanged {
                    previous: prev,
                    current: InputModality::Keyboard,
                });
            }
            break;
        }
    }
}

/// System observing pointer activity: pointer presses transition the modality to [`InputModality::Pointer`].
pub(crate) fn focus_visible_pointer_system(
    mut modality: ResMut<InputModalityState>,
    mut commands: Commands,
    query: Query<&PickingInteraction, Changed<PickingInteraction>>,
) {
    for interaction in &query {
        if *interaction == PickingInteraction::Pressed {
            let prev = modality.modality();
            if modality.transition(InputModality::Pointer) {
                commands.trigger(InputModalityChanged {
                    previous: prev,
                    current: InputModality::Pointer,
                });
            }
            break;
        }
    }
}

/// Register focus-visible modality resources, observer, and input monitoring systems.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<InputModalityState>();
    app.add_observer(on_input_modality_changed);
    app.add_systems(
        Update,
        (focus_visible_keyboard_system, focus_visible_pointer_system),
    );
}

#[cfg(test)]
#[path = "../tests/headless/focus_visible.rs"]
mod tests;

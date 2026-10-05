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
//! Actual widgets opt into toolkit Tab navigation; the single [`InputFocus`] resource
//! selects the one control whose outline is painted. No second focus marker exists.

use bevy::app::{App, PostUpdate, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::message::MessageReader;
use bevy::ecs::query::{Changed, Has, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::ecs::world::World;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::keyboard::{Key, KeyCode};
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin};
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::picking::hover::PickingInteraction;
use bevy::ui::{Node, Outline, Val};
use bevy::ui_widgets::ScrollIntoView;
use bevy::ui_widgets::{Button, Checkbox, RadioButton};
use bevy::window::PrimaryWindow;

use crate::confirmation::ConfirmationOverlay;
use crate::pages::processes::properties_modal::ProcessPropertiesOverlay;
use crate::window::{AppShellRoot, WindowPalette};

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

    #[must_use]
    pub(crate) fn shows_focus_ring(&self) -> bool {
        self.modality().shows_focus_ring()
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

/// A real focusable widget, decorated without changing its layout footprint.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRing;

/// Marker for the actual control whose keyboard outline is currently painted.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRingVisible;

type UndecoratedControls<'w, 's> = Query<
    'w,
    's,
    (Entity, Has<Checkbox>, Has<RadioButton>),
    (
        With<Node>,
        Without<FocusRing>,
        Or<(With<Button>, With<Checkbox>, With<RadioButton>)>,
    ),
>;

fn decorate_controls(mut commands: Commands, controls: UndecoratedControls) {
    for (entity, checkbox, radio) in &controls {
        // Text switches have no opaque fill, so an inset ring remains visible
        // and stays inside the scroll viewport. Filled buttons need an outset.
        let offset = if checkbox || radio { -2.0 } else { 0.0 };
        commands.entity(entity).insert((
            FocusRing,
            TabIndex(0),
            Outline::new(Val::Px(2.0), Val::Px(offset), bevy::color::Color::NONE),
        ));
    }
}

type UndecoratedModals<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Without<TabGroup>,
        Or<(With<ConfirmationOverlay>, With<ProcessPropertiesOverlay>)>,
    ),
>;

type FocusableControls<'w, 's> =
    Query<'w, 's, (), Or<(With<Button>, With<Checkbox>, With<RadioButton>)>>;

fn decorate_groups(
    mut commands: Commands,
    roots: Query<Entity, (With<AppShellRoot>, Without<TabGroup>)>,
    modals: UndecoratedModals,
    children: Query<&Children>,
    controls: FocusableControls,
) {
    for entity in &roots {
        commands.entity(entity).insert(TabGroup::new(0));
    }
    for entity in &modals {
        commands.entity(entity).insert(TabGroup::modal());
        if let Some(first) = children
            .iter_descendants(entity)
            .find(|child| controls.contains(*child))
        {
            commands.entity(first).insert(AutoFocus);
        }
    }
}

/// Toolkit focus is the authority; moving focus updates the ring even when
/// the input modality stays Keyboard across successive Tab presses.
fn paint_focus_ring(
    focus: Res<InputFocus>,
    modality: Res<InputModalityState>,
    palette: Option<Res<WindowPalette>>,
    mut controls: Query<(Entity, &mut Outline, Option<&FocusRingVisible>), With<FocusRing>>,
    mut commands: Commands,
) {
    let color = palette
        .as_ref()
        .map_or(bevy::color::Color::WHITE, |p| p.inner.focus_ring);
    for (entity, mut outline, visible) in &mut controls {
        let show = focus.get() == Some(entity) && modality.shows_focus_ring();
        outline.color = if show {
            color
        } else {
            bevy::color::Color::NONE
        };
        if show && visible.is_none() {
            commands.entity(entity).insert(FocusRingVisible);
        } else if !show && visible.is_some() {
            commands.entity(entity).remove::<FocusRingVisible>();
        }
    }
}

fn reveal_keyboard_focus(
    focus: Res<InputFocus>,
    modality: Res<InputModalityState>,
    mut commands: Commands,
) {
    if modality.shows_focus_ring()
        && let Some(entity) = focus.get()
    {
        commands.trigger(ScrollIntoView { entity });
    }
}

/// Capture uses normal Tab messages and waits for the actual painted control.
pub(crate) fn capture_ready(world: &mut World, switch: bool) -> bool {
    use bevy::ui::{CalculatedClip, ComputedNode, UiGlobalTransform};

    let focused = world.resource::<InputFocus>().get();
    if let Some(entity) = focused
        && (!switch || world.get::<Checkbox>(entity).is_some())
        && world.get::<FocusRing>(entity).is_some()
    {
        let Some(node) = world.get::<ComputedNode>(entity) else {
            return false;
        };
        let Some(transform) = world.get::<UiGlobalTransform>(entity) else {
            return false;
        };
        let half = node.outlined_node_size() * 0.5;
        let visible = world.get::<CalculatedClip>(entity).is_none_or(|clip| {
            clip.contains_point(transform.translation - half)
                && clip.contains_point(transform.translation + half)
        });
        return world.get::<FocusRingVisible>(entity).is_some()
            && node.size().x > 0.0
            && node.size().y > 0.0
            && visible;
    }
    let Some(window) = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(world)
        .next()
    else {
        return false;
    };
    world.write_message(KeyboardInput {
        key_code: KeyCode::Tab,
        logical_key: Key::Tab,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    false
}

/// System observing keyboard activity: any keyboard press transitions the modality to [`InputModality::Keyboard`].
pub(crate) fn focus_visible_keyboard_system(
    mut modality: ResMut<InputModalityState>,
    mut key_events: MessageReader<KeyboardInput>,
) {
    for event in key_events.read() {
        if event.state == ButtonState::Pressed {
            modality.transition(InputModality::Keyboard);
            break;
        }
    }
}

/// System observing pointer activity: pointer presses transition the modality to [`InputModality::Pointer`].
pub(crate) fn focus_visible_pointer_system(
    mut modality: ResMut<InputModalityState>,
    query: Query<&PickingInteraction, Changed<PickingInteraction>>,
) {
    for interaction in &query {
        if *interaction == PickingInteraction::Pressed {
            modality.transition(InputModality::Pointer);
            break;
        }
    }
}

/// Register focus-visible modality resources, observer, and input monitoring systems.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<InputModalityState>();
    app.init_resource::<InputFocus>();
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    app.add_systems(
        PostUpdate,
        (decorate_groups, decorate_controls, paint_focus_ring).chain(),
    );
    app.add_systems(
        PostUpdate,
        reveal_keyboard_focus.after(bevy::ui::UiSystems::PostLayout),
    );
    app.add_systems(
        Update,
        (focus_visible_keyboard_system, focus_visible_pointer_system),
    );
}

#[cfg(test)]
#[path = "../tests/headless/focus_visible.rs"]
mod tests;

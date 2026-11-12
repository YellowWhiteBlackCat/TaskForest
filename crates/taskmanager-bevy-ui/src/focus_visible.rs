//! Modality-aware high-contrast keyboard focus indication (CORE-08 / FocusVisible).
//!
//! Delivers true parity with GPUI and Iced: interactive controls and navigation items only paint a
//! high-contrast focus ring when navigating via keyboard. Pointer clicks switch the modality to
//! [`InputModality::Pointer`], suppressing the focus ring immediately to avoid distracting outlines
//! on mouse activation.
//!
//! # Architecture
//!
//! Strictly governed by an explicit discrete state machine ([`InputModalityState`]) with discrete three-state
//! transitions ([`InputModality`]: `Programmatic`, `Keyboard`, `Pointer`). Zero scattered booleans.
//! Actual widgets opt into toolkit Tab navigation; the single [`InputFocus`] resource
//! selects the one control whose outline is painted. No second focus marker exists.
//!
//! The modality vocabulary and the focus-visible rule belong to the shared contract
//! (`taskmanager_ui_contract::InputModality`); this module owns only the Bevy ECS adapter and the
//! observation systems.

use bevy::app::{App, PostUpdate, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::{Entities, Entity};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::message::MessageReader;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::{Changed, Has, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::SystemParam;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::keyboard::{Key, KeyCode};
use bevy::input_focus::InputFocusSystems;
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin};
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::picking::hover::PickingInteraction;
use bevy::ui::{CalculatedClip, ComputedNode, UiGlobalTransform};
use bevy::ui::{Node, Outline, Val};
use bevy::ui_widgets::ScrollIntoView;
use bevy::ui_widgets::{Button, Checkbox, RadioButton, Slider};
use bevy::window::PrimaryWindow;

use crate::confirmation::ConfirmationOverlay;
use crate::pages::processes::columns_modal::ProcessColumnsPopover;
use crate::pages::processes::properties_modal::ProcessPropertiesOverlay;
use crate::pages::settings::SettingsChoice;
use crate::scene_attach::insert_into;
use crate::spatial_focus::FocusTarget;
use crate::widgets::scene_paint::ScenePaint;
use crate::window::{AppShellRoot, DemoMode, WindowPalette};
use taskmanager_ui_contract::InputModality;

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
        self.modality.transition(next)
    }
}

/// A real focusable widget, decorated without changing its layout footprint.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRing;

/// Marker for the actual control whose keyboard outline is currently painted.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRingVisible;

/// A stable identity for a focusable control.
///
/// The toolkit focus points at an *entity*, but a page rebuild spawns a new
/// entity for the same control — so a focus that lived on the entity would
/// be lost at the first rebuild, while GPUI's focus handle survives it. The
/// identity lets the focus follow the control instead of the entity.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusIdentity(pub(crate) String);

/// The control that owned the keyboard focus, remembered across a rebuild.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct FocusedControl {
    entity: Option<Entity>,
    identity: Option<String>,
}

type UndecoratedControls<'w, 's> = Query<
    'w,
    's,
    (Entity, Has<Checkbox>, Has<RadioButton>),
    (
        With<Node>,
        Without<FocusRing>,
        Or<(
            With<Button>,
            With<Checkbox>,
            With<RadioButton>,
            With<Slider>,
        )>,
    ),
>;

fn decorate_controls(mut commands: Commands, controls: UndecoratedControls) {
    for (entity, checkbox, radio) in &controls {
        // Text switches have no opaque fill, so an inset ring remains visible
        // and stays inside the scroll viewport. Filled buttons need an outset.
        let offset = if checkbox || radio { -2.0 } else { 0.0 };
        insert_into(
            &mut commands,
            entity,
            (
                FocusRing,
                FocusTarget,
                TabIndex(0),
                // The inner ring's width comes from the shared focus-ring
                // seam (the same 2px the halo layer is measured against) — a
                // literal here would be a second width authority.
                Outline::new(
                    Val::Px(crate::widgets::focus_ring::focus_ring_inner_width()),
                    Val::Px(offset),
                    bevy::color::Color::NONE,
                ),
            ),
        );
    }
}

type UndecoratedModals<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Without<TabGroup>,
        Or<(
            With<ConfirmationOverlay>,
            With<ProcessPropertiesOverlay>,
            With<ProcessColumnsPopover>,
        )>,
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
        insert_into(&mut commands, entity, TabGroup::new(0));
    }
    for entity in &modals {
        insert_into(&mut commands, entity, TabGroup::modal());
        if let Some(first) = children
            .iter_descendants(entity)
            .find(|child| controls.contains(*child))
        {
            insert_into(&mut commands, first, AutoFocus);
        }
    }
}

/// Follow the keyboard focus across a page rebuild.
///
/// A rebuild replaces the focused control's *entity*, not the control: the
/// focus is restored to the control that carries the remembered identity, so
/// a keyboard-driven change (a slider step) does not strand the keyboard.
/// A focus the user moved elsewhere is never overridden — only a focus
/// whose entity no longer exists is restored.
fn follow_focus_across_rebuild(
    mut focus: ResMut<InputFocus>,
    mut remembered: ResMut<FocusedControl>,
    controls: Query<(Entity, &FocusIdentity)>,
    entities: &Entities,
) {
    if let Some(entity) = focus.get()
        && entities.contains(entity)
    {
        remembered.entity = Some(entity);
        remembered.identity = controls.get(entity).ok().map(|(_, id)| id.0.clone());
        return;
    }
    let Some(previous) = remembered.entity else {
        return;
    };
    if entities.contains(previous) {
        // The remembered control is still mounted: the focus moved on its
        // own, so nothing is restored.
        return;
    }
    let Some(identity) = remembered.identity.clone() else {
        return;
    };
    let Some((entity, _)) = controls.iter().find(|(_, id)| id.0 == identity) else {
        return;
    };
    focus.set(entity, bevy::input_focus::FocusCause::Navigated);
    remembered.entity = Some(entity);
}

/// Demo-only capture arm for the `settings-switch-focus` scenario: name the
/// High-contrast switch directly, because the live window's tab order is not
/// guaranteed to reach a page-level switch before the capture deadline (the
/// GPUI and Iced capture paths likewise focus their switch programmatically).
fn arm_settings_switch_focus(
    demo: Option<Res<DemoMode>>,
    mut modality: ResMut<InputModalityState>,
    mut focus: ResMut<InputFocus>,
    targets: Query<(Entity, &SettingsChoice), With<FocusRing>>,
) {
    if demo.is_none() || crate::capture::capture_scenario_target() != Some("settings-switch-focus")
    {
        return;
    }
    let Some((entity, _)) = targets.iter().find(|(_, choice)| {
        matches!(
            choice.0,
            crate::pages::settings::SettingsField::HighContrast(_)
        )
    }) else {
        return;
    };
    if focus.get() == Some(entity) && modality.shows_focus_ring() {
        return;
    }
    modality.transition(InputModality::Keyboard);
    focus.set(entity, bevy::input_focus::FocusCause::Navigated);
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
            insert_into(&mut commands, entity, FocusRingVisible);
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

type FocusControlQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static ComputedNode,
        &'static UiGlobalTransform,
        Option<&'static CalculatedClip>,
        Has<Checkbox>,
        Has<FocusRing>,
        Has<FocusRingVisible>,
    ),
>;

/// Capture uses normal Tab messages and waits for the actual painted control.
#[derive(SystemParam)]
pub(crate) struct FocusCapture<'w, 's> {
    focus: Res<'w, InputFocus>,
    controls: FocusControlQuery<'w, 's>,
    windows: Query<'w, 's, Entity, With<PrimaryWindow>>,
    keyboard: MessageWriter<'w, KeyboardInput>,
}
pub(crate) fn capture_ready(access: &mut FocusCapture, switch: bool) -> bool {
    if let Some(entity) = access.focus.get()
        && let Ok((node, transform, clip, checkbox, ring, ring_visible)) =
            access.controls.get(entity)
        && (!switch || checkbox)
        && ring
    {
        let half = node.outlined_node_size() * 0.5;
        let visible = clip.is_none_or(|clip| {
            clip.contains_point(transform.translation - half)
                && clip.contains_point(transform.translation + half)
        });
        return ring_visible && node.size().x > 0.0 && node.size().y > 0.0 && visible;
    }
    if crate::capture::capture_scenario_target() == Some("settings-switch-focus") {
        return false;
    }
    let Some(window) = access.windows.iter().next() else {
        return false;
    };
    access.keyboard.write(KeyboardInput {
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
    app.init_resource::<FocusedControl>();
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    // The focus follows the control across a rebuild, so the restore lands
    // before the toolkit routes this frame's keys to the focused entity.
    app.add_systems(
        bevy::app::PreUpdate,
        follow_focus_across_rebuild.before(InputFocusSystems::Dispatch),
    );
    app.add_systems(
        PostUpdate,
        arm_settings_switch_focus.chain().after(ScenePaint),
    );
    app.add_systems(
        PostUpdate,
        (decorate_groups, decorate_controls, paint_focus_ring)
            .chain()
            .after(arm_settings_switch_focus),
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

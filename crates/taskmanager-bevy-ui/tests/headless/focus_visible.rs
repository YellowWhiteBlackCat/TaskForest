//! test-intent: behavior
//!
//! Behavior tests for Bevy UI modality-aware focus indication (FocusVisible):
//! discrete state machine transitions, keyboard vs pointer modality synthesis,
//! and automatic focus ring mounting/despawning.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::With;
use bevy::ecs::system::RunSystemOnce;
use bevy::input::InputPlugin;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey};
use bevy::picking::hover::PickingInteraction;
use bevy::ui::BorderColor;
use taskmanager_theme::Theme;

use super::{
    FocusRing, FocusRingVisible, FocusedControl, InputModality, InputModalityChanged,
    InputModalityState,
};
use crate::palette::ui_palette;
use crate::window::WindowPalette;

#[test]
fn modality_state_machine_transitions_are_discrete_and_explicit() {
    let mut state = InputModalityState::default();
    assert_eq!(state.modality(), InputModality::Programmatic);
    assert!(!state.shows_focus_ring(), "Programmatic modality must not show focus ring");

    // Transition to Keyboard
    let changed = state.transition(InputModality::Keyboard);
    assert!(changed);
    assert_eq!(state.modality(), InputModality::Keyboard);
    assert!(state.shows_focus_ring(), "Keyboard modality must show focus ring");

    // Transition to Keyboard again is a no-op
    let changed = state.transition(InputModality::Keyboard);
    assert!(!changed);

    // Transition to Pointer
    let changed = state.transition(InputModality::Pointer);
    assert!(changed);
    assert_eq!(state.modality(), InputModality::Pointer);
    assert!(!state.shows_focus_ring(), "Pointer modality must suppress focus ring");

    // Transition to Programmatic
    let changed = state.transition(InputModality::Programmatic);
    assert!(changed);
    assert_eq!(state.modality(), InputModality::Programmatic);
    assert!(!state.shows_focus_ring(), "Programmatic modality must not show focus ring");
}

#[test]
fn input_events_drive_modality_transitions_and_trigger_observers() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin));
    super::register(&mut app);

    assert_eq!(
        app.world().resource::<InputModalityState>().modality(),
        InputModality::Programmatic
    );

    // 1. Keyboard event switches to Keyboard modality
    let key_event = KeyboardInput {
        key_code: KeyCode::Tab,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    };
    app.world_mut()
        .run_system_once(move |mut writer: MessageWriter<KeyboardInput>| {
            writer.write(key_event.clone());
        })
        .expect("write key event");
    app.update();

    assert_eq!(
        app.world().resource::<InputModalityState>().modality(),
        InputModality::Keyboard
    );
    assert!(app.world().resource::<InputModalityState>().shows_focus_ring());

    // 2. Pointer press switches to Pointer modality
    let pointer_entity = app
        .world_mut()
        .spawn((PickingInteraction::Pressed,))
        .id();
    app.update();

    assert_eq!(
        app.world().resource::<InputModalityState>().modality(),
        InputModality::Pointer
    );
    assert!(!app.world().resource::<InputModalityState>().shows_focus_ring());

    let _ = app.world_mut().despawn(pointer_entity);
}

#[test]
fn focus_ring_visible_mounts_on_keyboard_and_despawns_on_pointer() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin));
    let palette = ui_palette(&Theme::dark());
    app.insert_resource(WindowPalette { inner: palette.clone() });
    super::register(&mut app);

    let focused = app
        .world_mut()
        .spawn((
            FocusRing,
            FocusedControl,
            BorderColor::all(palette.border_color),
        ))
        .id();

    let unfocused = app
        .world_mut()
        .spawn((
            FocusRing,
            BorderColor::all(palette.border_color),
        ))
        .id();

    app.update();

    let world = app.world_mut();
    assert_eq!(
        world.query_filtered::<Entity, With<FocusRingVisible>>().iter(world).count(),
        0,
        "initially no entity has FocusRingVisible under Programmatic modality"
    );

    // 1. Keyboard navigation establishes focus ring
    let prev = app.world_mut().resource_mut::<InputModalityState>().modality();
    app.world_mut().resource_mut::<InputModalityState>().transition(InputModality::Keyboard);
    app.world_mut().commands().trigger(InputModalityChanged {
        previous: prev,
        current: InputModality::Keyboard,
    });
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world.query_filtered::<Entity, With<FocusRingVisible>>().iter(world).count(),
        1,
        "focused control receives FocusRingVisible"
    );
    assert!(
        app.world().get::<FocusRingVisible>(focused).is_some(),
        "focused entity mounts FocusRingVisible"
    );
    assert!(
        app.world().get::<FocusRingVisible>(unfocused).is_none(),
        "unfocused entity must not mount FocusRingVisible"
    );
    assert_eq!(
        app.world().get::<BorderColor>(focused).unwrap().top,
        palette.focus_ring,
        "border color changes to theme focus_ring"
    );

    // 2. Pointer interaction dismisses focus ring
    let prev = app.world_mut().resource_mut::<InputModalityState>().modality();
    app.world_mut().resource_mut::<InputModalityState>().transition(InputModality::Pointer);
    app.world_mut().commands().trigger(InputModalityChanged {
        previous: prev,
        current: InputModality::Pointer,
    });
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world.query_filtered::<Entity, With<FocusRingVisible>>().iter(world).count(),
        0,
        "pointer interaction suppresses all FocusRingVisible components"
    );
    assert_eq!(
        app.world().get::<BorderColor>(focused).unwrap().top,
        palette.border_color,
        "border color restores to default border_color"
    );
}

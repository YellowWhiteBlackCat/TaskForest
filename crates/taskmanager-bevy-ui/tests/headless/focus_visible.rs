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
use bevy::input::ButtonState;
use bevy::input::InputPlugin;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::hover::PickingInteraction;
use bevy::ui::{Node, Outline};
use bevy::ui_widgets::Button;
use taskmanager_theme::Theme;

use super::{FocusRingVisible, InputModality, InputModalityState};
use crate::palette::ui_palette;
use crate::window::WindowPalette;

#[test]
fn modality_state_machine_transitions_are_discrete_and_explicit() {
    let mut state = InputModalityState::default();
    assert_eq!(state.modality(), InputModality::Programmatic);
    assert!(
        !state.shows_focus_ring(),
        "Programmatic modality must not show focus ring"
    );

    // Transition to Keyboard
    let changed = state.transition(InputModality::Keyboard);
    assert!(changed);
    assert_eq!(state.modality(), InputModality::Keyboard);
    assert!(
        state.shows_focus_ring(),
        "Keyboard modality must show focus ring"
    );

    // Transition to Keyboard again is a no-op
    let changed = state.transition(InputModality::Keyboard);
    assert!(!changed);

    // Transition to Pointer
    let changed = state.transition(InputModality::Pointer);
    assert!(changed);
    assert_eq!(state.modality(), InputModality::Pointer);
    assert!(
        !state.shows_focus_ring(),
        "Pointer modality must suppress focus ring"
    );

    // Transition to Programmatic
    let changed = state.transition(InputModality::Programmatic);
    assert!(changed);
    assert_eq!(state.modality(), InputModality::Programmatic);
    assert!(
        !state.shows_focus_ring(),
        "Programmatic modality must not show focus ring"
    );
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
    assert!(
        app.world()
            .resource::<InputModalityState>()
            .shows_focus_ring()
    );

    // 2. Pointer press switches to Pointer modality
    let pointer_entity = app.world_mut().spawn((PickingInteraction::Pressed,)).id();
    app.update();

    assert_eq!(
        app.world().resource::<InputModalityState>().modality(),
        InputModality::Pointer
    );
    assert!(
        !app.world()
            .resource::<InputModalityState>()
            .shows_focus_ring()
    );

    let _ = app.world_mut().despawn(pointer_entity);
}

#[test]
fn focus_ring_visible_mounts_on_keyboard_and_despawns_on_pointer() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin));
    let palette = ui_palette(&Theme::dark());
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    super::register(&mut app);

    let focused = app.world_mut().spawn((Node::default(), Button)).id();

    let unfocused = app.world_mut().spawn((Node::default(), Button)).id();

    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(focused, FocusCause::Navigated);
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<FocusRingVisible>>()
            .iter(world)
            .count(),
        0,
        "initially no entity has FocusRingVisible under Programmatic modality"
    );

    // 1. Keyboard navigation establishes focus ring
    app.world_mut()
        .resource_mut::<InputModalityState>()
        .transition(InputModality::Keyboard);
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<FocusRingVisible>>()
            .iter(world)
            .count(),
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
        app.world().get::<Outline>(focused).unwrap().color,
        palette.focus_ring,
        "border color changes to theme focus_ring"
    );

    // 2. Pointer interaction dismisses focus ring
    app.world_mut()
        .resource_mut::<InputModalityState>()
        .transition(InputModality::Pointer);
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<FocusRingVisible>>()
            .iter(world)
            .count(),
        0,
        "pointer interaction suppresses all FocusRingVisible components"
    );
    assert_eq!(
        app.world().get::<Outline>(focused).unwrap().color,
        bevy::color::Color::NONE,
        "border color restores to default border_color"
    );
}

#[test]
fn normal_tab_navigation_moves_the_ring_between_real_product_controls() {
    use bevy::camera::visibility::InheritedVisibility;
    use bevy::input_focus::InputDispatchPlugin;
    use bevy::input_focus::tab_navigation::TabIndex;
    use bevy::window::{PrimaryWindow, Window};

    let mut app = crate::window::tests::scripted_frontend_app();
    app.add_plugins(InputDispatchPlugin);
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    app.update();
    app.update();
    // The headless composition has no visibility propagation plugin.
    for mut visible in app
        .world_mut()
        .query::<&mut InheritedVisibility>()
        .iter_mut(app.world_mut())
    {
        *visible = InheritedVisibility::VISIBLE;
    }
    let tab = |app: &mut App| {
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Tab,
            logical_key: Key::Tab,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        app.update();
    };
    tab(&mut app);
    let first = app
        .world()
        .resource::<InputFocus>()
        .get()
        .expect("first control");
    assert!(app.world().get::<Button>(first).is_some());
    assert!(app.world().get::<TabIndex>(first).is_some());
    assert!(app.world().get::<FocusRingVisible>(first).is_some());
    let original_node = app.world().get::<Node>(first).expect("real node").clone();
    tab(&mut app);
    let second = app
        .world()
        .resource::<InputFocus>()
        .get()
        .expect("next control");
    assert_ne!(first, second);
    assert!(app.world().get::<FocusRingVisible>(first).is_none());
    assert!(app.world().get::<FocusRingVisible>(second).is_some());
    assert_eq!(app.world().get::<Node>(first), Some(&original_node));

    // The normal frozen gate becomes the keyboard owner when it mounts.
    use crate::app::FrontendTrack;
    use crate::confirmation::{ConfirmationChanged, ConfirmationOverlay, PendingConfirmationView};
    use crate::demo_fixture::{demo_shell, seed_capture_confirmation_scenario};
    use bevy::ecs::hierarchy::ChildOf;
    let view = {
        let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
        track.shell = demo_shell();
        seed_capture_confirmation_scenario(&mut track.shell, "process-tree-confirm");
        PendingConfirmationView::from_pending(
            track.shell.pending_confirmation().expect("frozen gate"),
        )
        .expect("renderable gate")
    };
    app.world_mut().trigger(ConfirmationChanged(Some(view)));
    app.update();
    for mut visible in app
        .world_mut()
        .query::<&mut InheritedVisibility>()
        .iter_mut(app.world_mut())
    {
        *visible = InheritedVisibility::VISIBLE;
    }
    for _ in 0..6 {
        let focused = app
            .world()
            .resource::<InputFocus>()
            .get()
            .expect("modal control owns focus");
        let world = app.world();
        let mut ancestor = focused;
        let mut in_modal = false;
        while let Some(parent) = world.get::<ChildOf>(ancestor) {
            ancestor = parent.parent();
            if world.get::<ConfirmationOverlay>(ancestor).is_some() {
                in_modal = true;
                break;
            }
        }
        assert!(in_modal, "Tab must remain inside the armed gate");
        tab(&mut app);
    }
}

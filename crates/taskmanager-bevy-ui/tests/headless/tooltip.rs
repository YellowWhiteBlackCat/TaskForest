//! test-intent: behavior
//!
//! Behavior tests for Bevy UI hover tooltip state transitions and overlay mounting.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::With;
use bevy::ecs::system::RunSystemOnce;
use bevy::input::ButtonState;
use bevy::input::InputPlugin;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::InputFocusPlugin;
use bevy::picking::Pickable;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::ScenePlugin;
use bevy::text::Font;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::{TooltipOverlay, TooltipSession, TooltipState, TooltipText};
use crate::palette::ui_palette;
use crate::window::FrontendWindowPlugin;

#[test]
fn tooltip_state_transitions_are_explicit_and_discrete() {
    let mut state = TooltipState::default();
    assert!(!state.is_visible());
    assert_eq!(state.session, None);

    let target = Entity::from_raw_u32(42).unwrap();
    state.show(target, "CPU usage breakdown".into());
    assert!(state.is_visible());
    assert_eq!(
        state.session,
        Some(TooltipSession {
            target,
            text: "CPU usage breakdown".into(),
        })
    );

    state.hide();
    assert!(!state.is_visible());
    assert_eq!(state.session, None);
}

#[test]
fn tooltip_overlay_is_pointer_transparent_and_dismisses_until_pointer_exit() {
    set_language(Language::En);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        InputPlugin,
        InputFocusPlugin,
    ));
    app.init_resource::<Assets<Font>>();
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });

    // Initial frames to mount AppShellRoot
    app.update();
    app.update();

    // Spawn an interactive target entity with TooltipText and PickingInteraction
    let target = app
        .world_mut()
        .spawn((
            TooltipText("Processes column customizer".into()),
            PickingInteraction::None,
        ))
        .id();

    app.update();

    // Initially no tooltip overlay
    let world = app.world_mut();
    let overlays = world
        .query_filtered::<Entity, With<TooltipOverlay>>()
        .iter(world)
        .count();
    assert_eq!(overlays, 0, "no tooltip mounted before hover");

    // 1. Hover the entity
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .unwrap() = PickingInteraction::Hovered;
    app.update();

    // Verify TooltipOverlay is mounted
    let world = app.world_mut();
    let overlays = world
        .query_filtered::<Entity, With<TooltipOverlay>>()
        .iter(world)
        .count();
    assert_eq!(overlays, 1, "tooltip overlay mounts on hover");
    let overlay = {
        let world = app.world_mut();
        let mut query = world.query_filtered::<Entity, With<TooltipOverlay>>();
        query
            .iter(world)
            .next()
            .expect("one tooltip card is mounted")
    };
    assert_eq!(
        app.world().get::<Pickable>(overlay),
        Some(&Pickable::IGNORE),
        "tooltip card must not block its anchor or underlying controls"
    );

    // 2. Pressing the hovered trigger dismisses the card.
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .unwrap() = PickingInteraction::Pressed;
    app.update();
    assert_eq!(overlay_count(&mut app), 0, "press dismisses the tooltip");

    // 3. It does not reopen while the pointer remains over the trigger.
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .unwrap() = PickingInteraction::Hovered;
    app.update();
    assert_eq!(
        overlay_count(&mut app),
        0,
        "click dismissal lasts until the pointer exits"
    );

    // 4. Pointer exit clears the latch; a fresh hover mounts the card again.
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .unwrap() = PickingInteraction::None;
    app.update();
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .unwrap() = PickingInteraction::Hovered;
    app.update();
    assert_eq!(
        overlay_count(&mut app),
        1,
        "fresh hover remounts the tooltip"
    );

    // 5. Escape dismisses the visible card until the pointer leaves.
    let mut escape = Some(KeyboardInput {
        key_code: KeyCode::Escape,
        logical_key: Key::Escape,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    app.world_mut()
        .run_system_once(move |mut writer: MessageWriter<KeyboardInput>| {
            if let Some(event) = escape.take() {
                writer.write(event);
            }
        })
        .expect("Escape key injection");
    app.update();
    assert_eq!(overlay_count(&mut app), 0, "Escape dismisses the tooltip");
}

fn overlay_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<TooltipOverlay>>()
        .iter(world)
        .count()
}

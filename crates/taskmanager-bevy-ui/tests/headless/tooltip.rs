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
use bevy::input_focus::{FocusCause, InputFocus, InputFocusPlugin};
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

#[test]
fn tooltip_opens_on_keyboard_focus_and_dismisses_on_escape_press_or_focus_loss() {
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
    app.update();
    app.update();

    let target = app
        .world_mut()
        .spawn((
            TooltipText("Processes column customizer".into()),
            PickingInteraction::None,
        ))
        .id();
    let competing = app
        .world_mut()
        .spawn((
            TooltipText("Competing hover target".into()),
            PickingInteraction::None,
        ))
        .id();
    let elsewhere = app.world_mut().spawn_empty().id();
    app.update();
    assert_eq!(overlay_count(&mut app), 0, "focus has not landed yet");

    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(target, FocusCause::Navigated);
    app.update();
    assert_eq!(
        overlay_count(&mut app),
        1,
        "keyboard focus opens the card without a hover delay"
    );
    assert!(
        app.world().resource::<TooltipState>().is_ready(),
        "a focused card is ready immediately"
    );

    // A hover over another trigger cannot steal the focused card.
    *app.world_mut()
        .get_mut::<PickingInteraction>(competing)
        .expect("the competing target carries its hover state") = PickingInteraction::Hovered;
    app.update();
    assert_eq!(
        app.world().resource::<TooltipState>().session,
        Some(TooltipSession {
            target,
            text: "Processes column customizer".into(),
        }),
        "the focused trigger keeps the card"
    );

    press_escape(&mut app);
    assert_eq!(overlay_count(&mut app), 0, "Escape dismisses the card");
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .expect("the target carries its hover state") = PickingInteraction::Hovered;
    app.update();
    assert_eq!(
        overlay_count(&mut app),
        0,
        "hover does not reopen a card dismissed while its trigger is focused"
    );

    // Focus leaves and returns: the card opens again.
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(elsewhere, FocusCause::Navigated);
    app.update();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(target, FocusCause::Navigated);
    app.update();
    assert_eq!(
        overlay_count(&mut app),
        1,
        "returning focus reopens the card"
    );

    // Pointer press dismisses and keeps it closed until focus leaves.
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .expect("the target carries its hover state") = PickingInteraction::None;
    app.update();
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .expect("the target carries its hover state") = PickingInteraction::Pressed;
    app.update();
    assert_eq!(
        overlay_count(&mut app),
        0,
        "pointer press dismisses the focused card"
    );

    // Focus loss dismisses; hover still works once the trigger is unfocused.
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(elsewhere, FocusCause::Navigated);
    app.update();
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .expect("the target carries its hover state") = PickingInteraction::None;
    app.update();
    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .expect("the target carries its hover state") = PickingInteraction::Hovered;
    app.update();
    assert_eq!(
        overlay_count(&mut app),
        1,
        "hover still opens the card after focus left"
    );
    assert!(
        !app.world().resource::<TooltipState>().is_ready(),
        "a pointer hover keeps its delay"
    );
}

fn press_escape(app: &mut App) {
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
}

fn overlay_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<TooltipOverlay>>()
        .iter(world)
        .count()
}

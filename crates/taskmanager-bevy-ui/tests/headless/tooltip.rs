//! test-intent: behavior
//!
//! Behavior tests for Bevy UI hover tooltip state transitions and overlay mounting.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::input::InputPlugin;
use bevy::input_focus::InputFocusPlugin;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::ScenePlugin;
use bevy::text::Font;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_theme::Theme;

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
fn tooltip_overlay_mounts_on_hover_and_despawns_on_leave() {
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
        palette: ui_palette(&Theme::dark()),
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
    *app.world_mut().get_mut::<PickingInteraction>(target).unwrap() = PickingInteraction::Hovered;
    app.update();

    // Verify TooltipOverlay is mounted
    let world = app.world_mut();
    let overlays = world
        .query_filtered::<Entity, With<TooltipOverlay>>()
        .iter(world)
        .count();
    assert_eq!(overlays, 1, "tooltip overlay mounts on hover");

    // 2. Unhover the entity
    *app.world_mut().get_mut::<PickingInteraction>(target).unwrap() = PickingInteraction::None;
    app.update();

    // Verify TooltipOverlay is despawned
    let world = app.world_mut();
    let overlays = world
        .query_filtered::<Entity, With<TooltipOverlay>>()
        .iter(world)
        .count();
    assert_eq!(overlays, 0, "tooltip overlay despawns on leave");
}

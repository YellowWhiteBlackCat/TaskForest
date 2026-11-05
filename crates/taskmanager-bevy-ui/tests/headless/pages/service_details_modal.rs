//! test-intent: behavior
//!
//! Service details modal behavior in Bevy.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::AssetPlugin;
use bevy::ecs::query::With;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use taskforest_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_theme::GlassTier;
use taskmanager_theme::tokens::UiSize;

use super::details_modal::{ServiceDetailsModalState, service_details_modal_scene};
use crate::glass::OpticalGlassCard;
use taskmanager_theme::Theme;

fn dummy_service() -> ServiceItem {
    ServiceItem::from_inventory(
        "test.service",
        "test",
        ServiceStatus::Active,
        "A test service for details modal".to_owned(),
        "loaded",
        "active",
        "running",
    )
}

#[test]
fn service_details_modal_state_open_and_close() {
    let mut state = ServiceDetailsModalState::default();
    assert!(state.target.is_none());

    let service = dummy_service();
    state.target = Some(service.clone());
    assert_eq!(state.target.as_ref().unwrap().name, "test");

    state.target = None;
    assert!(state.target.is_none());
}

#[test]
fn service_details_modal_scene_renders_without_panic() {
    let service = dummy_service();
    let theme = Theme::default();
    let palette = crate::palette::ui_palette(&theme, UiSize::Standard);
    let scene = service_details_modal_scene(&service, &palette);

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    let world = app.world_mut();
    world
        .spawn_scene(scene)
        .expect("the details modal scene resolves without app resources");
    let card = world
        .query_filtered::<&OpticalGlassCard, With<bevy::ui::Node>>()
        .single(world)
        .expect("the details modal is an optical glass candidate");
    assert!(card.enabled && card.tier == GlassTier::Modal);
}

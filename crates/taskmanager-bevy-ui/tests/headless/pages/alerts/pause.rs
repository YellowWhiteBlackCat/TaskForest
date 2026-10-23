//! test-intent: behavior
//!
//! A paused telemetry refresh freezes the surface: the overlay with the
//! PAUSED watermark appears while the pause holds and is gone when the
//! telemetry resumes. Without it the numbers would silently stand still
//! and read as live.

use super::tests::{headless_shell_app, mount_alerts};
use super::*;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ui::widget::Text;
use taskmanager_application::AppAction;

fn overlays(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, With<pause::PausedOverlay>>()
        .iter(app.world())
        .collect()
}

fn toggle_pause(app: &mut App) {
    let _ = app
        .world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .apply_action(AppAction::TogglePause);
}

#[test]
fn a_paused_telemetry_freezes_the_surface_with_a_paused_watermark() {
    let mut app = headless_shell_app();
    mount_alerts(&mut app);
    assert!(
        overlays(&mut app).is_empty(),
        "a live surface carries no freeze overlay"
    );

    toggle_pause(&mut app);
    app.update();
    assert_eq!(
        overlays(&mut app).len(),
        1,
        "the paused surface carries the freeze overlay"
    );
    let texts: Vec<String> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(
        texts.iter().any(|text| text == "PAUSED"),
        "the freeze says so: {texts:?}"
    );

    toggle_pause(&mut app);
    app.update();
    assert!(
        overlays(&mut app).is_empty(),
        "the resumed surface carries no freeze overlay"
    );
}

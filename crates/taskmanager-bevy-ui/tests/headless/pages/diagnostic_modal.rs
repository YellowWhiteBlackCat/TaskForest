//! test-intent: behavior

use super::*;
use bevy::MinimalPlugins;
use bevy::asset::AssetPlugin;
use bevy::scene::ScenePlugin;
use taskmanager_core::core::diagnostics::DiagnosticBundleErrorKind;
use taskmanager_shell::fixture::{self, ProjectionSeedFact, seed_projection_fact};

fn diagnostic_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), ScenePlugin));
    app.insert_non_send(FrontendTrack {
        shell: fixture::demo_app(),
        initial_refresh_submitted: true,
        process_tree_expansion: Default::default(),
    });
    register(&mut app);
    app
}

#[test]
fn open_reviews_cached_facts_confirm_requires_a_writer_and_close_invalidates() {
    let mut app = diagnostic_app();
    app.world_mut().trigger(DiagnosticCommand::Open);
    app.update();
    let Some(DiagnosticBundleUiState::Preview(plan)) =
        &app.world().resource::<DiagnosticModalState>().0
    else {
        panic!("normal command must prepare a review")
    };
    assert_eq!(plan.preview().files.len(), 4);
    assert!(
        plan.sanitized_contents("snapshot.json")
            .expect("snapshot")
            .contains("zed")
    );
    assert!(plan.sanitized_contents("services.json").is_some());
    assert!(app.world().resource::<DiagnosticRuntime>().0.is_none());
    app.world_mut().trigger(DiagnosticCommand::Confirm);
    app.update();
    assert!(matches!(&app.world().resource::<DiagnosticModalState>().0,
        Some(DiagnosticBundleUiState::Failed(error)) if error.kind() == DiagnosticBundleErrorKind::Unavailable));
    app.world_mut().trigger(DiagnosticCommand::Close);
    app.update();
    assert!(app.world().resource::<DiagnosticModalState>().0.is_none());
    app.world_mut().trigger(DiagnosticCommand::Retry);
    app.update();
    assert!(
        app.world().resource::<DiagnosticModalState>().0.is_none(),
        "a stale retry cannot reopen a dismissed surface"
    );
}

#[test]
fn unobserved_inventory_is_a_failure_and_retry_uses_new_observations() {
    let mut app = diagnostic_app();
    seed_projection_fact(
        &mut app.world_mut().non_send_mut::<FrontendTrack>().shell,
        ProjectionSeedFact::Processes(None),
    );
    app.world_mut().trigger(DiagnosticCommand::Open);
    app.update();
    assert!(matches!(&app.world().resource::<DiagnosticModalState>().0,
        Some(DiagnosticBundleUiState::Failed(error)) if error.kind() == DiagnosticBundleErrorKind::Unavailable));
    let processes = fixture::demo_app()
        .projection()
        .processes
        .as_ref()
        .map(|rows| rows.as_ref().clone());
    seed_projection_fact(
        &mut app.world_mut().non_send_mut::<FrontendTrack>().shell,
        ProjectionSeedFact::Processes(processes),
    );
    app.world_mut().trigger(DiagnosticCommand::Retry);
    app.update();
    assert!(matches!(
        app.world().resource::<DiagnosticModalState>().0,
        Some(DiagnosticBundleUiState::Preview(_))
    ));
}

#[test]
fn diagnostic_layout_reserves_title_and_actions_around_one_scroll_body() {
    use crate::app::{Page, Route};
    use crate::window::tests::scripted_frontend_app;
    use bevy::asset::Assets;
    use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo};
    use bevy::ecs::query::Has;
    use bevy::image::TextureAtlasLayout;
    use bevy::math::UVec2;
    use bevy::picking::DefaultPickingPlugins;
    use bevy::text::TextPlugin;
    use bevy::transform::TransformPlugin;
    use bevy::ui::{UiGlobalTransform, UiPlugin};
    use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
    for (width, height) in [(480, 360), (720, 480), (1280, 720), (1600, 400), (720, 960)] {
        let mut app = scripted_frontend_app();
        app.world_mut().resource_mut::<Route>().page = Page::System;
        app.add_plugins((
            WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..Default::default()
            },
            DefaultPickingPlugins,
            TransformPlugin,
            TextPlugin,
            UiPlugin,
        ));
        app.init_resource::<Assets<TextureAtlasLayout>>();
        app.world_mut().spawn((
            Window {
                resolution: (width, height).into(),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        let size = UVec2::new(width, height);
        app.world_mut().spawn((
            Camera2d,
            Camera {
                computed: ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size: size,
                        scale_factor: 1.0,
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
        ));
        app.update();
        app.world_mut().non_send_mut::<FrontendTrack>().shell = fixture::demo_app();
        app.world_mut().trigger(DiagnosticCommand::Open);
        app.update();
        app.update();
        let world = app.world_mut();
        let mut nodes = world.query::<(
            &ComputedNode,
            &UiGlobalTransform,
            Has<DiagnosticHeading>,
            Has<DiagnosticBody>,
            Has<DiagnosticFooter>,
        )>();
        let mut found = 0;
        for (node, transform, heading, body, footer) in nodes.iter(world) {
            if !(heading || body || footer) {
                continue;
            }
            found += 1;
            let half = node.size() / 2.0;
            assert!(
                node.size().x > 0.0 && node.size().y > 0.0,
                "every diagnostic slot has a readable floor"
            );
            assert!(
                transform.translation.x - half.x >= 0.0 && transform.translation.y - half.y >= 0.0
            );
            assert!(
                transform.translation.x + half.x < width as f32
                    && transform.translation.y + half.y < height as f32,
                "diagnostic slot exceeds {width}x{height}: {:?} {:?}",
                transform.translation,
                node.size()
            );
        }
        assert_eq!(found, 3);
    }
}

#[test]
fn a_diagnostic_open_before_root_mount_is_rendered_after_the_root_arrives() {
    use taskmanager_theme::Theme;
    let mut app = diagnostic_app();
    app.world_mut().trigger(DiagnosticCommand::Open);
    app.update();
    assert!(
        app.world_mut()
            .query_filtered::<Entity, With<DiagnosticModalOverlay>>()
            .iter(app.world())
            .next()
            .is_none()
    );
    app.insert_resource(WindowPalette {
        inner: crate::palette::ui_palette(&Theme::dark()),
    });
    app.world_mut().spawn((Node::default(), AppShellRoot));
    app.update();
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<DiagnosticModalOverlay>>()
            .iter(app.world())
            .count(),
        1
    );
    let text: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.as_str())
        .collect();
    assert!(
        text.iter().any(|text| text.contains("snapshot.json")),
        "the actual reviewed facts must mount"
    );
    app.world_mut().trigger(DiagnosticCommand::Close);
    app.update();
    assert!(
        app.world_mut()
            .query_filtered::<Entity, With<DiagnosticModalOverlay>>()
            .iter(app.world())
            .next()
            .is_none()
    );
}

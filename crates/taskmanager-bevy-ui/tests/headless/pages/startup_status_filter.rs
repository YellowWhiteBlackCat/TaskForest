use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::image::TextureAtlasLayout;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::transform::TransformPlugin;
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin};
use bevy::ui_widgets::Activate;
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskforest_core::core::startup::StartupEntryId;
use taskmanager_shell::StartupStatusFilter;

use super::super::tests::{headless_startup_app, push_startup, route_to_startup, startup_entry};
use super::super::{
    StartupRowClicked, StartupRowMarker, StartupSelection, StartupSelectionMoved, StartupStatusLine,
};
use super::{StartupFilterChoice, StartupStatusFilterState};

fn install_layout_world(app: &mut bevy::app::App, frame: (u32, u32)) {
    app.add_plugins((
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..Default::default()
        },
        DefaultPickingPlugins,
        TransformPlugin,
        bevy::text::TextPlugin,
        UiPlugin,
    ));
    app.init_resource::<bevy::asset::Assets<TextureAtlasLayout>>();
    let size = UVec2::new(frame.0, frame.1);
    app.world_mut().spawn((
        Window {
            resolution: frame.into(),
            decorations: false,
            ..Default::default()
        },
        PrimaryWindow,
    ));
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
            viewport: Some(Viewport {
                physical_size: size,
                ..Default::default()
            }),
            ..Default::default()
        },
    ));
}

#[test]
fn status_filter_and_first_row_fit_standard_and_compact_frames() {
    for frame in [(1180, 780), (720, 480)] {
        let (mut app, events) = headless_startup_app();
        install_layout_world(&mut app, frame);
        route_to_startup(&mut app);
        push_startup(
            &events,
            vec![
                startup_entry("enabled-a", "Alpha", true),
                startup_entry("disabled", "Beta", false),
            ],
        );
        for _ in 0..5 {
            app.update();
        }

        let world = app.world_mut();
        let mut controls =
            world.query::<(&StartupFilterChoice, &ComputedNode, &UiGlobalTransform)>();
        let mut bounds: Vec<_> = controls
            .iter(world)
            .map(|(_, node, transform)| {
                let size = node.size() * node.inverse_scale_factor();
                let center = transform.translation;
                (
                    center.x - size.x / 2.0,
                    center.x + size.x / 2.0,
                    center.y,
                    size,
                )
            })
            .collect();
        assert_eq!(bounds.len(), 3, "all status choices fit in {frame:?}");
        bounds.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (left, right) in bounds.iter().zip(bounds.iter().skip(1)) {
            assert!(
                left.1 <= right.0 + 0.5,
                "status controls do not overlap in {frame:?}: {left:?} {right:?}"
            );
            assert!(
                (left.2 - right.2).abs() <= 0.5,
                "status controls stay on one row in {frame:?}"
            );
        }
        for (left, right, _, size) in &bounds {
            assert!(*left >= -0.5, "filter left edge stays inside {frame:?}");
            assert!(
                *right <= frame.0 as f32 + 0.5,
                "filter right edge stays inside {frame:?}"
            );
            assert!(size.y > 0.0, "filter control keeps its height in {frame:?}");
        }

        let filter_bottom = bounds[0].2 + bounds[0].3.y / 2.0;
        let mut rows = world.query::<(&StartupRowMarker, &ComputedNode, &UiGlobalTransform)>();
        let (_, row_node, row_transform) = rows
            .iter(world)
            .next()
            .expect("a filtered startup row mounts");
        let row_size = row_node.size() * row_node.inverse_scale_factor();
        let row_top = row_transform.translation.y - row_size.y / 2.0;
        assert!(
            row_top >= filter_bottom,
            "the first data row begins below the complete filter band in {frame:?}"
        );
    }
}

#[test]
fn status_filter_controls_project_visible_rows_and_keep_identity() {
    let (mut app, events) = headless_startup_app();
    route_to_startup(&mut app);
    push_startup(
        &events,
        vec![
            startup_entry("enabled-a", "Alpha", true),
            startup_entry("disabled", "Beta", false),
            startup_entry("enabled-b", "Gamma", true),
        ],
    );
    app.update();
    app.update();

    let choices: Vec<_> = app
        .world_mut()
        .query_filtered::<(Entity, &StartupFilterChoice), ()>()
        .iter(app.world())
        .map(|(entity, choice)| (entity, choice.0))
        .collect();
    assert_eq!(choices.len(), 3, "the filter exposes every shared choice");

    app.world_mut().trigger(StartupRowClicked(0));
    app.update();
    let selected_before_filter = app
        .world()
        .resource::<StartupSelection>()
        .target
        .clone()
        .expect("the first visible row is selected");

    let disabled = app
        .world_mut()
        .query_filtered::<(Entity, &StartupFilterChoice), ()>()
        .iter(app.world())
        .map(|(entity, choice)| (entity, choice.0))
        .find(|(_, choice)| *choice == StartupStatusFilter::Disabled)
        .map(|(entity, _)| entity)
        .expect("the Disabled filter is mounted");
    app.world_mut()
        .commands()
        .trigger(Activate { entity: disabled });
    app.update();
    app.update();

    assert_eq!(
        app.world().resource::<StartupStatusFilterState>().selected,
        StartupStatusFilter::Disabled
    );
    let visible: Vec<_> = app
        .world_mut()
        .query_filtered::<&StartupRowMarker, ()>()
        .iter(app.world())
        .map(|row| row.1.as_str().to_owned())
        .collect();
    assert_eq!(visible, vec!["disabled"]);
    assert_eq!(
        app.world().resource::<StartupSelection>().target,
        Some(selected_before_filter),
        "filtering keeps the provider identity selected while it is hidden"
    );
    let status = app
        .world_mut()
        .query_filtered::<&Text, With<StartupStatusLine>>()
        .iter(app.world())
        .next()
        .expect("the summary line is mounted");
    assert!(status.0.starts_with("1 "), "the count follows the filter");

    app.world_mut().trigger(StartupSelectionMoved(1));
    app.update();
    assert_eq!(
        app.world().resource::<StartupSelection>().target,
        Some(StartupEntryId::new("disabled")),
        "keyboard movement enters the filtered projection"
    );

    let enabled = app
        .world_mut()
        .query_filtered::<(Entity, &StartupFilterChoice), ()>()
        .iter(app.world())
        .map(|(entity, choice)| (entity, choice.0))
        .find(|(_, choice)| *choice == StartupStatusFilter::Enabled)
        .map(|(entity, _)| entity)
        .expect("the Enabled filter is mounted");
    app.world_mut()
        .commands()
        .trigger(Activate { entity: enabled });
    app.update();
    app.update();
    let visible: Vec<_> = app
        .world_mut()
        .query_filtered::<&StartupRowMarker, ()>()
        .iter(app.world())
        .map(|row| row.1.as_str().to_owned())
        .collect();
    assert_eq!(visible.len(), 2);
    assert!(visible.iter().all(|target| target.starts_with("enabled-")));
}

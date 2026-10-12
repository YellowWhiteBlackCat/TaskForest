use super::*;
use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::tooltip::TooltipText;
use taskmanager_shell::presentation::{bytes, control_error_detail};

#[test]
fn aggregate_metric_states_reach_the_production_row_scene_and_semantic_nodes() {
    use bevy::MinimalPlugins;
    use bevy::a11y::AccessibilityNode;
    use bevy::app::App;
    use bevy::asset::{AssetPlugin, Assets};
    use bevy::ecs::hierarchy::Children;
    use bevy::scene::{ScenePlugin, WorldSceneExt};
    use bevy::text::Font;
    use bevy::ui::widget::Text;
    use taskmanager_application::i18n::t;
    use taskmanager_test_support::pin_english;
    use taskmanager_theme::Theme;
    use taskmanager_theme::tokens::UiSize;

    pin_english();

    fn apply_metrics(
        process: &mut ProcessItem,
        cpu: ScalarObservation<f32>,
        memory: ScalarObservation<u64>,
    ) {
        let mut observations = *process.scalar_observations();
        observations.cpu_percentage = cpu;
        observations.memory_bytes = memory;
        process.apply_scalar_observations(observations);
    }

    let mut available_zero = application(100, None, "Zero");
    apply_metrics(
        &mut available_zero,
        ScalarObservation::available(0.0, SNAPSHOT_MS),
        ScalarObservation::default(),
    );

    let mut partial_current = tokened(200, "partial-current", None)
        .with_application_identity_observation(ProcessMetadataObservation::absent(SNAPSHOT_MS));
    apply_metrics(
        &mut partial_current,
        ScalarObservation::available(4.0, SNAPSHOT_MS),
        ScalarObservation::available(0, SNAPSHOT_MS),
    );
    let mut partial_permission = tokened(201, "partial-permission", None)
        .with_application_identity_observation(ProcessMetadataObservation::absent(SNAPSHOT_MS));
    apply_metrics(
        &mut partial_permission,
        ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ScalarObservation::available(0, SNAPSHOT_MS),
    );

    let mut stale = tokened(300, "stale", None);
    apply_metrics(
        &mut stale,
        ScalarObservation::stale(7.5, SNAPSHOT_MS, FailureKind::TimedOut),
        ScalarObservation::unavailable(FailureKind::MissingDependency),
    );
    let mut stale_unavailable = tokened(301, "stale-unavailable", None);
    apply_metrics(
        &mut stale_unavailable,
        ScalarObservation::unavailable(FailureKind::TimedOut),
        ScalarObservation::unavailable(FailureKind::MissingDependency),
    );

    // The shell projection supplies one row per process category. Those rows
    // exercise every aggregate state through the production scene adapter.
    let items = vec![
        available_zero,
        partial_current,
        partial_permission,
        stale,
        stale_unavailable,
    ];
    let rows = project_items(&items, &ProcessTreeExpansion::default(), SNAPSHOT_MS);
    assert_eq!(rows.len(), 3);

    let palette = crate::palette::ui_palette(&Theme::default(), UiSize::Standard);
    let mut scene_app = App::new();
    scene_app.add_plugins(MinimalPlugins);
    scene_app.add_plugins((AssetPlugin::default(), ScenePlugin));
    scene_app.init_resource::<Assets<Font>>();
    for row in &rows {
        scene_app
            .world_mut()
            .spawn_scene(row_scene(row, &palette))
            .expect("the production tree row scene resolves");
    }
    scene_app.update();

    let world = scene_app.world_mut();
    let mut cell_query = world.query::<(&ProcessTreeTextCell, &Children)>();
    let cells: Vec<(String, ProcessTreeCellKind, String)> = cell_query
        .iter(world)
        .map(|(cell, children)| {
            let text = children
                .first()
                .and_then(|entity| world.get::<Text>(*entity))
                .expect("each production cell has its text node");
            (cell.0.clone(), cell.1, text.0.clone())
        })
        .collect();
    assert_eq!(cells.len(), 9, "three bounded cells per aggregate row");
    assert_eq!(
        world
            .query::<(&SemanticAddress, &ProcessTreeRowMarker)>()
            .iter(world)
            .count(),
        3,
        "each state-bearing row retains its stable semantic identity and typed row marker"
    );

    let key_for = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .expect("fixture row exists")
            .semantic_key
            .clone()
    };
    let application_key = key_for("Applications");
    let background_key = key_for("Background");
    let uncategorized_key = key_for("Uncategorized");
    let background_address = stable_semantic_address("process-tree", &background_key);
    let uncategorized_address = stable_semantic_address("process-tree", &uncategorized_key);
    let cell_text = |row_key: &str, kind| {
        cells
            .iter()
            .find(|(key, cell_kind, _)| key == row_key && *cell_kind == kind)
            .map(|(_, _, text)| text.clone())
            .expect("production cell exists")
    };

    assert_eq!(
        cell_text(&application_key, ProcessTreeCellKind::Name),
        "Applications · 1".to_owned()
    );
    assert_eq!(
        cell_text(&application_key, ProcessTreeCellKind::Cpu),
        format!("{}: 0.0%", t("common.cpu")),
        "a real zero stays visible"
    );
    assert_eq!(
        cell_text(&application_key, ProcessTreeCellKind::Memory),
        format!("{}: {}", t("common.memory"), t("proc_insights.unknown")),
        "unknown has no number or fabricated zero"
    );

    assert_eq!(
        cell_text(&background_key, ProcessTreeCellKind::Cpu),
        format!("{}: 4%P1/2", t("common.cpu")),
        "partial current data keeps visible coverage and status compact"
    );
    assert_eq!(
        cell_text(&background_key, ProcessTreeCellKind::Memory),
        format!("{}: {}", t("common.memory"), bytes(0)),
        "available zero bytes remains a value"
    );
    assert_eq!(
        cell_text(&uncategorized_key, ProcessTreeCellKind::Cpu),
        format!("{}: 7.5%S1/2", t("common.cpu")),
        "stale displays its last-known value, coverage, and stale marker"
    );
    assert_eq!(
        cell_text(&uncategorized_key, ProcessTreeCellKind::Memory),
        format!("{}: {}", t("common.memory"), t("proc_insights.unavailable")),
        "unavailable has no number"
    );

    let mut semantic_query = world.query::<(&SemanticAddress, &TooltipText, &AccessibilityNode)>();
    let semantic_rows: Vec<(String, String, String)> = semantic_query
        .iter(world)
        .map(|(address, tooltip, node)| {
            (
                address.0.as_str().to_owned(),
                tooltip.0.clone(),
                node.label().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    assert_eq!(semantic_rows.len(), 3);

    let background_summary = semantic_rows
        .iter()
        .find(|(key, _, _)| key == background_address.as_str())
        .expect("background row has tooltip and accessible summary");
    assert_eq!(
        background_summary.1,
        format!(
            "Background · 2 · {} 4.0% · 1/2 · {} · {} · {} {}",
            t("common.cpu"),
            t("disk.usage_partial"),
            control_error_detail(FailureKind::PermissionDenied),
            t("common.memory"),
            bytes(0),
        ),
        "the tooltip keeps the complete partial state and permission reason"
    );
    assert_eq!(
        background_summary.2, background_summary.1,
        "the accessible row label preserves the same full state and reason"
    );

    let uncategorized_summary = semantic_rows
        .iter()
        .find(|(key, _, _)| key == uncategorized_address.as_str())
        .expect("uncategorized row has tooltip and accessible summary");
    assert_eq!(
        uncategorized_summary.1,
        format!(
            "Uncategorized · 2 · {} 7.5% · 1/2 · {} · {} · {} {} · {}",
            t("common.cpu"),
            t("device.stale"),
            control_error_detail(FailureKind::TimedOut),
            t("common.memory"),
            t("proc_insights.unavailable"),
            control_error_detail(FailureKind::MissingDependency),
        ),
        "the tooltip preserves stale last-known coverage and the unavailable reason"
    );
    assert_eq!(uncategorized_summary.2, uncategorized_summary.1);
}

#[test]
fn aggregate_cells_and_full_tooltip_fit_standard_and_compact_row_widths() {
    use crate::tooltip::{TooltipOverlay, tooltip_card_scene};
    use crate::window::tests::scripted_frontend_app;
    use bevy::asset::Assets;
    use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
    use bevy::ecs::entity::Entity;
    use bevy::ecs::hierarchy::ChildOf;
    use bevy::ecs::query::With;
    use bevy::image::TextureAtlasLayout;
    use bevy::math::UVec2;
    use bevy::picking::DefaultPickingPlugins;
    use bevy::scene::WorldSceneExt;
    use bevy::text::{LineBreak, TextLayout, TextPlugin};
    use bevy::transform::TransformPlugin;
    use bevy::ui::{ComputedNode, OverflowAxis, UiGlobalTransform, UiPlugin};
    use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_test_support::pin_english;

    pin_english();

    fn apply_metrics(
        process: &mut ProcessItem,
        cpu: ScalarObservation<f32>,
        memory: ScalarObservation<u64>,
    ) {
        let mut observations = *process.scalar_observations();
        observations.cpu_percentage = cpu;
        observations.memory_bytes = memory;
        process.apply_scalar_observations(observations);
    }

    for (width, height, row_width) in [(1_180, 780, 632.0), (720, 480, 600.0)] {
        // The modal is min(90% of the viewport, 680px) wide with 24px padding
        // on each side, leaving these exact row-content widths.
        let mut app = scripted_frontend_app();
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
        let size = UVec2::new(width, height);
        app.world_mut().spawn((
            Window {
                resolution: (width, height).into(),
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
        app.update();

        let long_name = "A Very Long Application Name That Must Stay Inside Its Row Cell";
        let mut current = application(700, None, long_name);
        let mut denied = application(701, Some(700), long_name);
        apply_metrics(
            &mut current,
            ScalarObservation::available(4.0, SNAPSHOT_MS),
            ScalarObservation::unavailable(FailureKind::MissingDependency),
        );
        apply_metrics(
            &mut denied,
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
            ScalarObservation::unavailable(FailureKind::MissingDependency),
        );
        let items = vec![current, denied];
        let mut expansion = ProcessTreeExpansion::default();
        expansion.toggle_category(ProcessCategory::Application);
        let rows = project_items(&items, &expansion, SNAPSHOT_MS);
        let row = rows
            .iter()
            .find(|row| row.label == long_name)
            .expect("expanded application aggregate row");
        let palette = app
            .world()
            .resource::<crate::window::WindowPalette>()
            .inner
            .clone();
        let scene = bsn! {
            Node {
                width: px(row_width),
                height: px(palette.control_height_px()),
                overflow: bevy::ui::Overflow::clip_x(),
            }
            Children [ @{ row_scene(row, &palette) } ]
        };
        app.world_mut()
            .spawn_scene(scene)
            .expect("the bounded production row scene resolves");
        app.update();
        app.update();

        let summary = {
            let world = app.world_mut();
            let mut rows = world.query_filtered::<&TooltipText, With<ProcessTreeRowMarker>>();
            rows.iter(world)
                .next()
                .expect("mounted row has a full tooltip")
                .0
                .clone()
        };
        assert!(summary.contains(long_name));
        assert!(summary.contains(&control_error_detail(FailureKind::PermissionDenied)));
        assert!(summary.contains(&control_error_detail(FailureKind::MissingDependency)));
        app.world_mut()
            .spawn_scene(tooltip_card_scene(&summary, &palette))
            .expect("the full tooltip scene resolves");
        app.update();

        let world = app.world_mut();
        let mut row_query = world.query_filtered::<
            (Entity, &bevy::ui::Node, &ComputedNode, &UiGlobalTransform),
            With<ProcessTreeRowMarker>,
        >();
        let (row_entity, row_style, row_node, row_transform) = row_query
            .single(world)
            .expect("one measured application row");
        assert_eq!(row_style.overflow.x, OverflowAxis::Clip);
        assert!(row_node.size().x <= row_width + 0.5);
        let row_left = row_transform.translation.x - row_node.size().x / 2.0;
        let row_right = row_transform.translation.x + row_node.size().x / 2.0;

        let mut cell_query = world.query::<(
            &ProcessTreeTextCell,
            &bevy::ui::Node,
            &ComputedNode,
            &UiGlobalTransform,
            &ChildOf,
            &Children,
        )>();
        let mut cells = 0;
        for (cell, node, computed, transform, parent, children) in cell_query.iter(world) {
            assert_eq!(parent.parent(), row_entity);
            assert_eq!(node.overflow.x, OverflowAxis::Clip);
            let left = transform.translation.x - computed.size().x / 2.0;
            let right = transform.translation.x + computed.size().x / 2.0;
            assert!(computed.size().x > 0.0, "every cell retains a width floor");
            assert!(
                left >= row_left - 0.5,
                "{cell:?} crosses the row's left edge"
            );
            assert!(
                right <= row_right + 0.5,
                "{cell:?} crosses the row's right edge"
            );
            let text_entity = children.first().expect("cell text child");
            let text_layout = world
                .get::<TextLayout>(*text_entity)
                .expect("cell text uses an explicit layout");
            assert_eq!(
                text_layout.linebreak,
                LineBreak::NoWrap,
                "bounded cells never wrap into a second row"
            );
            cells += 1;
        }
        assert_eq!(cells, 3, "name, CPU, and memory each have bounded slots");

        let mut tooltip_query = world
            .query_filtered::<(&ComputedNode, &UiGlobalTransform, &Children), With<TooltipOverlay>>(
            );
        let (tooltip_node, tooltip_transform, tooltip_children) = tooltip_query
            .single(world)
            .expect("full tooltip has measured geometry");
        let tooltip_right = tooltip_transform.translation.x + tooltip_node.size().x / 2.0;
        let tooltip_left = tooltip_transform.translation.x - tooltip_node.size().x / 2.0;
        assert!(tooltip_node.size().x <= width as f32 * 0.9 + 0.5);
        assert!(
            tooltip_right <= width as f32 + 0.5,
            "tooltip crosses viewport edge"
        );
        let tooltip_text = tooltip_children.first().expect("full tooltip text child");
        let tooltip_text_node = world
            .get::<ComputedNode>(*tooltip_text)
            .expect("tooltip text has measured bounds");
        let tooltip_text_transform = world
            .get::<UiGlobalTransform>(*tooltip_text)
            .expect("tooltip text has a viewport position");
        let tooltip_text_left =
            tooltip_text_transform.translation.x - tooltip_text_node.size().x / 2.0;
        let tooltip_text_right =
            tooltip_text_transform.translation.x + tooltip_text_node.size().x / 2.0;
        assert!(tooltip_text_left >= tooltip_left - 0.5);
        assert!(tooltip_text_right <= tooltip_right + 0.5);
        let tooltip_layout = world
            .get::<TextLayout>(*tooltip_text)
            .expect("tooltip uses a wrapping text layout");
        assert_eq!(tooltip_layout.linebreak, LineBreak::WordOrCharacter);
    }
}

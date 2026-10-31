//! test-intent: behavior
//!
//! Headless behaviour tests for the Bevy Containers page's DECLARED quota
//! (BEVY-152): the page compares a container against the ceilings the
//! container declared, never against a cap the page invented.
//!
//! The claims under test:
//! - the warning halo appears ONLY for a declared ceiling whose observed
//!   share the observation actually crossed;
//! - a container that declares nothing produces no over-limit state and no
//!   percentage;
//! - a declaration the platform could not read says "unknown" — never a
//!   "0%" ring;
//! - an overrun stays visible: the share is never clamped to 100%;
//! - the declared CPU budget is honoured, and a `cpu.max` that names no
//!   budget declares no ceiling at all;
//! - the quota column keeps its slot: the row's cells never leave the row's
//!   content box, at any viewport width.

use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::image::TextureAtlasLayout;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::{Font, RemSize, TextPlugin};
use bevy::transform::TransformPlugin;
use bevy::ui::prelude::{ComputedNode, UiGlobalTransform};
use bevy::ui::widget::Text;
use bevy::ui::{IsDefaultUiCamera, UiPlugin};
use bevy::ui_widgets::UiWidgetsPlugins;
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskforest_core::core::FailureKind;
use taskforest_core::core::device_state::DeviceState;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process_telemetry::{
    ContainerCpuQuota, ContainerQuota, ContainerRollup, ContainerSummary, IsolationKind,
};
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::quota::{
    ContainerQuotaBreach, ContainerQuotaCell, ContainerQuotaState, ContainerWarningHalo,
    container_quota_view, quota_readout, warning_halo_alpha,
};
use super::{ContainerRowMarker, scene};
use crate::app::PageContext;
use crate::palette::ui_palette;
use crate::window::WindowPalette;

const GIB: u64 = 1024 * 1024 * 1024;

/// One container summary with an explicit declared quota.
fn summary_with_quota(
    id: &str,
    name: &str,
    cpu_percent: Option<f32>,
    memory_bytes: Option<u64>,
    quota: ContainerQuota,
) -> ContainerSummary {
    ContainerSummary {
        id: id.to_owned(),
        name: name.to_owned(),
        runtime: Some(IsolationKind::Docker),
        cgroup_path: format!("/sys/fs/cgroup/{id}"),
        cpu_percentage: cpu_percent.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TimedOut),
            |value| ScalarObservation::available(value, 1),
        ),
        memory_bytes: memory_bytes.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TimedOut),
            |value| ScalarObservation::available(value, 1),
        ),
        member_pids: vec![42],
        quota,
    }
}

/// A quota that declares a soft memory ceiling of one gibibyte.
fn soft_ceiling(bytes: u64) -> ContainerQuota {
    ContainerQuota {
        memory_soft_limit_bytes: Some(bytes),
        ..ContainerQuota::not_declared()
    }
}

/// A quota that declares a CPU budget of whole CPUs.
fn cpu_budget(cpus: u64) -> ContainerQuota {
    ContainerQuota {
        cpu: Some(ContainerCpuQuota {
            quota_us: Some(cpus * 100_000),
            period_us: 100_000,
        }),
        ..ContainerQuota::not_declared()
    }
}

/// A quota whose CPU declaration names no budget at all ("max").
fn cpu_without_budget() -> ContainerQuota {
    ContainerQuota {
        cpu: Some(ContainerCpuQuota {
            quota_us: None,
            period_us: 100_000,
        }),
        ..ContainerQuota::not_declared()
    }
}

/// The shell with the given container rows.
fn shell_with(containers: Vec<ContainerSummary>) -> ShellApp {
    let mut shell = demo_app();
    fixture::edit_containers(&mut shell, |rollup| {
        *rollup = Some(ContainerRollup {
            state: DeviceState::healthy(100),
            containers,
        });
    });
    shell
}

/// Mount the containers page scene in a headless app so the rendered rows
/// can be inspected.
fn mount_containers_page(shell: &ShellApp) -> App {
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<bevy::asset::Assets<Font>>();
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell,
        palette: &palette,
        history: &history.0,
    };
    app.world_mut()
        .spawn_scene(scene(&context))
        .expect("the containers scene resolves without assets");
    app.update();
    app
}

/// The breach every mounted warning halo carries.
fn halos(app: &mut App) -> Vec<ContainerQuotaBreach> {
    app.world_mut()
        .query_filtered::<&ContainerWarningHalo, ()>()
        .iter(app.world())
        .filter_map(|halo| halo.0)
        .collect()
}

/// The quota state and the readout text of every mounted row, in row order.
fn quota_cells(app: &mut App) -> Vec<(String, ContainerQuotaState, String)> {
    let rows: Vec<(String, Entity)> = app
        .world_mut()
        .query_filtered::<(Entity, &ContainerRowMarker), ()>()
        .iter(app.world())
        .map(|(entity, marker)| (marker.0.clone(), entity))
        .collect();
    let world = app.world();
    let mut rows = rows;
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    rows.into_iter()
        .map(|(id, row)| {
            let cell = world
                .get::<Children>(row)
                .expect("the row carries its cells")
                .iter()
                .find(|entity| world.get::<ContainerQuotaCell>(**entity).is_some())
                .expect("the row carries a quota cell");
            let state = world
                .get::<ContainerQuotaCell>(*cell)
                .expect("the quota cell carries its state")
                .0;
            let readout = world
                .get::<Children>(*cell)
                .expect("the quota cell carries its readout")
                .iter()
                .find_map(|entity| world.get::<Text>(*entity).map(|text| text.0.clone()))
                .expect("the quota cell shows a readout");
            (id, state, readout)
        })
        .collect()
}

#[test]
fn the_declared_quota_state_separates_unknown_undeclared_and_declared() {
    let undeclared = container_quota_view(&summary_with_quota(
        "a",
        "plain",
        Some(5.0),
        Some(GIB),
        ContainerQuota::not_declared(),
    ));
    assert_eq!(
        undeclared.state,
        ContainerQuotaState::NotDeclared,
        "a container that declares no ceiling is not declared, never under a limit"
    );
    assert!(
        undeclared.breach().is_none(),
        "an undeclared ceiling never produces an over-limit state"
    );
    assert_eq!(
        undeclared.memory_share_percent, None,
        "an undeclared ceiling has no share to show"
    );

    let unknown = container_quota_view(&summary_with_quota(
        "b",
        "unreadable",
        Some(5.0),
        Some(GIB),
        ContainerQuota::unavailable(FailureKind::PermissionDenied),
    ));
    assert_eq!(
        unknown.state,
        ContainerQuotaState::Unknown,
        "an unreadable declaration is unknown, never a zero"
    );
    assert!(
        unknown.breach().is_none(),
        "an unknown declaration never claims a breach"
    );
    assert_eq!(
        unknown.memory_share_percent, None,
        "an unknown declaration has no share to show"
    );

    let declared = container_quota_view(&summary_with_quota(
        "c",
        "capped",
        Some(5.0),
        Some(GIB),
        soft_ceiling(2 * GIB),
    ));
    assert_eq!(
        declared.state,
        ContainerQuotaState::Declared,
        "a declared ceiling is read as declared"
    );
    assert_eq!(
        declared.memory_share_percent,
        Some(50.0),
        "the observed share is the observed one, not a page policy"
    );
    assert!(
        declared.breach().is_none(),
        "a container below its declared ceiling is not over the limit"
    );
}

#[test]
fn an_undeclared_ceiling_produces_no_over_limit_state() {
    let shell = shell_with(vec![summary_with_quota(
        "a",
        "plain",
        Some(5.0),
        Some(64 * GIB),
        ContainerQuota::not_declared(),
    )]);
    let mut app = mount_containers_page(&shell);

    assert!(
        halos(&mut app).is_empty(),
        "a container that declares nothing never wears the warning halo"
    );
    let cells = quota_cells(&mut app);
    assert_eq!(cells.len(), 1, "the row is mounted");
    assert_eq!(
        cells[0].1,
        ContainerQuotaState::NotDeclared,
        "the row states that nothing was declared"
    );
    assert_eq!(
        cells[0].2,
        t("proc_insights.not_declared"),
        "an undeclared quota reads as not declared, never as a percentage"
    );
    assert!(
        !cells[0].2.contains('%'),
        "an undeclared quota shows no percentage at all: {readout}",
        readout = cells[0].2
    );
}

#[test]
fn an_unreadable_declaration_says_unknown_instead_of_a_zero() {
    let shell = shell_with(vec![summary_with_quota(
        "a",
        "unreadable",
        Some(5.0),
        Some(GIB),
        ContainerQuota::unavailable(FailureKind::PermissionDenied),
    )]);
    let mut app = mount_containers_page(&shell);

    assert!(
        halos(&mut app).is_empty(),
        "an unreadable declaration never draws a ring"
    );
    let cells = quota_cells(&mut app);
    assert_eq!(cells.len(), 1, "the row is mounted");
    assert_eq!(
        cells[0].1,
        ContainerQuotaState::Unknown,
        "the row states that the quota is unknown"
    );
    assert_eq!(
        cells[0].2,
        t("proc_insights.unknown"),
        "the unknown quota is named, never rendered as a zero"
    );
    assert!(
        !cells[0].2.contains("0%"),
        "an unknown quota never becomes a zero percent ring: {readout}",
        readout = cells[0].2
    );
}

#[test]
fn the_halo_appears_only_for_a_declared_ceiling_the_observed_share_touches() {
    let shell = shell_with(vec![
        summary_with_quota(
            "declared-under",
            "under",
            Some(5.0),
            Some(GIB),
            soft_ceiling(2 * GIB),
        ),
        summary_with_quota(
            "declared-over",
            "over",
            Some(5.0),
            Some(3 * GIB),
            soft_ceiling(2 * GIB),
        ),
        summary_with_quota(
            "undeclared",
            "plain",
            Some(5.0),
            Some(64 * GIB),
            ContainerQuota::not_declared(),
        ),
    ]);
    let mut app = mount_containers_page(&shell);

    let breaches = halos(&mut app);
    assert_eq!(
        breaches.len(),
        1,
        "only the row that crossed a declared ceiling wears the halo"
    );
    assert_eq!(
        breaches[0],
        ContainerQuotaBreach::Memory {
            share_percent: 150.0
        },
        "the halo carries the observed share that crossed the declared ceiling"
    );
}

#[test]
fn an_overrun_stays_visible() {
    let shell = shell_with(vec![summary_with_quota(
        "over",
        "over",
        Some(5.0),
        Some(3 * GIB),
        soft_ceiling(2 * GIB),
    )]);
    let mut app = mount_containers_page(&shell);

    let breaches = halos(&mut app);
    assert_eq!(breaches.len(), 1, "the overrun wears the halo");
    assert_eq!(
        breaches[0],
        ContainerQuotaBreach::Memory {
            share_percent: 150.0
        },
        "the halo carries the real share, not a clamped hundred percent"
    );
    let cells = quota_cells(&mut app);
    assert_eq!(
        cells[0].2, "150.0%",
        "the row shows the overrun as the percentage it reached"
    );
    assert_ne!(
        cells[0].2, "100.0%",
        "an overrun never reads as exactly the declared ceiling"
    );
}

#[test]
fn the_declared_cpu_budget_is_honoured() {
    let over = container_quota_view(&summary_with_quota(
        "over",
        "over",
        Some(150.0),
        Some(GIB),
        cpu_budget(1),
    ));
    assert_eq!(
        over.breach(),
        Some(ContainerQuotaBreach::Cpu {
            observed_cpus: 1.5,
            budget_cpus: 1.0,
        }),
        "a load above the declared CPU budget is an over-limit state"
    );

    let within = container_quota_view(&summary_with_quota(
        "within",
        "within",
        Some(100.0),
        Some(GIB),
        cpu_budget(1),
    ));
    assert_eq!(
        within.breach(),
        None,
        "a load that equals the declared budget is still within it"
    );

    let unbounded = container_quota_view(&summary_with_quota(
        "max",
        "max",
        Some(400.0),
        Some(GIB),
        cpu_without_budget(),
    ));
    assert_eq!(
        unbounded.state,
        ContainerQuotaState::NotDeclared,
        "a CPU declaration without a budget declares no ceiling"
    );
    assert_eq!(
        unbounded.breach(),
        None,
        "a CPU declaration without a budget never produces an over-limit state"
    );
    assert_eq!(
        unbounded.cpu_budget_cpus, None,
        "the page claims no CPU budget the container did not declare"
    );
}

#[test]
fn the_declared_cpu_budget_reaches_the_mounted_row() {
    let shell = shell_with(vec![summary_with_quota(
        "over",
        "over",
        Some(150.0),
        Some(GIB),
        cpu_budget(1),
    )]);
    let mut app = mount_containers_page(&shell);

    let breaches = halos(&mut app);
    assert_eq!(
        breaches,
        vec![ContainerQuotaBreach::Cpu {
            observed_cpus: 1.5,
            budget_cpus: 1.0,
        }],
        "the row that exceeded its declared CPU budget wears the halo"
    );
    let cells = quota_cells(&mut app);
    assert_eq!(
        cells[0].1,
        ContainerQuotaState::Declared,
        "the row states that a ceiling was declared"
    );
    assert!(
        cells[0].2.contains("1.5/1.0"),
        "the row shows the observed load against the declared budget: {readout}",
        readout = cells[0].2
    );
}

#[test]
fn a_declared_ceiling_without_an_observation_claims_no_percentage() {
    let view = container_quota_view(&summary_with_quota(
        "a",
        "unobserved",
        None,
        None,
        soft_ceiling(2 * GIB),
    ));
    assert_eq!(
        view.state,
        ContainerQuotaState::Declared,
        "the declaration was read"
    );
    assert_eq!(
        view.memory_share_percent, None,
        "an unobserved usage has no share"
    );
    let readout = quota_readout(&view);
    assert!(
        !readout.contains('%'),
        "an unobserved usage never becomes a percentage: {readout}"
    );
    assert!(
        readout.contains("2.0"),
        "the row names the declared ceiling it knows: {readout}"
    );
}

#[test]
fn the_warning_halo_pulses_with_the_projection_revision() {
    let alphas: Vec<f32> = (0..4).map(warning_halo_alpha).collect();
    assert!(
        alphas.windows(2).any(|pair| pair[0] != pair[1]),
        "the halo breathes across revisions: {alphas:?}"
    );
    assert!(
        alphas.iter().all(|alpha| (0.0..=1.0).contains(alpha)),
        "the halo opacity stays inside the palette's alpha range: {alphas:?}"
    );
}

/// A headless window with a real UI layout, so every geometry claim in this
/// file is a measured fact.
fn layout_app(width: u32, height: u32, shell: &ShellApp) -> App {
    let mut app = App::new();
    app.add_plugins((
        bevy::MinimalPlugins,
        AssetPlugin::default(),
        ScenePlugin,
        bevy::input::InputPlugin,
        bevy::input_focus::InputFocusPlugin,
        UiWidgetsPlugins,
    ));
    app.init_resource::<bevy::asset::Assets<Font>>();
    app.init_resource::<bevy::asset::Assets<bevy::image::Image>>();
    app.init_resource::<Assets<TextureAtlasLayout>>();
    app.init_resource::<RemSize>();
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
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
        IsDefaultUiCamera,
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
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell,
        palette: &palette,
        history: &history.0,
    };
    app.world_mut()
        .spawn_scene(scene(&context))
        .expect("the containers scene resolves");
    for _ in 0..4 {
        app.update();
    }
    app
}

#[test]
fn the_quota_column_keeps_its_slot_inside_the_row() {
    // The row's slot budget: the column shares of the row's content box.
    let bases = [14.0, 18.0, 14.0, 10.0, 14.0, 12.0, 18.0];
    for (width, height) in [(480, 360), (720, 480), (1280, 720), (1600, 360), (480, 960)] {
        let shell = shell_with(vec![summary_with_quota(
            "a",
            "container",
            Some(5.0),
            Some(GIB),
            soft_ceiling(2 * GIB),
        )]);
        let mut app = layout_app(width, height, &shell);

        let row = app
            .world_mut()
            .query_filtered::<Entity, With<ContainerRowMarker>>()
            .iter(app.world())
            .next()
            .expect("the container row is mounted");
        let row_node = *app
            .world()
            .get::<ComputedNode>(row)
            .expect("the row is laid out");
        let row_x = app
            .world()
            .get::<UiGlobalTransform>(row)
            .expect("the row is placed")
            .translation
            .x;
        let content_right = row_x + row_node.content_box().max.x;
        let content_left = row_x + row_node.content_box().min.x;
        let gap = crate::palette::space_8();
        let content_width = row_node.content_box().width();
        let available = content_width - gap * (bases.len() as f32 - 1.0);

        let cells: Vec<Entity> = app
            .world()
            .get::<Children>(row)
            .expect("the row carries its cells")
            .iter()
            .copied()
            .collect();
        assert_eq!(
            cells.len(),
            bases.len(),
            "the row carries the seven declared slots at {width}x{height}"
        );

        let mut right_edge = f32::MIN;
        for (cell, basis) in cells.iter().zip(bases) {
            let node = *app
                .world()
                .get::<ComputedNode>(*cell)
                .expect("the cell is laid out");
            let cell_x = app
                .world()
                .get::<UiGlobalTransform>(*cell)
                .expect("the cell is placed")
                .translation
                .x;
            let cell_right = cell_x + node.size.x / 2.0;
            assert!(
                cell_right <= content_right + 0.5,
                "the cell stays inside the row's content box at {width}x{height}: \
                 {cell_right} > {content_right}"
            );
            assert!(
                node.size.x > 0.0,
                "every cell keeps a measurable slot at {width}x{height}"
            );
            let expected = available * basis / 100.0;
            assert!(
                (node.size.x - expected).abs() < 1.0,
                "the cell keeps its declared share of the row at {width}x{height}: \
                 {} != {expected}",
                node.size.x
            );
            right_edge = right_edge.max(cell_right);
        }
        assert!(
            (right_edge - content_right).abs() < 1.0,
            "the last slot ends at the row's content edge at {width}x{height}: \
             {right_edge} != {content_right}"
        );
        assert!(
            content_left < right_edge,
            "the row's content box is used at {width}x{height}"
        );
    }
}

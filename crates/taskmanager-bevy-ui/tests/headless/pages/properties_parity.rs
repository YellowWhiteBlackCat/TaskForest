//! Behaviour tests for the process-properties parity surfaces.
//!
//! Every assertion here is measured from the mounted, laid-out surface: the
//! footer's safe band, the memory segment proportions, the GPU micro bars,
//! the quota alerts and the sliding tab indicator are all read back from the
//! world, never from a constant the implementation happens to use.

use super::backdrop::ModalBackdrop;
use super::gpu::{GpuBarKind, GpuBarMark};
use super::memory::{MemorySegment, MemorySegmentMark};
use super::quota::{QuotaGauge, QuotaMark};
use super::tabs::{TabIndicator, TabIndicatorMotion};
use super::{
    ProcessPropertiesDismissButton, ProcessPropertiesFacet, ProcessPropertiesPanel,
    ProcessPropertiesSection, ProcessPropertiesTab, register,
};
use crate::app::FrontendTrack;
use crate::icons::IconPlate;
use crate::input::PendingEffects;
use crate::palette::ui_palette;
use crate::window::{AppShellRoot, WindowPalette};
use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo};
use bevy::color::Alpha;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::With;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::scene::ScenePlugin;
use bevy::text::{Font, RemSize, TextColor};
use bevy::transform::TransformPlugin;
use bevy::ui::prelude::{BackgroundColor, Node, percent};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin};
use bevy::ui_widgets::{Activate, UiWidgetsPlugins, ValueChange};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskforest_core::core::device_state::DeviceState;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskforest_core::core::process_telemetry::{
    LimitValue, ProcessResourceSnapshot, ResourceObservation,
};
use taskforest_core::core::{ProcessResourceObservations, ScalarObservation};
use taskmanager_application::{
    AppAction, AppPage, ProcessInsightFacet, ProcessInsightFacetState, i18n::t,
};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;
use taskmanager_ui_contract::IconId;

/// The measured process behind the open properties review: a unique set of
/// 2 MiB, a proportional shared share of 1 MiB and 1 MiB of swap.
fn measured_process() -> ProcessItem {
    let mib = 1024 * 1024;
    let mut process = ProcessItem::new(42, "measured");
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(100, 1),
        memory_bytes: ScalarObservation::available(4 * mib, 1),
        memory_pss_bytes: ScalarObservation::available(3 * mib, 1),
        memory_uss_bytes: ScalarObservation::available(2 * mib, 1),
        swap_bytes: ScalarObservation::available(mib, 1),
        ..Default::default()
    });
    process
}

/// The shell with one measured process selected and the properties review
/// already open on it.
fn opened_shell() -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |rows| *rows = Some(vec![measured_process()]));
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));
    let _ = shell.apply_action(AppAction::OpenProperties);
    shell
}

/// A headless window with a real UI layout, so every geometry assertion in
/// this file is a measured fact.
fn properties_app(width: u32, height: u32) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        ScenePlugin,
        bevy::input::InputPlugin,
        bevy::input_focus::InputFocusPlugin,
        UiWidgetsPlugins,
    ));
    app.init_resource::<Assets<Font>>();
    app.init_resource::<Assets<bevy::image::Image>>();
    app.init_resource::<Assets<bevy::image::TextureAtlasLayout>>();
    app.init_resource::<RemSize>();
    app.insert_resource(WindowPalette {
        inner: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    app.init_resource::<PendingEffects>();
    app.insert_non_send(FrontendTrack {
        shell: opened_shell(),
        initial_refresh_submitted: true,
        process_tree_expansion: Default::default(),
    });
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
    app.world_mut().spawn((
        Node {
            width: percent(100),
            height: percent(100),
            ..Default::default()
        },
        AppShellRoot,
    ));
    register(&mut app);
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
    for _ in 0..4 {
        app.update();
    }
    app
}

/// Seed the process-insights projection of the frozen review target.
fn seed_insights(app: &mut App, resources: Option<ProcessResourceSnapshot>) {
    let target = app
        .world()
        .non_send::<FrontendTrack>()
        .shell
        .process_properties_target()
        .expect("frozen target")
        .clone();
    let mut projection =
        fixture::process_insights::process_insights_projection(target).expect("projection");
    if let Some(resources) = resources {
        projection.resources = ProcessInsightFacetState::Current(resources);
    }
    fixture::seed_projection_fact(
        &mut app.world_mut().non_send_mut::<FrontendTrack>().shell,
        fixture::ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );
}

/// The resource snapshot of one observed memory quota.
fn memory_quota(usage: Option<u64>, limit: Option<LimitValue>) -> ProcessResourceSnapshot {
    let now = 42_000;
    ProcessResourceSnapshot::from_observations(
        DeviceState::healthy(now),
        ProcessResourceObservations {
            memory_usage_bytes: match usage {
                Some(value) => ResourceObservation::current(value, now),
                None => ResourceObservation::default(),
            },
            memory_limit: match limit {
                Some(value) => ResourceObservation::current(value, now),
                None => ResourceObservation::default(),
            },
            ..ProcessResourceObservations::default()
        },
        Vec::new(),
    )
}

/// Drive the tab strip the way a click does: the typed selection request.
fn select_tab(app: &mut App, section: ProcessPropertiesSection) {
    let (tab, list) = {
        let world = app.world_mut();
        let mut found = None;
        for (entity, tab, parent) in world
            .query::<(Entity, &ProcessPropertiesTab, &ChildOf)>()
            .iter(world)
        {
            if tab.0 == section {
                found = Some((entity, parent.parent()));
                break;
            }
        }
        found.expect("the mounted tab")
    };
    app.world_mut().trigger(ValueChange {
        source: list,
        value: Some(tab),
        is_final: true,
    });
}

/// Drive the facet strip the way a click does.
fn select_facet(app: &mut App, facet: ProcessInsightFacet) {
    let button = {
        let world = app.world_mut();
        world
            .query::<(Entity, &ProcessPropertiesFacet)>()
            .iter(world)
            .find(|(_, button)| button.0 == facet)
            .map(|(entity, _)| entity)
            .expect("the mounted facet")
    };
    app.world_mut().trigger(Activate { entity: button });
}

/// Open the insights facet of the properties review and let it settle.
fn open_insights_facet(app: &mut App, facet: ProcessInsightFacet) {
    select_tab(app, ProcessPropertiesSection::Insights);
    app.update();
    app.update();
    select_facet(app, facet);
    app.update();
    app.update();
}

/// The measured texts of the mounted surface.
fn texts(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect()
}

#[test]
fn the_footer_actions_keep_a_measured_sixteen_pixel_bottom_safe_band() {
    for (width, height) in [(1280u32, 720u32), (900, 600)] {
        let mut app = properties_app(width, height);
        let world = app.world_mut();
        let panel = world
            .query_filtered::<Entity, With<ProcessPropertiesPanel>>()
            .single(world)
            .expect("the properties panel");
        let action = world
            .query_filtered::<Entity, With<ProcessPropertiesDismissButton>>()
            .single(world)
            .expect("the close action");
        let panel_node = world.get::<ComputedNode>(panel).expect("panel geometry");
        let panel_transform = world
            .get::<UiGlobalTransform>(panel)
            .expect("panel position");
        let action_node = world.get::<ComputedNode>(action).expect("action geometry");
        let action_transform = world
            .get::<UiGlobalTransform>(action)
            .expect("action position");
        assert!(
            panel_node.size().y > 0.0 && action_node.size().y > 0.0,
            "the panel and its actions must have a measurable height"
        );
        let panel_bottom = panel_transform.translation.y + panel_node.size().y / 2.0;
        let action_bottom = action_transform.translation.y + action_node.size().y / 2.0;
        let inset = panel_bottom - action_bottom;
        assert!(
            inset >= 16.0 - 0.01,
            "the footer actions must keep a 16px safe band above the modal edge, measured {inset} at {width}x{height}"
        );
    }
}

#[test]
fn the_modal_backdrop_declares_a_frost_over_a_translucent_viewport() {
    let mut app = properties_app(1280, 720);
    let world = app.world_mut();
    let backdrop = world
        .query_filtered::<Entity, With<ModalBackdrop>>()
        .single(world)
        .expect("the frosted viewport");
    let radius = world
        .get::<ModalBackdrop>(backdrop)
        .expect("the backdrop contract")
        .blur_radius_px;
    assert!(
        radius >= 8.0,
        "the frosted viewport must declare a real gaussian radius, got {radius}"
    );
    let layers = world.get::<Children>(backdrop).expect("the frost layers");
    let mut translucent = 0;
    for layer in layers.iter() {
        let Some(color) = world.get::<BackgroundColor>(*layer) else {
            continue;
        };
        let alpha = color.0.alpha();
        assert!(
            alpha > 0.0 && alpha < 1.0,
            "a frost layer stays translucent so the viewport shows through, got {alpha}"
        );
        translucent += 1;
    }
    assert_eq!(
        translucent, 2,
        "the frost is a dim layer plus a diffusion layer"
    );
    let panel = world
        .query_filtered::<Entity, With<ProcessPropertiesPanel>>()
        .single(world)
        .expect("the properties panel");
    let panel_ink = world.get::<BackgroundColor>(panel).expect("the panel fill");
    assert!(
        panel_ink.0.alpha() >= 1.0,
        "the foreground panel stays opaque above the frosted viewport"
    );
}

#[test]
fn the_tab_indicator_slides_onto_the_selected_tab() {
    let mut app = properties_app(1280, 720);
    let motion = *app.world().resource::<TabIndicatorMotion>();
    assert_eq!(motion.target, Some(ProcessPropertiesSection::Overview));
    assert_eq!(
        motion.progress, 1.0,
        "the first mount lands directly on the overview tab"
    );
    select_tab(&mut app, ProcessPropertiesSection::Insights);
    app.update();
    app.update();
    let motion = *app.world().resource::<TabIndicatorMotion>();
    assert_eq!(motion.target, Some(ProcessPropertiesSection::Insights));
    assert!(
        motion.progress < 1.0,
        "the indicator slides toward the new tab instead of jumping, progress {}",
        motion.progress
    );
    for _ in 0..12 {
        app.update();
    }
    let motion = *app.world().resource::<TabIndicatorMotion>();
    assert_eq!(motion.progress, 1.0, "the slide lands on the selected tab");
    let world = app.world_mut();
    let indicator = world
        .query_filtered::<Entity, With<TabIndicator>>()
        .single(world)
        .expect("the sliding indicator");
    let mut tab = None;
    for (entity, marker) in world.query::<(Entity, &ProcessPropertiesTab)>().iter(world) {
        if marker.0 == ProcessPropertiesSection::Insights {
            tab = Some(entity);
        }
    }
    let tab = tab.expect("the insights tab");
    let indicator_size = world
        .get::<ComputedNode>(indicator)
        .expect("indicator geometry")
        .size();
    let indicator_left = world
        .get::<UiGlobalTransform>(indicator)
        .expect("indicator position")
        .translation
        .x
        - indicator_size.x / 2.0;
    let tab_size = world.get::<ComputedNode>(tab).expect("tab geometry").size();
    let tab_left = world
        .get::<UiGlobalTransform>(tab)
        .expect("tab position")
        .translation
        .x
        - tab_size.x / 2.0;
    assert!(
        tab_size.x > 0.0,
        "the selected tab must have a measurable width"
    );
    assert!(
        (indicator_size.x - tab_size.x).abs() <= 1.0,
        "the indicator spans the selected tab: {} against {}",
        indicator_size.x,
        tab_size.x
    );
    assert!(
        (indicator_left - tab_left).abs() <= 1.5,
        "the indicator lands on the selected tab: {indicator_left} against {tab_left}"
    );
}

#[test]
fn the_memory_comparison_bar_keeps_the_measured_segment_proportions() {
    let mut app = properties_app(1280, 720);
    let world = app.world_mut();
    let mut widths: Vec<(MemorySegment, f32)> = Vec::new();
    for (entity, marker) in world.query::<(Entity, &MemorySegmentMark)>().iter(world) {
        let node = world.get::<ComputedNode>(entity).expect("segment geometry");
        widths.push((marker.0, node.size().x));
    }
    assert_eq!(
        widths.len(),
        3,
        "the overview draws the unique, the shared and the swap segment"
    );
    let total: f32 = widths.iter().map(|(_, width)| *width).sum();
    assert!(
        total > 0.0,
        "the comparison bar must have a measurable width"
    );
    for (segment, width) in widths {
        let share = width / total;
        let expected = match segment {
            MemorySegment::Unique => 0.5,
            MemorySegment::Shared => 0.25,
            MemorySegment::Swap => 0.25,
        };
        assert!(
            (share - expected).abs() < 0.02,
            "the {segment:?} segment must keep its measured share, got {share}"
        );
    }
}

#[test]
fn the_gpu_facet_draws_the_vram_and_the_engine_micro_bars() {
    let mut app = properties_app(1280, 720);
    seed_insights(&mut app, None);
    open_insights_facet(&mut app, ProcessInsightFacet::Gpu);
    let world = app.world_mut();
    let mut bars: Vec<(GpuBarKind, f32)> = Vec::new();
    for (entity, marker) in world.query::<(Entity, &GpuBarMark)>().iter(world) {
        let node = world.get::<Node>(entity).expect("the bar fill");
        bars.push((marker.0, node.flex_grow));
    }
    assert_eq!(
        bars.len(),
        4,
        "the GPU facet draws the device memory, the device usage and both engines"
    );
    let share_of = |kind: GpuBarKind| {
        bars.iter()
            .filter(|(bar, _)| *bar == kind)
            .map(|(_, share)| *share)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        share_of(GpuBarKind::DeviceMemory),
        vec![1.0],
        "the only device holds the process's whole dedicated GPU memory"
    );
    assert_eq!(
        share_of(GpuBarKind::DeviceUsage),
        vec![0.375],
        "the device utilization bar rides the observed rate"
    );
    let mut engines = share_of(GpuBarKind::EngineUsage);
    engines.sort_by(|a, b| a.partial_cmp(b).expect("finite shares"));
    assert_eq!(
        engines,
        vec![0.0, 0.375],
        "every engine bar rides its own observed rate"
    );
}

#[test]
fn the_resource_quota_alert_follows_the_observed_limit() {
    let mut app = properties_app(1280, 720);
    let mib = 1024 * 1024;
    seed_insights(
        &mut app,
        Some(memory_quota(
            Some(1000 * mib),
            Some(LimitValue::Value(1024 * mib)),
        )),
    );
    open_insights_facet(&mut app, ProcessInsightFacet::Resources);
    let critical_ink = crate::palette::theme_color(
        app.world()
            .resource::<WindowPalette>()
            .inner
            .severity
            .critical(),
    );
    let world = app.world_mut();
    let mut gauges: Vec<(String, bool, f32)> = Vec::new();
    for (entity, marker) in world.query::<(Entity, &QuotaGauge)>().iter(world) {
        let node = world.get::<Node>(entity).expect("the gauge fill");
        gauges.push((marker.0.clone(), marker.1, node.flex_grow));
    }
    let memory = gauges
        .iter()
        .find(|(label, _, _)| label == t("common.memory"))
        .expect("the memory quota row");
    assert!(
        memory.1,
        "a quota at 97% of its declared limit must raise the alert"
    );
    assert!(
        (memory.2 - 1000.0 / 1024.0).abs() < 0.01,
        "the alert bar rides the observed share, got {}",
        memory.2
    );
    let alerts = world
        .query::<&IconPlate>()
        .iter(world)
        .filter(|plate| plate.0 == IconId::TriangleAlert)
        .count();
    assert!(alerts > 0, "a critical quota shows the alert icon");
    let critical_rows = world
        .query::<&QuotaMark>()
        .iter(world)
        .filter(|mark| mark.0)
        .count();
    assert_eq!(critical_rows, 1, "only the critical quota flips its colour");
    let mut flipped = false;
    for (entity, text, color) in world.query::<(Entity, &Text, &TextColor)>().iter(world) {
        if world.get::<super::quota::QuotaAlert>(entity).is_some() {
            assert_eq!(
                color.0.to_srgba(),
                critical_ink.to_srgba(),
                "the critical readout flips into the theme's critical ink"
            );
            assert!(
                text.0.contains("1000.0 MiB"),
                "the alert row carries the observed quota, got {}",
                text.0
            );
            flipped = true;
        }
    }
    assert!(flipped, "the critical quota readout must be mounted");
}

#[test]
fn an_undeclared_quota_stays_honest_instead_of_becoming_a_percentage() {
    let mut app = properties_app(1280, 720);
    let mib = 1024 * 1024;
    seed_insights(&mut app, Some(memory_quota(Some(384 * mib), None)));
    open_insights_facet(&mut app, ProcessInsightFacet::Resources);
    let readouts = texts(&mut app);
    let memory = readouts
        .iter()
        .find(|value| value.contains("384.0 MiB"))
        .expect("the memory quota readout");
    // The quota surface says "not declared" for a limit nobody declared, and
    // keeps the plain "unknown" wording for a reading it could not observe.
    assert!(
        memory.contains(t("proc_insights.not_declared")),
        "an undeclared quota renders as not declared, got {memory}"
    );
    assert!(
        !memory.contains('%'),
        "an undeclared quota never becomes a percentage, got {memory}"
    );
    let world = app.world_mut();
    let mut gauges: Vec<(String, bool, f32)> = Vec::new();
    for (entity, marker) in world.query::<(Entity, &QuotaGauge)>().iter(world) {
        let node = world.get::<Node>(entity).expect("the gauge fill");
        gauges.push((marker.0.clone(), marker.1, node.flex_grow));
    }
    let memory = gauges
        .iter()
        .find(|(label, _, _)| label == t("common.memory"))
        .expect("the memory quota row");
    assert!(!memory.1, "an undeclared quota never raises a limit alert");
    assert_eq!(
        memory.2, 0.0,
        "an undeclared quota never draws a measured share"
    );
}

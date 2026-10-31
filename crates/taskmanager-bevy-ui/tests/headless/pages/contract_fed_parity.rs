//! test-intent: behavior
//!
//! The four contract-fed surfaces of the Bevy frontend. Each check reads the
//! RENDERED scene or the shared projection — never the source text, never a
//! fabricated value:
//!
//! - **BEVY-104**: the curve hover pill reads the sampling INSTANT of the
//!   hovered sample, never a `-N` sample offset;
//! - **BEVY-131**: the danger confirmation lists exactly the FROZEN
//!   collateral of the armed gate, and a later refresh of the live process
//!   table cannot change that list;
//! - **BEVY-148**: the boot overview compares the MEASURED boot phases as
//!   proportion bars, and an unmeasured phase gets an explicit row instead
//!   of a zero-width bar;
//! - **BEVY-162**: the NPU card draws the accelerator's power curve only
//!   while the power fact exists — an absent capability draws no curve.

use std::sync::Arc;

use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::input::InputPlugin;
use bevy::input_focus::InputFocusPlugin;
use bevy::math::{UVec2, Vec2};
use bevy::picking::DefaultPickingPlugins;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::{Font, TextPlugin};
use bevy::transform::TransformPlugin;
use bevy::ui::widget::Text;
use bevy::ui::{Node, UiPlugin};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskforest_core::core::DeviceGeneration;
use taskforest_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, MetricSample, ScalarObservation, SystemSnapshot,
};
use taskforest_core::core::npu::{
    NpuDevice, NpuInventorySnapshot, NpuPowerObservation, NpuPowerReport,
};
use taskforest_core::core::process::{
    ProcessBatchAction, ProcessBatchIntent, ProcessItem, ProcessLiveKey, ProcessScalarObservations,
};
use taskforest_core::core::startup::{StartupBootEvidenceSnapshot, StartupTimelineSnapshot};
use taskforest_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::{
    ProjectionSeedFact, edit_processes, record_demo_history_frame, seed_projection_fact,
};
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::confirmation::{ConfirmationCollateralRow, PendingConfirmationView};
use crate::pages::performance::SystemCurve;
use crate::pages::performance::scene::chart::curve_card_scene;
use crate::pages::performance::scene::hover::{CurveSeries, HostLocalTime};
use crate::pages::performance::scene::npu_power::NpuPowerStrip;
use crate::pages::startup::phases::{
    BootPhase, StartupPhaseBar, StartupPhaseRow, boot_phase_slices,
};
use crate::pages::startup::timeline::boot_timeline_scene;
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::widgets::chart::{ChartCanvas, ChartContent, ChartHoverLabel, ChartPointer};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;
use taskmanager_application::PendingConfirmation;
use taskmanager_test_support::pin_english;

// ---- fixtures ---------------------------------------------------------------

/// The palette the headless compositions render with.
fn palette() -> crate::palette::UiPalette {
    ui_palette(&Theme::dark(), UiSize::Standard)
}

/// A CPU observation with one measured global usage.
fn cpu_metrics(usage_pct: f32, at_ms: u64) -> CpuMetrics {
    CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(usage_pct, at_ms),
        ..CpuScalarObservations::default()
    })
}

/// One token-bearing process row.
fn process(pid: u32, name: &str, parent_pid: Option<u32>) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.parent_pid = parent_pid;
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    process
}

/// The live key of one process row.
fn live_key(process: &ProcessItem) -> ProcessLiveKey {
    ProcessLiveKey::from_process(process).expect("the fixture row carries a start token")
}

/// A quiet platform client: the headless composition never talks to a host.
fn quiet_runtime() -> &'static SharedRuntime {
    use taskforest_contract::{
        CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
        RequestEnvelope, RequestPort, SubmissionError,
    };
    use taskmanager_application::{
        HostTelemetryRequest, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
        SystemFacets,
    };

    struct Quiet;

    impl CapabilityCatalog for Quiet {
        fn snapshot(&self) -> CapabilitySnapshot {
            CapabilitySnapshot::default()
        }
    }

    impl EventPort for Quiet {
        type Event = PlatformEvent;

        fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
            Ok(None)
        }
    }

    impl RequestPort for Quiet {
        type Request = HostTelemetryRequest;

        fn try_submit(
            &self,
            _request: RequestEnvelope<Self::Request>,
        ) -> Result<(), SubmissionError> {
            Ok(())
        }
    }

    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(Quiet),
        Arc::new(Quiet),
        PlatformFacets::default().with_system(SystemFacets::default().with_host(Arc::new(Quiet))),
    ));
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    cache
        .get_or_init(|| Ok(client))
        .expect("the quiet runtime always starts")
}

/// A headless composition with the REAL UI layout: the chart's slot is
/// measured, so the hover seam resolves the sample under the pointer.
fn layout_app(width: u32, height: u32) -> App {
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins)
        .add_plugins(HeadlessFrontendPlugins)
        .add_plugins(FrontendWindowPlugin {
            runtime: quiet_runtime(),
            palette: palette(),
        });
    app.init_resource::<Assets<Font>>();
    app.init_resource::<Assets<bevy::image::TextureAtlasLayout>>();
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
    app
}

/// A headless composition for the surfaces that need no measured slot.
fn window_app() -> App {
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins);
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        InputPlugin,
        InputFocusPlugin,
    ));
    app.init_resource::<Assets<Font>>();
    app.add_plugins(FrontendWindowPlugin {
        runtime: quiet_runtime(),
        palette: palette(),
    });
    app
}

/// Route to the Performance page the programmatic way (the same pair the
/// keyboard adapter performs).
fn route_to_performance(app: &mut App) {
    app.world_mut().resource_mut::<Route>().page = Page::Performance;
    app.world_mut().commands().trigger(RouteChanged);
    app.update();
    app.update();
}

/// The canvas of one curve card.
fn curve_canvas(app: &mut App, curve: SystemCurve) -> Entity {
    let world = app.world_mut();
    world
        .query_filtered::<(Entity, &CurveSeries), With<CurveSeries>>()
        .iter(world)
        .find(|(_, series)| series.0 == curve)
        .map(|(entity, _)| entity)
        .expect("the curve card mounts its canvas")
}

/// The text of the hover pill that rides on one canvas.
fn pill_text(app: &mut App, canvas: Entity) -> Option<String> {
    let world = app.world_mut();
    let children = world.get::<Children>(canvas)?;
    for child in children.iter() {
        if world.get::<ChartHoverLabel>(*child).is_none() {
            continue;
        }
        let pill_children = world.get::<Children>(*child)?;
        let text_entity = pill_children.first()?;
        return world.get::<Text>(*text_entity).map(|text| text.0.clone());
    }
    None
}

// ---- BEVY-104: the hover pill reads the sampling instant --------------------

#[test]
fn the_hover_pill_reads_the_sampling_instant_instead_of_a_sample_offset() {
    pin_english();
    // One measured CPU sample: the value 42% observed at a fixed instant.
    let at_ms = 1_767_225_600_000_u64;
    let mut app = layout_app(1280, 720);
    let mut shell = ShellApp::new();
    let snapshot = SystemSnapshot {
        timestamp_ms: at_ms,
        cpu: cpu_metrics(42.0, at_ms),
        ..Default::default()
    };
    record_demo_history_frame(&mut shell, &snapshot, None, None);
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    route_to_performance(&mut app);
    for _ in 0..4 {
        app.update();
    }

    // The pointer rests over the newest sample of the CPU curve.
    let canvas = curve_canvas(&mut app, SystemCurve::Cpu);
    {
        let world = app.world_mut();
        let mut pointer = world
            .get_mut::<ChartPointer>(canvas)
            .expect("the canvas carries the pointer seam");
        pointer.normalized = Some(Vec2::new(0.49, 0.0));
    }
    for _ in 0..3 {
        app.update();
    }

    let text = pill_text(&mut app, canvas).expect("the hover pill is mounted");
    assert_eq!(
        text, "42% · 2026-01-01 00:00:00",
        "the pill reads the instant the sample was measured at"
    );
    assert!(
        !text.contains(" -"),
        "the pill never spells a sample offset: {text}"
    );
}

#[test]
fn a_sample_without_a_measurement_instant_reads_the_shared_dash() {
    pin_english();
    // The instant is the SAMPLE's own measurement time: a sample without one
    // has no instant to show, and the shared dash is the honest answer.
    let sample = MetricSample::gap(1_000);
    let rules = LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0);
    let text = crate::pages::performance::scene::hover::pill_text(
        Some(|value| format!("{value:.0}%")),
        42.0,
        Some(&sample),
        &rules,
    );
    assert_eq!(
        text,
        format!("42% · {MISSING_VALUE}"),
        "a gap sample shows the shared dash, never an offset"
    );
    assert!(!text.contains(" -"), "the dash is not an offset: {text}");
}

// ---- BEVY-131: the danger confirmation lists the frozen collateral -----------

#[test]
fn the_danger_confirmation_lists_the_frozen_collateral_and_a_refresh_cannot_change_it() {
    pin_english();
    let mut app = window_app();
    let root = process(10, "editor", None);
    let child = process(11, "editor-worker", Some(10));
    let grandchild = process(12, "editor-helper", Some(11));
    let root_key = live_key(&root);
    {
        let shell = &mut app.world_mut().non_send_mut::<FrontendTrack>().shell;
        edit_processes(shell, |processes| {
            *processes = Some(vec![root.clone(), child.clone(), grandchild.clone()]);
        });
        shell.request_process_tree_end(root_key);
    }
    app.update();
    app.update();

    // The armed gate carries the FROZEN collateral: the root first, then
    // every descendant with its depth.
    let view = {
        let shell = &app.world().non_send::<FrontendTrack>().shell;
        let pending = shell
            .pending_confirmation()
            .expect("the tree end armed the shared gate");
        PendingConfirmationView::from_pending(pending).expect("the batch renders a dialog view")
    };
    assert_eq!(
        view.collateral
            .iter()
            .map(|row| (row.name.as_str(), row.pid, row.depth))
            .collect::<Vec<_>>(),
        vec![
            ("editor", 10, 0),
            ("editor-worker", 11, 1),
            ("editor-helper", 12, 2),
        ],
        "the confirmation lists the frozen collateral with its depths"
    );

    // The mounted dialog shows the same rows.
    app.world_mut()
        .commands()
        .trigger(crate::confirmation::ConfirmationChanged(Some(view.clone())));
    app.update();
    app.update();
    let rows: Vec<(usize, u32)> = {
        let world = app.world_mut();
        world
            .query_filtered::<&ConfirmationCollateralRow, With<ConfirmationCollateralRow>>()
            .iter(world)
            .map(|row| (row.depth, row.pid))
            .collect()
    };
    assert_eq!(
        rows,
        vec![(0, 10), (1, 11), (2, 12)],
        "the dialog draws the frozen tree as an indented list"
    );

    // A LATER refresh of the live table cannot change the frozen list: the
    // gate still holds the intent it froze at arming time.
    {
        let shell = &mut app.world_mut().non_send_mut::<FrontendTrack>().shell;
        edit_processes(shell, |processes| {
            let mut rows = processes.clone().unwrap_or_default();
            rows.push(process(13, "editor-late", Some(10)));
            *processes = Some(rows);
        });
    }
    let republished = {
        let shell = &app.world().non_send::<FrontendTrack>().shell;
        let pending = shell
            .pending_confirmation()
            .expect("the gate is still armed");
        PendingConfirmationView::from_pending(pending).expect("the batch renders a dialog view")
    };
    assert_eq!(
        republished.collateral, view.collateral,
        "a refresh of the live rows cannot change the frozen collateral"
    );
}

#[test]
fn a_plain_selection_batch_keeps_the_flat_echo_without_a_collateral_list() {
    pin_english();
    let intent = ProcessBatchIntent::freeze(
        &[process(20, "worker", None)],
        [live_key(&process(20, "worker", None))],
        ProcessBatchAction::End,
    );
    let view = PendingConfirmationView::from_pending(&PendingConfirmation::ProcessBatch(intent))
        .expect("a selection batch renders a dialog view");
    assert!(
        view.collateral.is_empty(),
        "a plain selection has no hierarchy to draw"
    );
    assert!(
        view.body.contains("worker"),
        "the flat target echo stays: {}",
        view.body
    );
}

// ---- BEVY-148: the boot overview compares the measured phases ---------------

#[test]
fn the_boot_overview_bars_follow_the_measured_phases_and_name_the_unmeasured_one() {
    pin_english();
    // Firmware and kernel are measured; the user-space phase is not.
    let evidence = StartupBootEvidenceSnapshot {
        phase_timeline: Some(StartupTimelineSnapshot {
            firmware_ms: Some(1_000),
            loader_ms: None,
            kernel_ms: Some(2_000),
            user_space_ms: None,
        }),
        ..Default::default()
    };
    let slices = boot_phase_slices(Some(&evidence)).expect("the published split has rows");
    assert_eq!(slices.len(), 3, "the overview compares three boot phases");

    let mut app = window_app();
    let scene = boot_timeline_scene(Some(&evidence), &palette()).expect("the card mounts");
    app.world_mut()
        .spawn_scene(scene)
        .expect("the boot overview scene resolves");
    app.update();

    let rows: Vec<StartupPhaseRow> = {
        let world = app.world_mut();
        world
            .query_filtered::<&StartupPhaseRow, With<StartupPhaseRow>>()
            .iter(world)
            .copied()
            .collect()
    };
    assert_eq!(rows.len(), 3, "every compared phase has a row");
    let firmware = rows
        .iter()
        .find(|row| row.0 == BootPhase::Firmware)
        .expect("the firmware row");
    let kernel = rows
        .iter()
        .find(|row| row.0 == BootPhase::Kernel)
        .expect("the kernel row");
    let userspace = rows
        .iter()
        .find(|row| row.0 == BootPhase::Userspace)
        .expect("the userspace row");
    assert_eq!(
        firmware.1,
        Some(1_000),
        "the firmware row carries its measurement"
    );
    assert_eq!(
        kernel.1,
        Some(2_000),
        "the kernel row carries its measurement"
    );
    assert_eq!(
        userspace.1, None,
        "an unmeasured phase is never a measured zero"
    );

    // The bars carry the measured share of the compared phases.
    let bars: Vec<(StartupPhaseBar, f32)> = {
        let world = app.world_mut();
        world
            .query_filtered::<(&StartupPhaseBar, &Node), With<StartupPhaseBar>>()
            .iter(world)
            .map(|(bar, node)| {
                let width = match node.width {
                    bevy::ui::Val::Percent(value) => value,
                    other => {
                        panic!("a phase bar is a percentage of the measured scale: {other:?}")
                    }
                };
                (*bar, width)
            })
            .collect()
    };
    assert_eq!(
        bars.len(),
        2,
        "only the measured phases carry a bar: {bars:?}"
    );
    let firmware_bar = bars
        .iter()
        .find(|(bar, _)| bar.0 == BootPhase::Firmware)
        .expect("the firmware bar");
    let kernel_bar = bars
        .iter()
        .find(|(bar, _)| bar.0 == BootPhase::Kernel)
        .expect("the kernel bar");
    assert!(
        (firmware_bar.1 - (1000.0 / 3000.0) * 100.0).abs() < 0.01,
        "the firmware bar is its share of the measured phases: {}",
        firmware_bar.1
    );
    assert!(
        (kernel_bar.1 - (2000.0 / 3000.0) * 100.0).abs() < 0.01,
        "the kernel bar is its share of the measured phases: {}",
        kernel_bar.1
    );
    assert!(
        bars.iter().all(|(bar, _)| bar.0 != BootPhase::Userspace),
        "the unmeasured phase draws no bar at all"
    );
    let texts: Vec<String> = {
        let world = app.world_mut();
        world
            .query_filtered::<&Text, With<Text>>()
            .iter(world)
            .map(|text| text.0.clone())
            .collect()
    };
    assert!(
        texts.contains(&t("startup.impact_unmeasured").to_owned()),
        "the unmeasured phase says so explicitly: {texts:?}"
    );
}

// ---- BEVY-162: the NPU card draws the power curve only with the fact -------

#[test]
fn the_npu_card_draws_the_power_curve_only_when_the_power_fact_exists() {
    pin_english();
    let mut app = window_app();
    let mut shell = ShellApp::new();

    // No inventory at all: no power fact, so no power curve.
    let card = curve_card_scene(SystemCurve::Npu, &shell, &palette());
    app.world_mut()
        .spawn_scene(card)
        .expect("the NPU card scene resolves");
    app.update();
    let without_fact = {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<NpuPowerStrip>>()
            .iter(world)
            .count()
    };
    assert_eq!(
        without_fact, 0,
        "an accelerator without the power capability draws no power curve"
    );

    // An inventory that REPORTS the power fact: the card grows the power
    // curve and the shell owns the bounded history behind it.
    let mut device = NpuDevice {
        device_id: "npu0".into(),
        device_generation: DeviceGeneration::new(1),
        brand: None,
        driver: None,
        utilization_pct: ScalarObservation::available(20.0, 1_000),
        engines: Vec::new(),
        memory: Default::default(),
    };
    device.device_generation = DeviceGeneration::new(1);
    let snapshot = NpuInventorySnapshot::discovered(vec![device], 1_000).with_power(vec![
        NpuPowerObservation {
            device_id: "npu0".into(),
            report: NpuPowerReport {
                power_w: ScalarObservation::available(12.5, 1_000),
                energy_j: ScalarObservation::available(3_600, 1_000),
            },
        },
    ]);
    seed_projection_fact(&mut shell, ProjectionSeedFact::NpuInventory(Some(snapshot)));
    assert_eq!(
        shell.projection().npu_power_history.len(),
        1,
        "the shell owns the bounded power history"
    );
    assert_eq!(
        shell.projection().npu_power_history.front().copied(),
        Some((1_000, 12.5)),
        "the history sample carries the observation instant and the measured draw"
    );

    let card = curve_card_scene(SystemCurve::Npu, &shell, &palette());
    app.world_mut()
        .spawn_scene(card)
        .expect("the NPU card scene resolves");
    app.update();
    let power_curve: Vec<Vec<f32>> = {
        let world = app.world_mut();
        world
            .query_filtered::<(&ChartCanvas, &NpuPowerStrip), With<NpuPowerStrip>>()
            .iter(world)
            .map(|(canvas, _)| match &canvas.content {
                ChartContent::Samples { values, .. } => values.clone(),
                other => panic!("the power curve carries its samples: {other:?}"),
            })
            .collect()
    };
    assert_eq!(
        power_curve,
        vec![vec![12.5]],
        "the power curve draws the shell's measured watts"
    );
}

#[test]
fn the_local_time_rules_default_to_the_explicit_utc_rule_set() {
    // The pill's instant is rendered through the shared formatter, which
    // needs a rule set: the frontend's copy is the explicit fixed UTC rules
    // (a real rule set, never an unavailable zone reinterpreted as UTC).
    let rules = HostLocalTime::default();
    assert!(
        rules.0.rules().is_some(),
        "the default rule set is a real rule set"
    );
    let sample = MetricSample::measured(1.0, 0, 0);
    let text =
        crate::pages::performance::scene::hover::pill_text(None, 1.0, Some(&sample), &rules.0);
    assert_eq!(
        text,
        "1 · 1970-01-01 00:00:00".to_owned(),
        "the instant is printed through the shared formatter"
    );
}

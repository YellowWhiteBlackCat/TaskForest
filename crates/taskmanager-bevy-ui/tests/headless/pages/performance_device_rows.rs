//! test-intent: behavior
//!
//! Performance-page device-block behavior tests: the disk caption's observed
//! I/O-depth and SMART evidence, the projected partition rows, and the
//! per-adapter GPU blocks. Split from `performance.rs` so each test file stays
//! inside the test source budget; the page's other tests stay there.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::With;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::transform::TransformPlugin;
use bevy::ui::prelude::{Display, Node, Val};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiPlugin};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskforest_core::core::SystemSnapshot;
use taskforest_core::core::metrics::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, ScalarObservation,
};
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::{demo_app, edit_snapshot};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::replay::{PerformanceHistoryEntry, PerformanceLiveBody, PerformanceReplayRoot};
use super::scene::content;
use super::tests::{block_keys, dyn_text_value, headless_perf_app, route_to_performance};
use super::{DynField, Section, section_keys};
use super::{DynText, PerformanceDeviceTarget};
use crate::app::{FrontendTrack, PageContext};
use crate::palette::ui_palette;
use crate::widgets::bounded_text::{BoundedText, ELLIPSIS};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;
use bevy::image::TextureAtlasLayout;
use bevy::text::TextPlugin;
use taskmanager_shell::presentation::device_status_i18n_key;
use taskmanager_shell::presentation::duration;
use taskmanager_shell::presentation::effective_smart_status;

/// A bare scene world for the page-assembly tests (the same minimal
/// composition `performance.rs` uses for its mounted-scene tests).
fn headless_scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

#[test]
fn disk_caption_renders_observed_iops_latency_and_queue_depth() {
    use taskforest_core::core::metrics::DiskScalarObservations;

    // Every throughput-depth fact comes from its own typed observation: the
    // painted caption must carry the observed values, never a placeholder.
    let mut disk = DiskMetrics::default();
    disk.apply_scalar_observations(DiskScalarObservations {
        iops: ScalarObservation::available(137, 1),
        response_time_ms: ScalarObservation::available(1.5, 1),
        average_queue_depth: ScalarObservation::available(2.25, 1),
        service_time_ms: ScalarObservation::available(2.5, 1),
        ..DiskScalarObservations::default()
    });
    assert_eq!(
        super::metrics::disk_caption(&disk),
        format!(
            "— · — · — · {} 137 · 1.5 ms · {} 2.25 · {} 2.5 ms",
            t("disk.iops"),
            t("disk.queue_depth"),
            t("disk.service_time"),
        )
    );

    // Unobserved depth facts stay omitted rather than a fabricated zero.
    let cold = super::metrics::disk_caption(&DiskMetrics::default());
    assert!(
        !cold.contains(t("disk.iops")) && !cold.contains(t("disk.queue_depth")),
        "an unobserved depth fact must not become a zero segment: {cold}"
    );
}

#[test]
fn disk_caption_renders_observed_smart_evidence_rows() {
    use taskforest_core::core::metrics::SmartAvailability;

    // The SMART family (temperature, per-sensor readings, endurance, spare,
    // power-on hours, unsafe shutdowns) all reach the painted caption from the
    // shared device projection.
    let mut disk = DiskMetrics::default();
    disk.smart_availability = SmartAvailability::Available;
    disk.smart_temperature_c = Some(41.0);
    disk.smart_temperature_sensors_c = vec![33.0, 41.0];
    disk.smart_percent_used = Some(2.0);
    disk.smart_available_spare_pct = Some(4.0);
    disk.smart_available_spare_threshold_pct = Some(10.0);
    disk.smart_power_on_hours = Some(7200);
    disk.smart_unsafe_shutdowns = Some(12);
    let caption = super::metrics::disk_caption(&disk);
    assert!(
        caption.contains(&format!("{} 41°C", t("common.temperature"))),
        "the observed SMART temperature must paint: {caption}"
    );
    assert!(
        caption.contains(&format!("{} 1 33°C", t("disk.temperature_sensor")))
            && caption.contains(&format!("{} 2 41°C", t("disk.temperature_sensor"))),
        "each observed SMART temperature sensor must paint with its index: {caption}"
    );
    assert!(
        caption.contains(&format!("{} 2%", t("disk.endurance_used"))),
        "the observed endurance percentage must paint: {caption}"
    );
    assert!(
        caption.contains(&format!("{} 4%", t("disk.available_spare"))),
        "the observed spare percentage must paint: {caption}"
    );
    assert!(
        caption.contains(
            &t("disk.power_on_format")
                .replace("{hours}", "7200")
                .replace("{days}", "300")
        ),
        "the observed power-on hours must paint: {caption}"
    );
    assert!(
        caption.contains(&format!("{} 12", t("disk.unsafe_shutdowns"))),
        "the observed unsafe-shutdown count must paint: {caption}"
    );
    // The spare warning plate follows the observed threshold, not a constant.
    assert!(
        super::metrics::disk_spare_warning(&disk),
        "4% spare at a 10% threshold must raise the warning plate"
    );

    // Reported availability without any concrete field still speaks through
    // the shared status fold; an honestly hidden SMART section adds nothing.
    let mut available_only = DiskMetrics::default();
    available_only.smart_availability = SmartAvailability::Available;
    let availability_caption = super::metrics::disk_caption(&available_only);
    assert!(
        availability_caption.contains(&format!(
            "{} {}",
            t("disk.smart_status"),
            t(device_status_i18n_key(effective_smart_status(
                &available_only
            )))
        )),
        "reported availability must paint the shared SMART status: {availability_caption}"
    );
    for unavailable in [
        SmartAvailability::MissingTool,
        SmartAvailability::PermissionDenied,
    ] {
        let mut disk = DiskMetrics::default();
        disk.smart_availability = unavailable;
        let caption = super::metrics::disk_caption(&disk);
        assert!(caption.contains(t("disk.smart_status")));
        assert!(
            caption.contains(t(device_status_i18n_key(effective_smart_status(&disk)))),
            "the actionable SMART failure must remain explicit: {caption}"
        );
    }

    // A disk whose provider supplied nothing keeps every SMART segment
    // absent; no fabricated `0%` spare row.
    let cold = super::metrics::disk_caption(&DiskMetrics::default());
    for absent in [
        t("disk.available_spare"),
        t("disk.unsafe_shutdowns"),
        t("disk.endurance_used"),
        t("disk.temperature_sensor"),
        t("disk.smart_status"),
    ] {
        assert!(
            !cold.contains(absent),
            "unobserved SMART facts must stay absent: {cold}"
        );
    }
}

#[test]
fn gpu_section_enumerates_every_projected_adapter() {
    use taskforest_core::core::metrics::{
        GpuEngine, GpuEngineKind, GpuMetrics, GpuScalarObservations, SystemSnapshot,
    };
    use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

    fn adapter(id: &str, model: &str, usage_pct: f32, engine: &str) -> GpuMetrics {
        let mut gpu = GpuMetrics::new(id, model);
        gpu.engines = vec![GpuEngine {
            name: engine.into(),
            kind: GpuEngineKind::Render,
            usage_pct,
        }];
        gpu.apply_scalar_observations(GpuScalarObservations {
            utilization_pct: ScalarObservation::available(usage_pct, 1),
            ..GpuScalarObservations::default()
        });
        gpu
    }

    // A dGPU and an iGPU in one projection: each adapter must get its own
    // block with its own identity and facts.
    let mut shell = ShellApp::new();
    let snapshot = SystemSnapshot {
        gpu: vec![
            adapter("gpu:pci:0000:01:00.0", "Discrete GPU", 42.0, "render"),
            adapter("gpu:pci:0000:00:02.0", "Integrated Graphics", 7.0, "copy"),
        ],
        ..SystemSnapshot::default()
    };
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    assert_eq!(
        section_keys(&shell, Section::Gpu),
        vec![
            "gpu:pci:0000:01:00.0".to_owned(),
            "gpu:pci:0000:00:02.0".to_owned()
        ],
        "the section enumerates every projected adapter in projection order"
    );

    // The mounted page really spawns one block per adapter with that
    // adapter's own fact line and engine rows.
    let mut app = headless_scene_app();
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell: &shell,

        palette: &palette,
        history: &history.0,
    };
    let world = app.world_mut();
    let root = world
        .spawn_scene(content(&context))
        .expect("the seeded performance page resolves")
        .id();
    assert_eq!(
        block_keys(world, Section::Gpu),
        vec![
            "gpu:pci:0000:00:02.0".to_owned(),
            "gpu:pci:0000:01:00.0".to_owned()
        ],
        "two adapters must mount two distinct device blocks"
    );
    for (device, expected) in [
        ("gpu:pci:0000:01:00.0", "42.0%"),
        ("gpu:pci:0000:00:02.0", "7.0%"),
    ] {
        let field = DynField::Device {
            section: Section::Gpu,
            device: device.to_owned(),
        };
        let line = dyn_text_value(world, &field).expect("each adapter has its own fact line");
        assert!(
            line.starts_with(expected),
            "adapter {device} must paint its own utilization: {line}"
        );
    }
    let texts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    for expected in ["Discrete GPU", "Integrated Graphics", "render", "copy"] {
        assert!(
            texts.iter().any(|text| text == expected),
            "the {expected} identity/engine row must paint: {texts:?}"
        );
    }
    // Each adapter's per-engine utilization row paints that engine's own
    // percentage (the joined fact line is never exactly one percentage).
    for expected in ["42.0%", "7.0%"] {
        assert!(
            texts.iter().any(|text| text == expected),
            "the per-engine utilization row must paint {expected}: {texts:?}"
        );
    }
    assert!(world.despawn(root), "the seeded page despawns cleanly");
}

#[test]
fn battery_block_renders_voltage_health_and_cycles() {
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::power::{
        BatteryInfo, BatteryScalarObservations, PowerSupplySnapshot,
    };
    use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

    let mut battery = BatteryInfo::new("BAT0", DeviceState::healthy(1_000));
    battery.model_name = "Primary Battery".to_owned();
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(85, 1),
        voltage_uv: ScalarObservation::available(11_400_000, 1), // 11.40 V
        power_w: ScalarObservation::available(15.5, 1),          // 15.5 W
        cycle_count: ScalarObservation::available(128, 1),
        energy_full_uwh: ScalarObservation::available(48_000_000.0, 1),
        energy_full_design_uwh: ScalarObservation::available(50_000_000.0, 1), // 96.0% health
        time_to_empty_secs: ScalarObservation::available(7200.0, 1),           // 2h 00m
        ..BatteryScalarObservations::default()
    });

    let mut shell = ShellApp::new();
    let snapshot = PowerSupplySnapshot {
        batteries: vec![battery.clone()],
        ..PowerSupplySnapshot::default()
    };
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::PowerSupplies(Some(snapshot)),
    );
    assert_eq!(
        section_keys(&shell, Section::Battery),
        vec!["BAT0".to_owned()]
    );

    let fact_line = super::metrics::battery_fact_line(&battery);
    assert!(fact_line.contains("85%"));
    assert!(fact_line.contains("15.5 W"));
    assert!(fact_line.contains("11.40 V"));
    assert!(fact_line.contains("128"));
    assert!(fact_line.contains("96.0%"));
    assert!(fact_line.contains(&duration(7200)));

    let mut app = headless_scene_app();
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell: &shell,

        palette: &palette,
        history: &history.0,
    };
    let world = app.world_mut();
    let root = world
        .spawn_scene(content(&context))
        .expect("the seeded performance page resolves")
        .id();

    assert_eq!(block_keys(world, Section::Battery), vec!["BAT0".to_owned()]);
    let field = DynField::Device {
        section: Section::Battery,
        device: "BAT0".to_owned(),
    };
    let line = dyn_text_value(world, &field).expect("battery has dynamic fact line");
    assert_eq!(line, fact_line);

    let texts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(texts.iter().any(|t| t == "Primary Battery"));

    assert!(world.despawn(root), "the seeded page despawns cleanly");
}

#[test]
fn performance_history_mount_starts_live_without_inventing_replay_availability() {
    let mut app = headless_scene_app();
    let shell = ShellApp::new();
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell: &shell,

        palette: &palette,
        history: &history.0,
    };
    let world = app.world_mut();
    let root = world
        .spawn_scene(content(&context))
        .expect("scene mounts")
        .id();

    let display = {
        let mut query = world.query::<(&PerformanceHistoryEntry, &Node)>();
        query
            .iter(world)
            .next()
            .map(|(_, node)| node.display)
            .unwrap_or(Display::None)
    };

    assert_eq!(display, Display::None);
    assert_eq!(world.query::<&PerformanceLiveBody>().iter(world).count(), 1);
    let mut replay = world.query::<(&PerformanceReplayRoot, &Node)>();
    assert_eq!(
        replay.single(world).expect("one review owner").1.display,
        Display::None
    );
    assert!(world.despawn(root));
}

#[test]
fn composition_bar_fractions_sum_to_one_and_zero_total_is_empty() {
    use super::tests::{GIB, memory_metrics};
    use crate::pages::performance::scene::blocks::memory::segment_bar_layout;
    use taskforest_core::core::metrics::MemoryMetrics;
    use taskmanager_shell::memory::memory_segments;

    let memory = memory_metrics(1, 4 * GIB, 16 * GIB, 12 * GIB, (GIB, 4 * GIB));
    let segments = memory_segments(&memory);
    let layout = segment_bar_layout(&segments);
    assert_eq!(layout.len(), segments.len(), "one span per segment");
    let total: f32 = layout.iter().map(|span| span.fraction).sum();
    assert!(
        (total - 1.0).abs() < 1e-4,
        "the spans tile the full width: {total}"
    );
    for span in &layout {
        assert!(
            span.fraction.is_finite() && span.fraction >= 0.0,
            "a span width is a real share, never NaN"
        );
    }

    // Nothing measured yet: an empty layout, never NaN widths.
    let zero = MemoryMetrics::default();
    assert!(segment_bar_layout(&memory_segments(&zero)).is_empty());
}

#[test]
fn composition_bar_rounds_only_its_outer_nonzero_segments() {
    use super::tests::{GIB, memory_metrics};
    use crate::pages::performance::scene::blocks::segment_bar_scene;

    let memory = memory_metrics(1, 4 * GIB, 16 * GIB, 12 * GIB, (GIB, 4 * GIB));
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = headless_scene_app();
    let world = app.world_mut();
    let root = world
        .spawn_scene(segment_bar_scene(&memory, &palette))
        .expect("composition bar mounts")
        .id();
    let segments = world.get::<Children>(root).expect("bar has segments");
    assert!(segments.len() >= 2);
    let first = world.get::<Node>(segments[0]).expect("first segment");
    let last = world
        .get::<Node>(segments[segments.len() - 1])
        .expect("last segment");
    assert!(matches!(first.border_radius.top_left.x, Val::Rem(r) if r > 0.0));
    assert_eq!(first.border_radius.top_right.x, Val::Px(0.0));
    assert_eq!(last.border_radius.bottom_left.x, Val::Px(0.0));
    assert!(matches!(last.border_radius.bottom_right.x, Val::Rem(r) if r > 0.0));
}

// ---- SMART self-test request ----------------------------------------------

// ---- the partition proportion appears once -------------------------------

/// The disk block and the storage surface are the SAME card: the storage
/// surface is the disk block's child. The partition's capacity therefore
/// appears exactly ONCE — the ring's readout. A second copy (the old
/// partition bar row) showed the same partition and the same numbers twice
/// in one card.
#[test]
fn the_partition_capacity_appears_once_in_the_disk_card() {
    let mut app = headless_perf_app();
    let mut shell = ShellApp::new();
    let mut disk = DiskMetrics::default();
    disk.device_id = "disk-0".to_owned();
    disk.name = "nvme0n1".to_owned();
    disk.model = "Example Disk".to_owned();
    let mut partition = DiskPartition::new("disk-0-part-1");
    partition.mount_point = "/".to_owned();
    partition.fs_type = "ext4".to_owned();
    partition.apply_scalar_observations(DiskPartitionScalarObservations {
        capacity_bytes: ScalarObservation::available(100, 1),
        used_bytes: ScalarObservation::available(62, 1),
        free_bytes: ScalarObservation::available(38, 1),
    });
    disk.partitions = vec![partition];
    let snapshot = SystemSnapshot {
        disks: vec![disk],
        ..Default::default()
    };
    edit_snapshot(&mut shell, |s| *s = Some(snapshot));
    route_to_performance(&mut app);
    for _ in 0..4 {
        app.update();
    }

    let world = app.world_mut();
    let readouts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .filter(|text| text.contains("(62%)"))
        .collect();
    assert!(
        readouts.is_empty(),
        "the disk block no longer repeats the partition the storage surface already states: {readouts:?}"
    );
}

// ---- the sidebar's bounded lines -----------------------------------------

/// The windowed headless composition: the real window plugin over a capture
/// frame, so the UI layout and the text pipeline run exactly as they do when
/// the frontend paints a window.
fn layout_shell_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    app.init_resource::<Assets<Font>>();
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
    let size = UVec2::new(1280, 720);
    app.world_mut().spawn((
        Window {
            resolution: (1280, 720).into(),
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
    app
}

/// The page mounted on a real frame: a bounded line is truncated against its
/// own SHAPED width, so the test needs the layout and the text pipeline, not
/// a character count.
fn layout_perf_app() -> App {
    let mut app = layout_shell_app();
    app.world_mut().non_send_mut::<FrontendTrack>().shell = demo_app();
    route_to_performance(&mut app);
    // The ellipsis converges over two passes: the first measures the whole
    // value, the second measures the ellipsis it just painted.
    for _ in 0..6 {
        app.update();
    }
    app
}

/// The slot a bounded line must fit: its parent's content box, in logical
/// pixels.
/// The slot a bounded line must fit: its parent's content box, in logical
/// pixels.
fn slot_width(node: &ComputedNode) -> f32 {
    node.content_box().width() * node.inverse_scale_factor
}

/// The shaped width of one bounded line, in logical pixels.
fn line_width(node: &ComputedNode) -> f32 {
    node.size().x * node.inverse_scale_factor
}

#[test]
fn no_bounded_sidebar_line_overflows_its_slot() {
    let mut app = layout_perf_app();
    let world = app.world_mut();
    let mut lines = world.query_filtered::<(&Text, &ComputedNode, &ChildOf), With<BoundedText>>();
    let mut slots = world.query::<&ComputedNode>();
    let mut bounded = 0;
    for (text, node, child_of) in lines.iter(world) {
        let parent = slots
            .get(world, child_of.parent())
            .expect("a bounded line belongs to a measured slot");
        let slot = slot_width(parent);
        let width = line_width(node);
        assert!(
            slot <= 0.0 || width <= slot + 1.0,
            "the bounded line {text:?} is measured to {width} px inside a {slot} px slot",
        );
        bounded += 1;
    }
    assert!(
        bounded > 8,
        "the sidebar's identity lines and captions are bounded slots: {bounded}"
    );
}

#[test]
fn a_brand_line_wider_than_the_sidebar_ends_with_an_ellipsis() {
    let mut app = layout_perf_app();
    let world = app.world_mut();
    let brand = world
        .query::<(&DynText, &Text)>()
        .iter(world)
        .find(|(marker, _)| marker.0 == DynField::SidebarCaption1(PerformanceDeviceTarget::Cpu))
        .map(|(_, text)| text.0.clone())
        .expect("the CPU row mounts its brand line");
    assert!(
        brand.ends_with(ELLIPSIS),
        "the demo brand is wider than the sidebar, so it ends in an ellipsis instead of a hard cut: {brand:?}"
    );
    assert!(
        brand.chars().count() < "Intel(R) Core(TM) Ultra 7 358H".chars().count(),
        "the value was shortened, not merely decorated: {brand:?}"
    );
}

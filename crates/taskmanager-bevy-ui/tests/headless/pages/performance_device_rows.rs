//! test-intent: behavior
//!
//! Performance-page device-block behavior tests: the disk caption's observed
//! I/O-depth and SMART evidence, the projected partition rows, and the
//! per-adapter GPU blocks. Split from `performance.rs` so each test file stays
//! inside the test source budget; the page's other tests stay there.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::{DiskMetrics, ScalarObservation};
use taskmanager_shell::ShellApp;
use taskmanager_theme::Theme;

use super::scene::content;
use super::tests::{block_keys, dyn_text_value};
use super::{DynField, Section, section_keys};
use crate::app::PageContext;
use crate::palette::ui_palette;
use taskmanager_shell::presentation::device_status_i18n_key;
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
    use taskmanager_core::core::metrics::DiskScalarObservations;

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
    use taskmanager_core::core::metrics::SmartAvailability;

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
    let mut missing_tool = DiskMetrics::default();
    missing_tool.smart_availability = SmartAvailability::MissingTool;
    assert!(
        !super::metrics::disk_caption(&missing_tool).contains(t("disk.smart_status")),
        "a hidden SMART section must not invent a status segment"
    );

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
    use taskmanager_core::core::metrics::{
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
    let palette = ui_palette(&Theme::dark());
    let history = crate::pages::history::HistoryProjectionResource::default();
    let process_tree_expansion = crate::pages::process_tree::ProcessTreeExpansion::default();
    let context = PageContext {
        shell: &shell,
        process_tree_expansion: &process_tree_expansion,
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
fn disk_block_renders_the_projected_partition_rows() {
    use taskmanager_core::core::metrics::{
        DiskPartition, DiskPartitionScalarObservations, SystemSnapshot,
    };
    use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

    const GIB: u64 = 1024 * 1024 * 1024;
    let partition = |name: &str, mount: &str, used: u64, capacity: u64| {
        let mut part = DiskPartition::new(name);
        part.mount_point = mount.to_owned();
        part.fs_type = "ext4".to_owned();
        part.apply_scalar_observations(DiskPartitionScalarObservations {
            capacity_bytes: ScalarObservation::available(capacity, 1),
            used_bytes: ScalarObservation::available(used, 1),
            ..DiskPartitionScalarObservations::default()
        });
        part
    };
    let mut disk = DiskMetrics::new("nvme0n1");
    disk.device_id = "disk:test:nvme0".to_owned();
    disk.model = "TiPro9000".to_owned();
    disk.partitions = vec![
        partition("nvme0n1p1", "/", 70 * GIB, 100 * GIB),
        partition("nvme0n1p2", "/home", 30 * GIB, 50 * GIB),
    ];

    let mut shell = ShellApp::new();
    let snapshot = SystemSnapshot {
        disks: vec![disk],
        ..SystemSnapshot::default()
    };
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    assert_eq!(section_keys(&shell, Section::Disk).len(), 1);

    // The mounted disk block paints one row per projected partition: its
    // identity and its own usage math.
    let mut app = headless_scene_app();
    let palette = ui_palette(&Theme::dark());
    let history = crate::pages::history::HistoryProjectionResource::default();
    let process_tree_expansion = crate::pages::process_tree::ProcessTreeExpansion::default();
    let context = PageContext {
        shell: &shell,
        process_tree_expansion: &process_tree_expansion,
        palette: &palette,
        history: &history.0,
    };
    let world = app.world_mut();
    let root = world
        .spawn_scene(content(&context))
        .expect("the seeded performance page resolves")
        .id();
    let texts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    for expected in [
        "/ (ext4)",
        "/home (ext4)",
        "70.0 GiB / 100.0 GiB (70%)",
        "30.0 GiB / 50.0 GiB (60%)",
    ] {
        assert!(
            texts.iter().any(|text| text == expected),
            "the partition row must paint {expected:?}: {texts:?}"
        );
    }
    assert!(world.despawn(root), "the seeded page despawns cleanly");
}

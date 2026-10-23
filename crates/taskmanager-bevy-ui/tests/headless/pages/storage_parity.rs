//! test-intent: behavior
//!
//! The storage surface's contract:
//!
//! - a partition ring reports the observed share, and a partition without a
//!   measured capacity reports no share at all;
//! - the SMART badge follows the shared effective status, and a missing tool
//!   is never a healthy disk;
//! - a missing SMART tool offers a copyable command;
//! - a read-only or erroring mount raises the alarm, a healthy mount does not;
//! - the surface slides open and shut through a typed motion state, never a
//!   sleep;
//! - the wear gauge follows the shared NVMe thresholds.

use super::*;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use std::path::PathBuf;
use taskmanager_application::i18n::t;
use taskmanager_core::core::FailureKind;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::{
    DiskPartition, DiskPartitionScalarObservations, ScalarObservation, SmartAvailability,
};
use taskmanager_core::core::storage_health::{
    FilesystemBackingKind, FilesystemHealth, FilesystemHealthSnapshot, FilesystemHealthStatus,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use crate::pages::performance::metrics::disk::SMART_TOOL_PROBE_COMMAND;
use crate::palette::{UiPalette, ui_palette};
use taskmanager_shell::presentation::missing_value;

/// A disk with one measured partition (62 of 100 units used).
fn disk_with_partition() -> DiskMetrics {
    let mut disk = DiskMetrics::new("sda");
    disk.device_id = "sda".to_owned();
    let mut partition = DiskPartition::new("sda1");
    partition.mount_point = "/".to_owned();
    partition.fs_type = "ext4".to_owned();
    partition.apply_scalar_observations(DiskPartitionScalarObservations {
        capacity_bytes: ScalarObservation::available(100, 1),
        used_bytes: ScalarObservation::available(62, 1),
        free_bytes: ScalarObservation::available(38, 1),
    });
    disk.partitions = vec![partition];
    disk
}

/// The palette the surfaces render with.
fn palette() -> UiPalette {
    ui_palette(&Theme::dark(), UiSize::Standard)
}

/// A bare scene world for the surface-assembly tests.
fn headless_scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

/// The refresh path rewrites the mounted readout in place, so it must state
/// the SAME share as the ring's arc — in percent. A readout that forgot the
/// percent scale printed "1%" for a 62% share while the ring drew 62%.
#[test]
fn the_refreshed_readout_states_the_observed_share_in_percent() {
    let disk = disk_with_partition();
    let partition = &disk.partitions[0];
    let readout = partition_readout(partition);
    assert!(
        readout.contains("62%"),
        "the refreshed readout states the observed share in percent: {readout}"
    );
    let models = partition_donut_models(&disk);
    assert!(
        models[0].usage_text.contains("62%"),
        "the mounted readout agrees with the refreshed one: {}",
        models[0].usage_text
    );
}

#[test]
fn the_partition_ring_reports_the_observed_share_and_never_a_zero_ring() {
    // A measured partition carries its observed share: the ring draws what
    // the filesystem reported.
    let disk = disk_with_partition();
    let models = partition_donut_models(&disk);
    assert_eq!(models.len(), 1, "one partition, one ring");
    let model = &models[0];
    assert_eq!(
        model.used_fraction,
        Some(0.62),
        "the ring carries the observed share"
    );
    assert!(
        model.usage_text.contains("62%"),
        "the readout carries the observed share: {}",
        model.usage_text
    );
    assert_eq!(model.key, "sda1", "the ring is keyed by its partition");

    // A partition without a measured capacity reports NO share: an empty
    // ring, never a zero-percent one.
    let mut cold = disk_with_partition();
    cold.partitions[0].apply_scalar_observations(DiskPartitionScalarObservations::unavailable(
        FailureKind::Unsupported,
    ));
    let cold_models = partition_donut_models(&cold);
    assert_eq!(
        cold_models[0].used_fraction, None,
        "an unmeasured partition has no share to draw"
    );
    assert_eq!(
        cold_models[0].usage_text,
        missing_value(),
        "an unmeasured partition reads the shared dash"
    );
}

#[test]
fn the_smart_badge_follows_the_shared_effective_status() {
    let mut disk = disk_with_partition();
    disk.smart_availability = SmartAvailability::Available;
    disk.smart_state = DeviceState::healthy(1);
    assert_eq!(
        smart_health_badge(&disk),
        SmartHealthBadge::Healthy,
        "a healthy provider wears the green plate"
    );
    assert_eq!(
        smart_health_label(&disk),
        t("device.healthy"),
        "the label is the shared device vocabulary"
    );

    // A stale provider cannot vouch for the device: the amber plate.
    disk.smart_state = DeviceState {
        status: DeviceStatus::Stale,
        last_success_ms: Some(1),
    };
    assert_eq!(
        smart_health_badge(&disk),
        SmartHealthBadge::Warning,
        "a stale provider wears the warning plate"
    );

    // A missing tool is an absence, not a healthy disk.
    disk.smart_availability = SmartAvailability::MissingTool;
    disk.smart_state = DeviceState {
        status: DeviceStatus::MissingTool,
        last_success_ms: None,
    };
    assert_eq!(
        smart_health_badge(&disk),
        SmartHealthBadge::Unavailable,
        "a missing tool wears no health plate at all"
    );
    assert_ne!(
        smart_health_label(&disk),
        t("device.healthy"),
        "a missing tool never reads as a healthy disk"
    );
}

#[test]
fn the_missing_smart_tool_offers_a_copyable_command() {
    let mut disk = disk_with_partition();
    disk.smart_availability = SmartAvailability::MissingTool;
    let guidance = smart_tool_guidance(&disk).expect("a missing tool carries guidance");
    assert_eq!(guidance.command, SMART_TOOL_PROBE_COMMAND);
    assert_eq!(guidance.title, t("disk.smart_missing_tool"));
    assert_eq!(guidance.hint, t("device.action_missing_tool"));

    // A healthy provider has nothing to install: no card, no command.
    disk.smart_availability = SmartAvailability::Available;
    assert!(
        smart_tool_guidance(&disk).is_none(),
        "a present tool never shows an install card"
    );

    // The mounted card carries the command for the copy action.
    disk.smart_availability = SmartAvailability::MissingTool;
    let mut app = headless_scene_app();
    let palette = palette();
    let world = app.world_mut();
    world
        .spawn_scene(storage_surface_scene(&disk, None, &palette, 7, 0.0))
        .expect("the storage surface resolves");
    let commands: Vec<String> = world
        .query::<&CopyCommand>()
        .iter(world)
        .map(|command| command.0.clone())
        .collect();
    assert_eq!(
        commands,
        vec![SMART_TOOL_PROBE_COMMAND.to_owned()],
        "the card's copy action carries the command"
    );
}

#[test]
fn the_read_only_and_erroring_mounts_raise_the_alarm() {
    let filesystem = FilesystemHealth {
        mount_point: PathBuf::from("/"),
        source: None,
        fs_type: "ext4".to_owned(),
        backing_kind: FilesystemBackingKind::PhysicalBlock,
        read_only: Some(true),
        error_count: Some(3),
        inode_used: None,
        inode_total: None,
        inode_usage_percent: None,
        status: FilesystemHealthStatus::ReadOnly,
        state: DeviceState::healthy(1),
        integrity_state: DeviceState::healthy(1),
    };
    let snapshot = FilesystemHealthSnapshot {
        state: DeviceState::healthy(1),
        filesystems: vec![filesystem.clone()],
    };
    let alerts = mount_alerts(Some(&snapshot), "/");
    assert!(
        alerts.contains(&MountAlert::ReadOnly),
        "a read-only mount raises the read-only alarm: {alerts:?}"
    );
    assert!(
        alerts.contains(&MountAlert::Errors),
        "a reported error count raises the error alarm: {alerts:?}"
    );

    // A healthy mount raises nothing.
    let healthy_filesystem = FilesystemHealth {
        read_only: Some(false),
        error_count: Some(0),
        status: FilesystemHealthStatus::Healthy,
        ..filesystem
    };
    let healthy = FilesystemHealthSnapshot {
        state: DeviceState::healthy(1),
        filesystems: vec![healthy_filesystem],
    };
    assert!(
        mount_alerts(Some(&healthy), "/").is_empty(),
        "a healthy mount raises no alarm"
    );

    // An unreported mount is an honest absence, not an invented alarm.
    assert!(
        mount_alerts(None, "/").is_empty(),
        "an unobserved mount claims no alarm"
    );
}

#[test]
fn the_storage_surface_slides_open_and_shut_through_a_typed_state() {
    // The motion is a typed state: the progress moves toward the target.
    let mut expand = StorageExpand {
        progress: 0.0,
        target: ExpandTarget::Expanding,
        natural_height: None,
    };
    advance_expand(&mut expand, EXPAND_SECONDS / 2.0);
    assert!(
        (expand.progress - 0.5).abs() < 0.001,
        "half the motion opens half the surface: {}",
        expand.progress
    );
    advance_expand(&mut expand, EXPAND_SECONDS);
    assert_eq!(expand.progress, 1.0, "the surface settles open");

    // The collapse is the same state with the other target.
    let mut closing = StorageExpand {
        progress: 1.0,
        target: ExpandTarget::Collapsing,
        natural_height: Some(120.0),
    };
    advance_expand(&mut closing, EXPAND_SECONDS / 2.0);
    assert!(
        (closing.progress - 0.5).abs() < 0.001,
        "half the motion closes half the surface: {}",
        closing.progress
    );
    advance_expand(&mut closing, EXPAND_SECONDS);
    assert_eq!(closing.progress, 0.0, "the surface settles shut");
}

#[test]
fn the_wear_gauge_follows_the_shared_nvme_thresholds() {
    let mut disk = disk_with_partition();
    disk.smart_percent_used = Some(2.0);
    disk.smart_temperature_c = Some(41.0);
    disk.smart_temp_critical_c = Some(70.0);
    let gauge = nvme_gauge_model(&disk).expect("the wear facts carry a gauge");
    assert_eq!(gauge.used_percent, Some(2.0));
    assert_eq!(gauge.temperature_c, Some(41.0));
    assert_eq!(gauge.critical_c, Some(70.0));
    assert_eq!(gauge.band, WearBand::Good);

    // The shared warning threshold (90% used) turns the band amber.
    disk.smart_percent_used = Some(90.0);
    assert_eq!(
        nvme_gauge_model(&disk).map(|gauge| gauge.band),
        Some(WearBand::Warning)
    );
    // The shared critical threshold (100% used) turns the band red.
    disk.smart_percent_used = Some(100.0);
    assert_eq!(
        nvme_gauge_model(&disk).map(|gauge| gauge.band),
        Some(WearBand::Critical)
    );
    // A temperature past the provider's critical threshold is critical too.
    disk.smart_percent_used = Some(2.0);
    disk.smart_temperature_c = Some(75.0);
    assert_eq!(
        nvme_gauge_model(&disk).map(|gauge| gauge.band),
        Some(WearBand::Critical)
    );
    // Without wear facts there is no gauge at all.
    assert!(
        nvme_gauge_model(&DiskMetrics::new("sdb")).is_none(),
        "a disk without wear facts draws no gauge"
    );
}

#[test]
fn the_mounted_surface_carries_one_ring_per_partition() {
    let disk = disk_with_partition();
    let palette = palette();
    let mut app = headless_scene_app();
    let world = app.world_mut();
    world
        .spawn_scene(storage_surface_scene(&disk, None, &palette, 7, 0.0))
        .expect("the storage surface resolves");

    let rings = world.query::<&StorageDonutArc>().iter(world).count();
    assert_eq!(rings, 1, "one partition carries one ring");
    let readouts = world.query::<&StorageDonutText>().iter(world).count();
    assert_eq!(readouts, 1, "one partition carries one readout");
    let texts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(
        texts.iter().any(|text| text.contains("62%")),
        "the ring's readout carries the observed share: {texts:?}"
    );
    // The surface starts collapsed and opens through its typed motion.
    let expand = world
        .query::<&StorageExpand>()
        .iter(world)
        .next()
        .copied()
        .expect("the surface carries its motion state");
    assert_eq!(expand.progress, 0.0);
    assert_eq!(expand.target, ExpandTarget::Expanding);
}

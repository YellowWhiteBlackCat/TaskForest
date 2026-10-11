use super::{AlertEngine, AlertMetric, AlertRule, AlertSeverity, AlertTargetIdentity};
use crate::core::device_state::{DeviceLifecycle, DevicePresence};
use crate::core::metrics::{DiskMetrics, SystemSnapshot};
use crate::core::storage::StorageIdentityStability;
use crate::core::{DeviceGeneration, DeviceId};
use std::collections::HashMap;
use std::time::Duration;

fn disk(
    device_id: &str,
    name: &str,
    generation: u64,
    stability: StorageIdentityStability,
    temperature: Option<f32>,
) -> DiskMetrics {
    let mut disk = DiskMetrics::new(name);
    disk.device_id = device_id.to_owned();
    disk.device_generation = DeviceGeneration::new(generation);
    disk.identity_stability = stability;
    disk.smart_temperature_c = temperature;
    disk
}

fn snapshot(at_ms: u64, disks: Vec<DiskMetrics>) -> SystemSnapshot {
    let device_lifecycles = disks
        .iter()
        .filter(|disk| !disk.device_id.trim().is_empty())
        .map(|disk| {
            (
                disk.device_id.trim().to_owned(),
                DeviceLifecycle {
                    presence: DevicePresence::Present,
                    generation: disk.device_generation,
                    ..DeviceLifecycle::default()
                },
            )
        })
        .collect();
    SystemSnapshot {
        timestamp_ms: at_ms,
        disks,
        device_lifecycles,
        ..SystemSnapshot::default()
    }
}

fn rule(id: &str, duration: u64, target: Option<&str>) -> AlertRule {
    let rule = AlertRule::new(
        id,
        AlertMetric::DiskTemperatureC,
        AlertSeverity::Warning,
        70.0,
        Duration::from_millis(duration),
        0.0,
    );
    match target {
        Some(target) => rule.for_target(target),
        None => rule,
    }
}

#[test]
fn system_and_storage_pair_ids_do_not_collide_on_delimiters() {
    let snap = snapshot(
        1,
        vec![
            disk(
                "c",
                "first",
                1,
                StorageIdentityStability::Persistent,
                Some(90.0),
            ),
            disk(
                "b:c",
                "second",
                1,
                StorageIdentityStability::Persistent,
                Some(90.0),
            ),
        ],
    );
    let alerts = AlertEngine::new([rule("a:b", 0, None), rule("a", 0, None)]).evaluate(&snap);
    assert_eq!(alerts.len(), 4);
    assert_ne!(alerts[0].instance_id, alerts[1].instance_id);
    assert_eq!(
        alerts
            .iter()
            .map(|alert| alert.instance_id.target().clone())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        2
    );
    assert!(alerts.iter().all(|alert| matches!(
        alert.instance_id.target(),
        AlertTargetIdentity::Storage { .. }
    )));
}

#[test]
fn generation_change_restarts_pending_duration_and_instance_identity() {
    let mut engine = AlertEngine::new([rule("disk-hot", 5_000, None)]);
    let generation_one = disk(
        "wwid-1",
        "Disk",
        1,
        StorageIdentityStability::Attachment,
        Some(90.0),
    );
    assert!(
        engine
            .evaluate(&snapshot(0, vec![generation_one]))
            .is_empty()
    );

    let generation_two = disk(
        "wwid-1",
        "Disk",
        2,
        StorageIdentityStability::Attachment,
        Some(90.0),
    );
    assert!(
        engine
            .evaluate(&snapshot(5_000, vec![generation_two.clone()]))
            .is_empty(),
        "a new lifecycle generation starts a new pending window"
    );
    assert!(
        engine
            .evaluate(&snapshot(9_999, vec![generation_two.clone()]))
            .is_empty()
    );
    let active = engine.evaluate(&snapshot(10_000, vec![generation_two]));
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].active_since_ms, 10_000);
    assert!(matches!(
        active[0].instance_id.target(),
        AlertTargetIdentity::Storage { generation, .. } if generation.get() == 2
    ));
}

#[test]
fn unknown_invalid_mismatched_or_duplicate_lifecycle_identity_never_falls_back() {
    let rule = rule("disk-hot", 0, None);
    let mut engine = AlertEngine::new([rule.clone()]);

    let unknown = disk(
        "wwid-1",
        "visible name",
        1,
        StorageIdentityStability::Unknown,
        Some(90.0),
    );
    assert!(engine.evaluate(&snapshot(1, vec![unknown])).is_empty());

    let invalid_generation = disk(
        "wwid-1",
        "visible name",
        0,
        StorageIdentityStability::Persistent,
        Some(90.0),
    );
    assert!(
        engine
            .evaluate(&snapshot(2, vec![invalid_generation]))
            .is_empty()
    );

    let mismatched = disk(
        "wwid-1",
        "visible name",
        1,
        StorageIdentityStability::Persistent,
        Some(90.0),
    );
    let mut wrong_lifecycle = snapshot(3, vec![mismatched.clone()]);
    wrong_lifecycle.device_lifecycles.insert(
        "wwid-1".into(),
        DeviceLifecycle {
            presence: DevicePresence::Present,
            generation: DeviceGeneration::new(2),
            ..DeviceLifecycle::default()
        },
    );
    assert!(engine.evaluate(&wrong_lifecycle).is_empty());
    wrong_lifecycle.device_lifecycles.insert(
        "wwid-1".into(),
        DeviceLifecycle {
            presence: DevicePresence::Absent,
            generation: DeviceGeneration::new(1),
            ..DeviceLifecycle::default()
        },
    );
    assert!(engine.evaluate(&wrong_lifecycle).is_empty());

    let duplicate = disk(
        "wwid-1",
        "second row",
        1,
        StorageIdentityStability::Persistent,
        Some(95.0),
    );
    assert!(
        engine
            .evaluate(&snapshot(4, vec![mismatched, duplicate]))
            .is_empty()
    );

    let no_owner = disk(
        "",
        "name fallback",
        1,
        StorageIdentityStability::Persistent,
        Some(99.0),
    );
    let mut no_owner_snapshot = snapshot(5, vec![no_owner]);
    no_owner_snapshot.device_lifecycles.insert(
        "name fallback".into(),
        DeviceLifecycle {
            presence: DevicePresence::Present,
            generation: DeviceGeneration::new(1),
            ..DeviceLifecycle::default()
        },
    );
    assert!(engine.evaluate(&no_owner_snapshot).is_empty());
}

#[test]
fn disk_name_filter_selects_only_a_valid_identity_and_never_creates_identity() {
    let disk_metrics = disk(
        "wwid-1",
        "System Disk",
        1,
        StorageIdentityStability::Persistent,
        Some(90.0),
    );
    let snap = snapshot(1, vec![disk_metrics]);
    let alerts = AlertEngine::new([rule("disk-hot", 0, Some("System Disk"))]).evaluate(&snap);
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].target, "System Disk");
    assert!(matches!(
        alerts[0].instance_id.target(),
        AlertTargetIdentity::Storage { device_id, .. } if device_id == &DeviceId::new("wwid-1")
    ));

    let fallback = disk(
        "",
        "System Disk",
        1,
        StorageIdentityStability::Unknown,
        Some(90.0),
    );
    assert!(
        AlertEngine::new([rule("disk-hot", 0, Some("System Disk"))])
            .evaluate(&snapshot(1, vec![fallback]))
            .is_empty()
    );
}

#[test]
fn trusted_lifecycle_lookup_is_exact_and_unique() {
    let mut snap = snapshot(
        1,
        vec![disk(
            "wwid-1",
            "Disk",
            1,
            StorageIdentityStability::Persistent,
            Some(90.0),
        )],
    );
    snap.device_lifecycles = HashMap::from([(
        "wwid-other".to_owned(),
        DeviceLifecycle {
            presence: DevicePresence::Present,
            generation: DeviceGeneration::new(1),
            ..DeviceLifecycle::default()
        },
    )]);
    assert!(
        AlertEngine::new([rule("disk-hot", 0, None)])
            .evaluate(&snap)
            .is_empty()
    );
}

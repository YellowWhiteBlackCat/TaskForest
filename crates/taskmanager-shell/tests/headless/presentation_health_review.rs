use super::*;
use crate::fixture::health::seed_shell_health;
use crate::fixture::{ProjectionSeedFact, demo_app, seed_projection_fact};
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::identity::DeviceGeneration;

#[test]
fn normal_health_fold_preserves_integrity_sensor_failure_and_report_identity() {
    set_language(Language::En);
    let mut app = demo_app();
    seed_shell_health(&mut app);
    let projection = app.projection();
    let (filesystems, _) = projection
        .storage_health_projection()
        .expect("normal storage fold");
    let snapshot = projection.snapshot.as_ref().expect("owned facts");
    let (reports, _) = projection.smart_projection();
    let groups = storage_groups(Some(filesystems), &snapshot.disks, reports.observations());
    assert_eq!(groups.len(), 4);
    assert!(groups[1].rows.iter().any(|row| row.value == "Read-only"));
    assert!(
        groups[2]
            .rows
            .iter()
            .any(|row| row.value == "Errors reported")
    );
    assert!(
        groups[2]
            .rows
            .iter()
            .any(|row| row.label == "Errors" && row.value == "3")
    );
    assert!(
        groups[0]
            .rows
            .iter()
            .any(|row| row.label == "Inodes" && row.value == "Unavailable")
    );
    let sensors = sensor_groups(projection.sensors.as_ref());
    assert!(sensors[1].rows.iter().any(|row| row.label == "Chassis fan"
        && row.value.contains("Unavailable")
        && row.value.contains("Permission denied")));
    assert!(sensors[2].rows[0].value.contains("42.75"));
    let mut replaced = snapshot.clone();
    replaced.disks[0].device_generation =
        DeviceGeneration::new(replaced.disks[0].device_generation.get() + 1);
    seed_projection_fact(
        &mut app,
        ProjectionSeedFact::Snapshot(Box::new(Some(replaced))),
    );
    let projection = app.projection();
    let (filesystems, _) = projection.storage_health_projection().expect("storage");
    let (reports, _) = projection.smart_projection();
    let groups = storage_groups(
        Some(filesystems),
        &projection.snapshot.as_ref().expect("snapshot").disks,
        reports.observations(),
    );
    assert_eq!(
        groups.len(),
        3,
        "a previous incarnation's self-test report cannot be displayed for a replacement"
    );
}

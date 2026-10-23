//! Headless proofs for the Disk page's bounded mount-point slot and its
//! damped dynamic throughput ceiling.

use std::time::{Duration, Instant};

use super::mount::{middle_ellipsize, partition_label_size};
use super::*;
use taskmanager_test_support::DiskPartitionFixtureBuilder;

/// The measured width of one partition identity run.
fn measured(text: &str) -> f32 {
    crate::text_metrics::measured_text_width(text, partition_label_size())
}

#[test]
fn a_long_mount_path_is_middle_ellipsized_inside_its_slot() {
    let mount = "/run/media/<user>/Some Very Long Volume Name";
    let elided = middle_ellipsize(mount, MOUNT_SLOT_WIDTH);
    assert!(
        elided.contains('\u{2026}'),
        "the middle of the path is elided: {elided}"
    );
    assert!(
        elided.starts_with("/run"),
        "the mount root survives the elision: {elided}"
    );
    assert!(
        elided.ends_with("Name"),
        "the volume name survives the elision: {elided}"
    );
    assert!(
        elided.len() < mount.len(),
        "the elided path is shorter than the original"
    );
    assert!(
        measured(&elided) <= MOUNT_SLOT_WIDTH,
        "the elided path fits its bounded slot: {elided}"
    );
}

#[test]
fn a_mount_path_that_fits_is_left_untouched() {
    assert_eq!(middle_ellipsize("/mnt/data", MOUNT_SLOT_WIDTH), "/mnt/data");
    assert_eq!(middle_ellipsize("", MOUNT_SLOT_WIDTH), "");
    // An unusable slot budget never fabricates a different path.
    assert_eq!(middle_ellipsize("/mnt/data", 0.0), "/mnt/data");
    assert_eq!(middle_ellipsize("/mnt/data", f32::NAN), "/mnt/data");
}

#[test]
fn the_partition_identity_keeps_the_volume_name_and_the_filesystem() {
    let partition = DiskPartitionFixtureBuilder::new()
        .name("nvme0n1p2".into())
        .mount_point("/run/media/<user>/Some Very Long Volume Name".into())
        .build();
    let label = partition_label(&partition, MOUNT_SLOT_WIDTH);
    assert!(
        label.contains("nvme0n1p2"),
        "the partition name survives: {label}"
    );
    assert!(
        label.contains('\u{2026}'),
        "the long mount path is middle-elided: {label}"
    );
    assert!(label.ends_with("Name"), "the volume name survives: {label}");
}

#[test]
fn the_disk_throughput_ceiling_eases_into_a_traffic_spike() {
    let key = "parity-test-disk";
    let start = Instant::now();
    // An idle disk keeps a zero ceiling.
    assert_eq!(range::damped_ceiling(key, 0.0, start), 0.0);
    // A spike starts the transition from the ceiling that is on screen.
    assert_eq!(
        range::damped_ceiling(key, 1000.0, start),
        0.0,
        "the transition starts at the ceiling on screen"
    );
    // Halfway through the 120 ms transition the ceiling has moved, but it
    // has not snapped.
    let middle = range::damped_ceiling(key, 1000.0, start + Duration::from_millis(60));
    assert!(
        middle > 0.0 && middle < 1000.0,
        "the ceiling must ease into the new scale: {middle}"
    );
    // The transition completes after the shared 120 ms duration.
    assert_eq!(
        range::damped_ceiling(key, 1000.0, start + Duration::from_millis(120)),
        1000.0
    );
    // A second disk keeps its own ceiling: the ledger is keyed by identity.
    assert_eq!(range::damped_ceiling("parity-test-disk-2", 0.0, start), 0.0);
}

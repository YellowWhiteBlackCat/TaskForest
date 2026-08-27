use std::fs;

use super::*;
use taskmanager_core::{StorageDeviceTarget, SystemObservationState};

fn fixture_root(name: &str) -> PathBuf {
    crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-storage-collector-{}-{name}",
        std::process::id()
    ))
}

#[test]
fn authoritative_empty_storage_is_current_and_not_a_guessed_failure() {
    let root = fixture_root("empty");
    fs::create_dir_all(&root).expect("create storage root");
    let mut collector = LinuxStorageTelemetryCollector::with_sysfs_root(root.clone());

    let observation = collector.observe(Instant::now(), 10);

    assert!(observation.current_value().is_some());
    assert!(matches!(
        observation.state(),
        SystemObservationState::Current { .. } | SystemObservationState::Partial { .. }
    ));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn unavailable_inventory_cannot_leave_a_mutation_target_authoritative() {
    let root = fixture_root("gone");
    fs::create_dir_all(&root).expect("create storage root");
    let mut collector = LinuxStorageTelemetryCollector::with_sysfs_root(root.clone());
    let resolver = collector.target_resolver();
    let _ = collector.observe(Instant::now(), 10);
    fs::remove_dir_all(&root).expect("remove fixture");

    let observation = collector.observe(Instant::now(), 20);

    assert!(matches!(
        observation.state(),
        SystemObservationState::Stale { .. } | SystemObservationState::Unavailable { .. }
    ));
    assert!(
        resolver.resolve(&StorageDeviceTarget::default()).is_err(),
        "failed discovery must fail closed instead of retaining a cached command locator"
    );
}

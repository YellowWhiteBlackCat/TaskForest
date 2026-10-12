use std::fs;

use super::*;
use taskmanager_core::core::smart::{SmartSelfTestCapabilities, SmartSelfTestSupport};
use taskmanager_core::{
    DeviceId, DiskMetrics, DiskSmart, ProviderId, SourceOutcome, SourceStatus, StorageDeviceTarget,
    SystemObservationState,
};

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

#[test]
fn reappeared_disk_generation_discards_cached_self_test_capability() {
    let id = "disk:wwid:fixture";
    let mut disk = DiskMetrics::new("nvme0n1");
    disk.device_id = id.to_owned();
    disk.smart_self_test_capabilities = SmartSelfTestCapabilities {
        short: SmartSelfTestSupport::Supported,
        ..Default::default()
    };
    let smart = DiskSmart {
        self_test_capabilities: disk.smart_self_test_capabilities,
        ..DiskSmart::default()
    };
    let mut state = DiskCollectionState::new();
    state.smart_cache.insert(id.to_owned(), smart);
    state.smart_source_cache.insert(
        id.to_owned(),
        SourceStatus {
            provider: ProviderId::borrowed("fixture.smart"),
            outcome: SourceOutcome::Available,
            item_count: 1,
        },
    );

    reset_reappeared_smart_facts(
        &mut state,
        std::slice::from_mut(&mut disk),
        &[DeviceId::new(id)],
    );

    assert_eq!(
        disk.smart_self_test_capabilities,
        SmartSelfTestCapabilities::default(),
        "a replacement generation begins with unknown test support"
    );
    assert!(!state.smart_cache.contains_key(id));
    assert!(!state.smart_source_cache.contains_key(id));
}

use super::*;
use taskmanager_core::DeviceGeneration;
use taskmanager_core::core::device_state::{DevicePresence, DeviceRefreshOutcome, DeviceStatus};

#[test]
fn fixture_add_stale_absent_readd_preserves_identity_and_unknowns() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    let mut disks = vec![
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:wwid:fixture".into())
            .device_state(DeviceState::healthy(10))
            .smart_temperature_c(Some(41.0))
            .build(),
    ];
    let first = reconcile_devices(
        &mut registry,
        &mut disks,
        DeviceRefreshOutcome::Complete,
        10,
    );
    assert!(first.newly_absent.is_empty());
    assert_eq!(
        registry
            .get("disk:wwid:fixture")
            .map(|lifecycle| lifecycle.generation),
        Some(DeviceGeneration::INITIAL)
    );
    assert_eq!(disks[0].device_generation.get(), 1);

    disks[0].device_state = DeviceState {
        status: DeviceStatus::Stale,
        last_success_ms: None,
    };
    disks[0].smart_temperature_c = None;
    reconcile_devices(
        &mut registry,
        &mut disks,
        DeviceRefreshOutcome::Complete,
        20,
    );
    assert_eq!(disks[0].device_state.last_success_ms, Some(10));
    assert_eq!(
        disks[0].smart_temperature_c, None,
        "unknown telemetry must remain absent, never a believable zero"
    );

    disks.clear();
    let removed = reconcile_devices(
        &mut registry,
        &mut disks,
        DeviceRefreshOutcome::Complete,
        30,
    );
    assert_eq!(
        removed.newly_absent.first().map(DeviceId::as_str),
        Some("disk:wwid:fixture")
    );
    assert!(
        disks.is_empty(),
        "confirmed-absent devices must not replay old measurements as live"
    );

    disks.push(
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:wwid:fixture".into())
            .device_state(DeviceState::healthy(40))
            .smart_temperature_c(Some(42.0))
            .build(),
    );
    let reappeared = reconcile_devices(
        &mut registry,
        &mut disks,
        DeviceRefreshOutcome::Complete,
        40,
    );
    assert_eq!(
        reappeared.reappeared.first().map(DeviceId::as_str),
        Some("disk:wwid:fixture")
    );
    let readded = registry
        .get("disk:wwid:fixture")
        .expect("stable identity should reconnect");
    assert_eq!(readded.generation, DeviceGeneration::new(2));
    assert_eq!(readded.state.last_success_ms, Some(40));
    assert_eq!(disks[0].device_generation.get(), 2);

    let mut generation_history =
        HashMap::from([("disk:wwid:fixture".to_owned(), "prior generation")]);
    prune_map(&mut generation_history, &reappeared.reappeared);
    assert!(
        generation_history.is_empty(),
        "re-add must not blend the prior hardware generation into new history"
    );
}

#[test]
fn network_readd_preserves_stable_identity_and_advances_generation() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    let mut networks = vec![
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id("net:mac:fixture".into())
            .device_state(DeviceState::healthy(10))
            .build(),
    ];

    reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        10,
    );
    assert_eq!(networks[0].device_generation.get(), 1);

    networks.clear();
    let removed = reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        20,
    );
    assert_eq!(
        removed.newly_absent.first().map(DeviceId::as_str),
        Some("net:mac:fixture")
    );

    networks.push(
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id("net:mac:fixture".into())
            .device_state(DeviceState::healthy(30))
            .build(),
    );
    let reappeared = reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        30,
    );
    assert_eq!(
        reappeared.reappeared.first().map(DeviceId::as_str),
        Some("net:mac:fixture")
    );
    assert_eq!(
        registry
            .get("net:mac:fixture")
            .map(|lifecycle| lifecycle.generation),
        Some(DeviceGeneration::new(2))
    );
    assert_eq!(networks[0].device_generation.get(), 2);
    assert_eq!(networks[0].device_state.last_success_ms, Some(30));
}

#[test]
fn provider_unavailable_and_ttl_history_cleanup_are_deterministic() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    let mut networks = vec![
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id("net:mac:aa:bb".into())
            .device_state(DeviceState::healthy(10))
            .build(),
    ];
    reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        10,
    );

    networks.clear();
    reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Unavailable(DeviceStatus::PermissionDenied),
        20,
    );
    let unavailable = registry
        .get("net:mac:aa:bb")
        .expect("unknown discovery outcome retains the device");
    assert_eq!(unavailable.presence, DevicePresence::Unavailable);
    assert_eq!(unavailable.state.last_success_ms, Some(10));

    reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        30,
    );
    let mut histories = HashMap::from([("net:mac:aa:bb".to_owned(), "retained")]);
    let boundary = reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        130,
    );
    prune_map(&mut histories, &boundary.expired);
    assert_eq!(histories.len(), 1, "exact TTL boundary remains retained");

    let expired = reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        131,
    );
    prune_map(&mut histories, &expired.expired);
    assert_eq!(
        expired.expired.first().map(DeviceId::as_str),
        Some("net:mac:aa:bb")
    );
    assert!(histories.is_empty());
}

#[test]
fn retained_rows_are_not_reobserved_as_present_after_discovery_failure() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    let mut networks = ["net:mac:aa", "net:mac:bb"]
        .into_iter()
        .map(|device_id| {
            taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                .device_id(device_id.into())
                .device_state(DeviceState::healthy(10))
                .build()
        })
        .collect::<Vec<_>>();
    reconcile_devices(
        &mut registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        10,
    );

    let discovered = vec![DeviceId::new("net:mac:aa")];
    reconcile_discovered_devices(
        &mut registry,
        &mut networks,
        &discovered,
        DeviceRefreshOutcome::Unavailable(DeviceStatus::PermissionDenied),
        20,
    );

    assert_eq!(
        registry
            .get("net:mac:aa")
            .map(|lifecycle| lifecycle.presence),
        Some(DevicePresence::Present)
    );
    assert_eq!(
        registry
            .get("net:mac:bb")
            .map(|lifecycle| lifecycle.presence),
        Some(DevicePresence::Unavailable)
    );
    assert_eq!(
        networks[1].device_state.status,
        DeviceStatus::PermissionDenied
    );
    assert_eq!(networks[1].device_state.last_success_ms, Some(10));
}

#[test]
fn disk_network_and_gpu_share_one_reconciliation_contract() {
    let mut network_registry = DeviceLifecycleRegistry::new(1);
    let mut networks = vec![
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id("net:mac:01".into())
            .device_state(DeviceState::healthy(5))
            .build(),
    ];
    reconcile_devices(
        &mut network_registry,
        &mut networks,
        DeviceRefreshOutcome::Complete,
        5,
    );

    let mut gpu_registry = DeviceLifecycleRegistry::new(1);
    let mut gpu = GpuMetrics::new("gpu:pci:0000:01:00.0", "Fixture GPU");
    gpu.device_state = DeviceState::healthy(5);
    let mut gpus = vec![gpu];
    reconcile_devices(
        &mut gpu_registry,
        &mut gpus,
        DeviceRefreshOutcome::Complete,
        5,
    );

    assert_eq!(
        network_registry
            .get("net:mac:01")
            .map(|lifecycle| lifecycle.generation),
        Some(DeviceGeneration::INITIAL)
    );
    assert_eq!(
        gpu_registry
            .get("gpu:pci:0000:01:00.0")
            .map(|lifecycle| lifecycle.generation),
        Some(DeviceGeneration::INITIAL)
    );
    assert_eq!(networks[0].device_generation.get(), 1);
    assert_eq!(gpus[0].device_generation.get(), 1);
    assert_eq!(gpus[0].current_temperature_c(), None);
}

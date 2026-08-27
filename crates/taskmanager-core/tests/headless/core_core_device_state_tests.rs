use super::*;

#[test]
fn discovery_source_outcome_controls_lifecycle_authority() {
    assert_eq!(
        DeviceRefreshOutcome::from_discovery_outcome(SourceOutcome::Empty),
        DeviceRefreshOutcome::Complete
    );
    assert_eq!(
        DeviceRefreshOutcome::from_discovery_outcome(SourceOutcome::Partial(
            FailureKind::PermissionDenied
        )),
        DeviceRefreshOutcome::Unavailable(DeviceStatus::PermissionDenied)
    );
}

#[test]
fn state_failure_preserves_last_success_and_recovery_refreshes_it() {
    let healthy = DeviceState::healthy(100);
    let stale = healthy.transition(DeviceStatus::Stale, 200);
    assert_eq!(stale.status, DeviceStatus::Stale);
    assert_eq!(stale.last_success_ms, Some(100));
    let denied = stale.transition(DeviceStatus::PermissionDenied, 300);
    assert_eq!(denied.last_success_ms, Some(100));
    let recovered = denied.transition(DeviceStatus::Healthy, 400);
    assert_eq!(recovered, DeviceState::healthy(400));
}

#[test]
fn state_success_timestamp_never_moves_backwards() {
    let newer = DeviceState::healthy(500);
    assert_eq!(
        newer.transition(DeviceStatus::Healthy, 400),
        DeviceState::healthy(500)
    );
    let observed = DeviceState {
        status: DeviceStatus::Stale,
        last_success_ms: Some(450),
    };
    assert_eq!(
        newer.merge_observation(observed, 300),
        DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: Some(500),
        }
    );
}

#[test]
fn lifecycle_add_stale_absent_readd_advances_only_confirmed_generation() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    registry.begin_refresh();
    let first = registry.observe("disk:wwid:alpha", DeviceState::healthy(10), 10);
    assert_eq!(first.presence, DevicePresence::Present);
    assert_eq!(first.generation, 1);

    registry.begin_refresh();
    let stale = registry.observe(
        "disk:wwid:alpha",
        DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: None,
        },
        20,
    );
    assert_eq!(stale.state.last_success_ms, Some(10));
    assert!(
        registry
            .finish_refresh(DeviceRefreshOutcome::Complete, 20)
            .newly_absent
            .is_empty()
    );

    registry.begin_refresh();
    let removed = registry.finish_refresh(DeviceRefreshOutcome::Complete, 30);
    assert_eq!(
        removed.newly_absent.first().map(DeviceId::as_str),
        Some("disk:wwid:alpha")
    );
    let absent = registry
        .get("disk:wwid:alpha")
        .expect("absence is retained through grace");
    assert_eq!(absent.presence, DevicePresence::Absent);
    assert_eq!(absent.state.last_success_ms, Some(10));

    registry.begin_refresh();
    let readded = registry.observe("disk:wwid:alpha", DeviceState::healthy(40), 40);
    assert_eq!(readded.presence, DevicePresence::Present);
    assert_eq!(readded.generation, 2);
    assert_eq!(readded.state.last_success_ms, Some(40));
    let recovered = registry.finish_refresh(DeviceRefreshOutcome::Complete, 40);
    assert_eq!(
        recovered.reappeared.first().map(DeviceId::as_str),
        Some("disk:wwid:alpha")
    );
}

#[test]
fn unavailable_refresh_is_not_absence_and_does_not_advance_generation() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    registry.begin_refresh();
    registry.observe("gpu:pci:0000:01:00.0", DeviceState::healthy(10), 10);
    registry.finish_refresh(DeviceRefreshOutcome::Complete, 10);

    registry.begin_refresh();
    let delta = registry.finish_refresh(
        DeviceRefreshOutcome::Unavailable(DeviceStatus::PermissionDenied),
        20,
    );
    assert!(delta.newly_absent.is_empty());
    let unavailable = registry
        .get("gpu:pci:0000:01:00.0")
        .expect("provider outage retains identity");
    assert_eq!(unavailable.presence, DevicePresence::Unavailable);
    assert_eq!(unavailable.state.status, DeviceStatus::PermissionDenied);
    assert_eq!(unavailable.state.last_success_ms, Some(10));

    registry.begin_refresh();
    let recovered = registry.observe("gpu:pci:0000:01:00.0", DeviceState::healthy(30), 30);
    assert_eq!(recovered.generation, 1);
}

#[test]
fn absence_ttl_keeps_exact_boundary_then_prunes() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    registry.begin_refresh();
    registry.observe("net:mac:aa", DeviceState::healthy(10), 10);
    registry.finish_refresh(DeviceRefreshOutcome::Complete, 10);

    registry.begin_refresh();
    registry.finish_refresh(DeviceRefreshOutcome::Complete, 20);
    registry.begin_refresh();
    let boundary = registry.finish_refresh(DeviceRefreshOutcome::Complete, 120);
    assert!(boundary.expired.is_empty());
    assert_eq!(registry.len(), 1);

    registry.begin_refresh();
    let expired = registry.finish_refresh(DeviceRefreshOutcome::Complete, 121);
    assert_eq!(
        expired.expired.first().map(DeviceId::as_str),
        Some("net:mac:aa")
    );
    assert!(registry.is_empty());
}

#[test]
fn stable_selection_resolves_after_remove_and_readd_at_new_index() {
    let mut selection = StableDeviceSelection::default();
    selection.select("disk:wwid:alpha");
    assert_eq!(
        selection.resolve(["disk:wwid:alpha", "disk:wwid:beta"]),
        Some(0)
    );
    assert_eq!(selection.resolve(["disk:wwid:beta"]), None);
    assert_eq!(
        selection.resolve(["disk:wwid:beta", "disk:wwid:alpha"]),
        Some(1)
    );
}

#[test]
fn hardware_identifiers_survive_kernel_name_changes() {
    assert_eq!(
        stable_disk_id("sda", Some("naa.5000-AB"), None),
        stable_disk_id("sdc", Some("naa.5000-AB"), None)
    );
    assert_eq!(
        stable_network_id("eth0", Some("AA:BB:CC:DD:EE:FF")),
        stable_network_id("enp4s0", Some("aa:bb:cc:dd:ee:ff"))
    );
    assert_eq!(
        stable_gpu_id("card0", Some("0000:00:02.0")),
        stable_gpu_id("card2", Some("0000:00:02.0"))
    );
}

#[test]
fn identity_growth_is_capped_by_evicting_the_least_recently_seen() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    registry.begin_refresh();
    for index in 0..MAX_TRACKED_DEVICE_IDENTITIES {
        registry.observe(
            format!("net:mac:{index:05}"),
            DeviceState::healthy(index as u64),
            index as u64,
        );
    }
    assert_eq!(registry.len(), MAX_TRACKED_DEVICE_IDENTITIES);

    // A churned identity beyond the ceiling cannot grow the registry: the
    // least recently seen entry (index 0) is forgotten instead.
    registry.begin_refresh();
    let newcomer = registry.observe("net:mac:dead", DeviceState::healthy(9_000), 9_000);
    assert_eq!(newcomer.generation, 1);
    assert_eq!(
        registry.len(),
        MAX_TRACKED_DEVICE_IDENTITIES,
        "the registry must stay saturated, never grow past the ceiling"
    );
    assert!(
        registry.get("net:mac:00000").is_none(),
        "the least recently seen identity must be the eviction victim"
    );
    assert!(registry.get("net:mac:dead").is_some());
    assert!(registry.get("net:mac:01023").is_some());
}

#[test]
fn a_saturated_registry_evicts_confirmed_absent_identities_first() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    registry.begin_refresh();
    for index in 0..MAX_TRACKED_DEVICE_IDENTITIES {
        registry.observe(
            format!("net:mac:{index:05}"),
            DeviceState::healthy(index as u64),
            index as u64,
        );
    }
    // Identity 0 goes absent; every other identity is seen again later, so
    // the absent one is NOT the least recently seen entry.
    registry.begin_refresh();
    for index in 1..MAX_TRACKED_DEVICE_IDENTITIES {
        registry.observe(
            format!("net:mac:{index:05}"),
            DeviceState::healthy(5_000 + index as u64),
            5_000 + index as u64,
        );
    }
    let delta = registry.finish_refresh(DeviceRefreshOutcome::Complete, 5_500);
    assert!(
        delta
            .newly_absent
            .iter()
            .any(|id| id.as_str() == "net:mac:00000"),
        "identity 0 must be confirmed absent before the eviction"
    );

    registry.begin_refresh();
    registry.observe("net:mac:churned", DeviceState::healthy(6_000), 6_000);
    assert_eq!(registry.len(), MAX_TRACKED_DEVICE_IDENTITIES);
    assert!(
        registry.get("net:mac:00000").is_none(),
        "a confirmed-absent identity must be evicted before any present one"
    );
    assert!(
        registry.get("net:mac:01023").is_some(),
        "a present identity survives the capacity eviction"
    );
    assert!(registry.get("net:mac:churned").is_some());
}

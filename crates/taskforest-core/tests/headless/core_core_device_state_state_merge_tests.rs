use super::{
    DeviceLifecycleRegistry, DevicePresence, DeviceRefreshOutcome, DeviceState, DeviceStatus,
};

fn state(status: DeviceStatus, last_success_ms: Option<u64>) -> DeviceState {
    DeviceState {
        status,
        last_success_ms,
    }
}

#[test]
fn merge_never_moves_the_success_marker_backwards() {
    // now_ms regressing behind the previous success must keep the
    // previous marker (a `previous > now_ms` guard mutation flips this).
    let merged = state(DeviceStatus::Healthy, Some(100))
        .merge_observation(state(DeviceStatus::Healthy, None), 50);
    assert_eq!(
        merged.last_success_ms,
        Some(100),
        "a regressing clock must not move the success marker"
    );
}

#[test]
fn merge_advances_the_marker_with_the_clock() {
    // previous(50) < now(100): the marker must move FORWARD to 100 (a
    // `previous > now_ms → true` guard mutation would pin the old 50).
    let merged = state(DeviceStatus::Healthy, Some(50))
        .merge_observation(state(DeviceStatus::Healthy, None), 100);
    assert_eq!(
        merged.last_success_ms,
        Some(100),
        "a forward clock must advance the success marker"
    );
}

#[test]
fn transition_with_regressing_clock_keeps_the_later_marker() {
    let transitioned =
        state(DeviceStatus::Healthy, Some(100)).transition(DeviceStatus::Healthy, 50);
    assert_eq!(transitioned.last_success_ms, Some(100));
}

#[test]
fn registry_counts_and_emptiness_reflect_observed_devices() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    registry.begin_refresh();
    registry.observe("disk:wwid:abc", state(DeviceStatus::Healthy, Some(10)), 10);
    registry.finish_refresh(DeviceRefreshOutcome::Complete, 10);

    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 1);

    registry.observe("disk:wwid:def", state(DeviceStatus::Healthy, Some(20)), 20);
    assert_eq!(registry.len(), 2);
}

#[test]
fn begin_refresh_resets_observation_set_for_the_next_round() {
    let mut registry = DeviceLifecycleRegistry::new(100);
    registry.begin_refresh();
    registry.observe("disk:wwid:abc", state(DeviceStatus::Healthy, Some(10)), 10);
    let delta = registry.finish_refresh(DeviceRefreshOutcome::Complete, 10);
    assert!(delta.newly_absent.is_empty());

    // A second round that observes nothing must report the device absent
    // — begin_refresh must have cleared the previous round's observations
    // (a `begin_refresh → ()` mutation keeps them and suppresses absence).
    registry.begin_refresh();
    let delta = registry.finish_refresh(DeviceRefreshOutcome::Complete, 20);
    assert_eq!(
        delta
            .newly_absent
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        vec!["disk:wwid:abc"],
        "an unobserved device is newly absent in the next round"
    );
    assert_eq!(
        registry.get("disk:wwid:abc").map(|l| l.presence),
        Some(DevicePresence::Absent)
    );
}

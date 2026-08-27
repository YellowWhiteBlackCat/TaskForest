use taskmanager_application::{
    DeviceGeneration, DeviceId, SmartSelfTestObservation, StorageDeviceKey,
};
use taskmanager_core::{SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport};

use super::{SharedSmartRuntimeState, SmartCommitStatus, SmartRuntimeState};

impl SharedSmartRuntimeState {
    pub(crate) fn with_job_limit(retention_ms: u64, max_jobs: usize) -> Self {
        let mut state = SmartRuntimeState::new(retention_ms);
        state.max_jobs = max_jobs;
        Self(Mutex::new(state))
    }
}

fn observation(device: &str, generation: u64, locator: &str) -> SmartSelfTestObservation {
    SmartSelfTestObservation {
        device_id: DeviceId::new(format!("disk:{device}")),
        device_generation: DeviceGeneration::new(generation),
        device_key: StorageDeviceKey::new(locator),
        display_name: device.into(),
        report: SmartSelfTestReport {
            phase: SmartSelfTestPhase::Running,
            kind: Some(SmartSelfTestKind::Short),
            ..SmartSelfTestReport::default()
        },
    }
}

#[test]
fn independent_targets_coexist_and_commit_without_cross_device_overwrite() {
    let mut state = SmartRuntimeState::new(100);
    let disk_a = state
        .install_started(observation("a", 1, "sda"), 1)
        .expect("disk A job");
    let disk_b = state
        .install_started(observation("b", 1, "sdb"), 2)
        .expect("disk B job");
    let mut refreshed_a = disk_a.installed.observation.clone();
    refreshed_a.report.phase = SmartSelfTestPhase::Completed;

    assert_eq!(
        state
            .commit_observation(&disk_a.installed.token, refreshed_a, 3)
            .expect("commit disk A"),
        SmartCommitStatus::Applied
    );
    let snapshot = state.snapshot();
    assert_eq!(
        snapshot.revision,
        taskmanager_application::SmartStateRevision::new(3)
    );
    assert_eq!(snapshot.jobs.len(), 2);
    assert_eq!(
        snapshot
            .jobs
            .iter()
            .find(|job| job.observation.device_key.as_str() == "sdb")
            .map(|job| job.observation.report.phase),
        Some(SmartSelfTestPhase::Running)
    );
    assert_ne!(
        disk_a.installed.token.job_generation,
        disk_b.installed.token.job_generation
    );
}

#[test]
fn restarting_one_target_invalidates_only_that_targets_inflight_poll() {
    let mut state = SmartRuntimeState::new(100);
    let old_a = state
        .install_started(observation("a", 1, "sda"), 1)
        .expect("old disk A job");
    let disk_b = state
        .install_started(observation("b", 1, "sdb"), 1)
        .expect("disk B job");
    let new_a = state
        .install_started(observation("a", 1, "sda"), 2)
        .expect("new disk A job");

    assert_eq!(
        state
            .commit_observation(&old_a.installed.token, old_a.installed.observation, 3)
            .expect("stale commit is a typed no-op"),
        SmartCommitStatus::Superseded
    );
    assert!(state.contains(&new_a.installed.token));
    assert!(state.contains(&disk_b.installed.token));
    let snapshot = state.snapshot();
    assert_eq!(snapshot.jobs.len(), 2);
    assert_eq!(
        snapshot.revision,
        taskmanager_application::SmartStateRevision::new(3),
        "superseded commit must not advance the authoritative projection"
    );
}

#[test]
fn reappeared_device_generation_retires_old_generation_without_touching_other_disks() {
    let mut state = SmartRuntimeState::new(100);
    state
        .install_started(observation("a", 1, "sda"), 1)
        .expect("old disk A generation");
    let disk_b = state
        .install_started(observation("b", 4, "sdb"), 1)
        .expect("disk B job");
    let reappeared = state
        .install_started(observation("a", 2, "sdc"), 2)
        .expect("new disk A generation");

    assert_eq!(reappeared.retired.len(), 1);
    assert_eq!(reappeared.retired[0].device_generation.get(), 1);
    assert!(state.contains(&disk_b.installed.token));
    assert_eq!(state.snapshot().jobs.len(), 2);
}

#[test]
fn cancellation_and_expiry_make_inflight_commits_stale_without_aba_revival() {
    let mut state = SmartRuntimeState::new(10);
    let canceled = state
        .install_started(observation("a", 1, "sda"), 1)
        .expect("cancel fixture");
    let target = canceled.installed.observation.target();
    assert!(
        state
            .stop_tracking(&target)
            .expect("stop tracking")
            .is_some()
    );
    assert_eq!(
        state
            .commit_observation(&canceled.installed.token, canceled.installed.observation, 2)
            .expect("canceled commit is a typed no-op"),
        SmartCommitStatus::Superseded
    );

    let expired = state
        .install_started(observation("a", 1, "sda"), 3)
        .expect("expiry fixture");
    assert_eq!(state.prune_expired(14).expect("prune expired").len(), 1);
    let restarted = state
        .install_started(observation("a", 1, "sda"), 15)
        .expect("restart after expiry");
    assert_ne!(
        expired.installed.token.job_generation,
        restarted.installed.token.job_generation
    );
    assert_eq!(
        state
            .commit_observation(&expired.installed.token, expired.installed.observation, 16)
            .expect("expired commit is a typed no-op"),
        SmartCommitStatus::Superseded
    );
    assert!(state.contains(&restarted.installed.token));
}

#[test]
fn native_locator_is_validated_but_never_part_of_the_target_map_key() {
    let mut state = SmartRuntimeState::new(100);
    let started = state
        .install_started(observation("a", 7, "sda"), 1)
        .expect("locator fixture");
    let mut stale_locator = started.installed.observation.target();
    stale_locator.locator = StorageDeviceKey::new("sdb");

    assert!(state.snapshot_target(&stale_locator).is_none());
    assert!(
        state
            .stop_tracking(&stale_locator)
            .expect("locator mismatch is a typed no-op")
            .is_none()
    );
    assert!(state.contains(&started.installed.token));
}

#[test]
fn exhausted_job_generation_fails_closed_without_retiring_current_jobs() {
    let mut state = SmartRuntimeState::new(100);
    let current = state
        .install_started(observation("a", 1, "sda"), 1)
        .expect("current job");
    state.next_job_generation = u64::MAX;

    let error = state
        .install_started(observation("a", 1, "sda"), 2)
        .expect_err("generation exhaustion must reject installation");

    assert_eq!(
        error,
        taskmanager_application::ProviderFailure::ProviderFault
    );
    assert!(state.contains(&current.installed.token));
    assert_eq!(state.snapshot().jobs.len(), 1);
}

#[test]
fn exhausted_projection_revision_rejects_every_mutation_without_partial_state_change() {
    let mut state = SmartRuntimeState::new(10);
    let current = state
        .install_started(observation("a", 1, "sda"), 1)
        .expect("current job");
    let original = state.snapshot();
    state.revision = taskmanager_application::SmartStateRevision::new(u64::MAX);
    let saturated = state.snapshot();
    let mut refreshed = current.installed.observation.clone();
    refreshed.report.phase = SmartSelfTestPhase::Completed;

    assert_eq!(
        state.commit_observation(&current.installed.token, refreshed, 2),
        Err(taskmanager_application::ProviderFailure::ProviderFault)
    );
    assert_eq!(
        state.stop_tracking(&current.installed.observation.target()),
        Err(taskmanager_application::ProviderFailure::ProviderFault)
    );
    assert_eq!(
        state.prune_expired(100),
        Err(taskmanager_application::ProviderFailure::ProviderFault)
    );
    assert_eq!(
        state.install_started(observation("a", 2, "sdb"), 3),
        Err(taskmanager_application::ProviderFailure::ProviderFault)
    );
    assert_eq!(state.snapshot(), saturated);
    assert_eq!(state.snapshot().jobs, original.jobs);
}

#[test]
fn tracked_job_limit_rejects_new_identity_but_allows_same_device_replacement() {
    let mut state = SmartRuntimeState::new(100);
    state.max_jobs = 2;
    state
        .install_started(observation("a", 1, "sda"), 1)
        .expect("first tracked job");
    state
        .install_started(observation("b", 1, "sdb"), 1)
        .expect("second tracked job");

    assert_eq!(
        state.install_started(observation("c", 1, "sdc"), 2),
        Err(taskmanager_application::ProviderFailure::Rejected)
    );
    assert_eq!(state.snapshot().jobs.len(), 2);

    let replacement = state
        .install_started(observation("a", 2, "sda2"), 3)
        .expect("one device may replace its own retained generation at the limit");
    assert_eq!(replacement.retired.len(), 1);
    assert_eq!(state.snapshot().jobs.len(), 2);
}
use std::sync::Mutex;

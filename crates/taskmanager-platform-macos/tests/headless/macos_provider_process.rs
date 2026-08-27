use super::*;
use taskmanager_core::ProcessBatchAction;
use taskmanager_platform_provider::{
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessGpuProvider,
    ProcessIsolationProvider, ProcessNetworkProvider, ProcessOpenFilesProvider,
    ProcessResourceControlProvider, ProcessResourcesProvider,
};

fn frozen(pid: u32) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, "test", 1, 1).expect("valid test identity")
}

#[test]
fn weak_start_marker_still_resets_process_disk_rate_on_observed_replacement() {
    let mut rates = MacProcessDiskRateState::default();
    let first = rates.observe(10, 4_096, 8_192, 100);
    assert_eq!(
        first.0,
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
    );
    let current = rates.observe(10, 6_144, 10_240, 2_100);
    assert_eq!(current.0, ScalarObservation::available(1_024, 2_100));
    let replaced = rates.observe(11, 1, 1, 3_100);
    assert_eq!(
        replaced.0,
        ScalarObservation::unavailable(FailureKind::IdentityChanged)
    );
}

#[test]
fn pending_providers_complete_with_typed_unsupported() {
    let mut network = PendingProcessNetworkProvider;
    assert_eq!(
        network.observe(&frozen(1), 1),
        Err(ProviderFailure::Unsupported)
    );
    let mut gpu = PendingProcessGpuProvider;
    assert_eq!(
        gpu.observe(&frozen(1), 1),
        Err(ProviderFailure::Unsupported)
    );
    let mut isolation = PendingProcessIsolationProvider;
    assert_eq!(
        isolation.observe(&frozen(1), 1),
        Err(ProviderFailure::Unsupported)
    );
    let mut open_files = PendingProcessOpenFilesProvider;
    assert_eq!(
        open_files.observe(&frozen(1), 1),
        Err(ProviderFailure::Unsupported)
    );
    let mut affinity = PendingProcessAffinityProvider;
    assert_eq!(
        affinity.affinity(&frozen(1)),
        Err(ProviderFailure::Unsupported)
    );
    let mut affinity_control = PendingProcessAffinityControlProvider;
    assert_eq!(
        affinity_control.set_affinity(&frozen(1), &[0]),
        Err(ProviderFailure::Unsupported)
    );
    let mut resource_control = PendingProcessResourceControlProvider;
    assert_eq!(
        resource_control.apply_limits(
            &frozen(1),
            &taskmanager_core::ResourceGroupLimitRequest::default()
        ),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
fn unsupported_signal_semantics_complete_without_panicking() {
    // macOS has no precise safe creation-token boundary yet, so even a
    // syntactically authoritative target must fail before any POSIX signal.
    let mut provider = MacProcessControlProvider::new();
    let result = provider.send_signal(&frozen(4_000_000), ProcessSignal::Terminate);
    assert_eq!(result, Err(ProviderFailure::Unsupported));

    let mut resources = MacProcessResourcesProvider::new();
    assert_eq!(
        resources.observe(&frozen(std::process::id()), 1),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
fn injected_facts_cache_promotes_current_process_nice_and_threads_to_available() {
    // The test process always appears in its own sysinfo process list, so
    // keying the injected cache on std::process::id() is host-independent.
    // A fresh cache (just constructed) is reused by `fresh` without
    // shelling out, so this never depends on a real `ps`.
    let me = std::process::id();
    let mut provider = MacProcessListProvider::new();
    provider.process_facts =
        ProcessFactsCache::with_map(HashMap::from([(me, (Some(5), Some(7)))]), Instant::now());
    let snapshot = provider.refresh(1_000).expect("process list must refresh");
    let row = snapshot
        .items
        .iter()
        .find(|item| item.pid == me)
        .expect("the test process must appear in its own process list");
    // Cache hit -> both scalars promoted to Available with the observed
    // timestamp; the legacy row fields are derived by apply_scalar_observations.
    assert_eq!(
        row.scalar_observations().nice,
        ScalarObservation::available(5, 1_000)
    );
    assert_eq!(
        row.scalar_observations().threads,
        ScalarObservation::available(7, 1_000)
    );
    assert_eq!(row.current_nice(), Some(5));
    assert_eq!(row.current_threads(), Some(7));
}

#[test]
fn empty_facts_cache_keeps_nice_and_threads_unavailable() {
    // A fresh-but-empty cache (no rows for any PID) keeps both scalars
    // honestly Unsupported — the provider never fabricates a 0 nice or
    // thread count on a cache miss.
    let me = std::process::id();
    let mut provider = MacProcessListProvider::new();
    provider.process_facts = ProcessFactsCache::with_map(HashMap::new(), Instant::now());
    let snapshot = provider.refresh(1_000).expect("process list must refresh");
    let row = snapshot
        .items
        .iter()
        .find(|item| item.pid == me)
        .expect("the test process must appear in its own process list");
    assert_eq!(
        row.scalar_observations().nice,
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        row.scalar_observations().threads,
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        row.scalar_observations().start_token,
        ScalarObservation::unavailable(FailureKind::Unsupported),
        "second-resolution process start metadata must not authorize target work"
    );
    assert_eq!(
        row.scalar_observations().start_time_secs,
        ScalarObservation::available(row.current_start_time_secs().unwrap_or_default(), 1_000),
        "the coarse display/history fact remains observable independently"
    );
}

#[test]
fn application_identity_is_decided_whenever_the_executable_path_is_current() {
    // Host-neutral wiring invariant: on any platform a process row whose
    // executable path was successfully observed must never carry an
    // Unknown application identity — the provider either matched a bundle
    // layout (Available) or confirmed a non-bundle path (Absent). Only a
    // missing executable path may honestly stay Unknown.
    use taskmanager_core::ProcessMetadataAvailability;

    let mut provider = MacProcessListProvider::new();
    provider.process_facts = ProcessFactsCache::with_map(HashMap::new(), Instant::now());
    let snapshot = provider.refresh(1_000).expect("process list must refresh");
    let mut rows_with_current_path = 0_usize;
    for row in &snapshot.items {
        if row.metadata_observations().executable_path.availability()
            == ProcessMetadataAvailability::Available
        {
            rows_with_current_path += 1;
            assert_ne!(
                row.application_identity_observation().availability(),
                ProcessMetadataAvailability::Unknown,
                "pid {} has a current executable path, so its application identity must be decided",
                row.pid
            );
        }
    }
    assert!(
        rows_with_current_path > 0,
        "the live table must expose at least one current executable path"
    );
}

#[test]
fn batch_preserves_the_exact_unsupported_failure_for_every_action_and_target() {
    let targets = vec![frozen(4_000_001), frozen(4_000_002), frozen(4_000_003)];
    for action in [
        ProcessBatchAction::End,
        ProcessBatchAction::Kill,
        ProcessBatchAction::Suspend,
        ProcessBatchAction::Resume,
        ProcessBatchAction::SetPriority(taskmanager_core::PriorityTier::High),
    ] {
        let mut provider = MacProcessControlProvider::new();
        let intent = ProcessBatchIntent {
            action,
            scope: Default::default(),
            targets: targets.clone(),
        };
        let result = provider
            .execute_batch(intent)
            .expect("batch must return per-target outcomes");
        assert_eq!(result.targets.len(), targets.len());
        assert!(result.targets.iter().all(|(_, outcome)| {
            *outcome == ProcessBatchTargetResult::Failed(FailureKind::Unsupported)
        }));
    }
}

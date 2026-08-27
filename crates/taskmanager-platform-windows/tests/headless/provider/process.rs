//! Unit and integration tests for Windows process providers and controls.

use super::insights::{environment_value_from_boundary, open_files_value_from_boundary};
use super::*;

fn frozen(pid: u32) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, "test", 1, 1).expect("valid test identity")
}

#[cfg(windows)]
fn live_frozen(pid: u32) -> FrozenProcessIdentity {
    let token = taskmanager_windows_api::process_creation_time_100ns(pid)
        .expect("current process creation time");
    FrozenProcessIdentity::from_authoritative_parts(pid, "test", token, 1)
        .expect("valid live identity")
}

#[test]
fn win_process_insight_providers_reject_a_wrong_creation_token() {
    let current_pid = std::process::id();
    let mut network = WinProcessNetworkProvider;
    let network_res = network.observe(&frozen(current_pid), 1);
    let mut isolation = WinProcessIsolationProvider;
    let isolation_res = isolation.observe(&frozen(current_pid), 1);
    let mut open_files = WinProcessOpenFilesProvider;
    let open_files_res = open_files.observe(&frozen(current_pid), 1);

    #[cfg(windows)]
    {
        assert_eq!(network_res, Err(ProviderFailure::IdentityChanged));
        assert_eq!(isolation_res, Err(ProviderFailure::IdentityChanged));
        // The handle-table walk is bracketed by the same creation-token
        // validation as the other insights, so a wrong token is rejected
        // before any boundary call.
        assert_eq!(open_files_res, Err(ProviderFailure::IdentityChanged));
    }
    #[cfg(not(windows))]
    {
        assert_eq!(network_res, Err(ProviderFailure::Unsupported));
        assert_eq!(isolation_res, Err(ProviderFailure::Unsupported));
        assert_eq!(open_files_res, Err(ProviderFailure::Unsupported));
    }

    let mut resource_control = WinProcessResourceControlProvider;
    let outcome = resource_control.apply_limits(
        &frozen(1),
        &taskmanager_core::ResourceGroupLimitRequest::default(),
    );
    // The wrong frozen token is rejected before any job is created or
    // released; off-Windows the whole lane is the typed dormant fallback.
    #[cfg(windows)]
    assert_eq!(outcome, Err(ProviderFailure::IdentityChanged));
    #[cfg(not(windows))]
    assert_eq!(outcome, Err(ProviderFailure::Unsupported));
}

#[test]
fn windows_resource_assembly_keeps_memory_current_and_other_facts_typed_unsupported() {
    let snapshot = resource_snapshot(4096, 42);

    assert_eq!(snapshot.current_memory_usage_bytes(), Some(4096));
    assert_eq!(snapshot.current_limits(), None);
    assert!(matches!(
        &snapshot.observations().limits,
        ResourceObservation::Unavailable {
            failure: FailureKind::Unsupported
        }
    ));
    assert!(snapshot.sources().iter().any(|source| {
        source.provider == PROCESS_RESOURCE_MEMORY_PROVIDER
            && source.outcome == SourceOutcome::Available
    }));
    assert!(snapshot.sources().iter().any(|source| {
        source.provider == PROCESS_RESOURCE_LIMITS_PROVIDER
            && source.outcome == SourceOutcome::Unavailable(FailureKind::Unsupported)
    }));
}

#[test]
fn open_files_projection_maps_kinds_keeps_unreadable_and_sorts_by_fd() {
    use taskmanager_core::OpenFileKind;
    use taskmanager_windows_api::{WindowsOpenHandleEntry, WindowsOpenHandleKind};

    let raw = vec![
        WindowsOpenHandleEntry {
            handle: 0x120,
            kind: WindowsOpenHandleKind::File,
            target: Some(r"\Device\HarddiskVolume3\app.log".to_string()),
        },
        WindowsOpenHandleEntry {
            handle: 0x30,
            // Unresolved name (failed, timed out, or closed between snapshot
            // and duplication): the entry stays with an honest None target.
            kind: WindowsOpenHandleKind::Other,
            target: None,
        },
        WindowsOpenHandleEntry {
            handle: 0x80,
            kind: WindowsOpenHandleKind::Pipe,
            target: Some(r"\Device\NamedPipe\abc".to_string()),
        },
        WindowsOpenHandleEntry {
            // Exceeds u32::MAX: counted unreadable, never truncated to a fd.
            handle: u64::from(u32::MAX) + 1,
            kind: WindowsOpenHandleKind::File,
            target: Some(r"\Device\HarddiskVolume3\huge".to_string()),
        },
    ];

    let value = open_files_value_from_boundary(raw, 5000);

    assert_eq!(value.state.status, taskmanager_core::DeviceStatus::Healthy);
    assert_eq!(value.state.last_success_ms, Some(5000));
    // Ascending fd order regardless of snapshot order.
    let fds: Vec<u32> = value.entries.iter().map(|entry| entry.fd).collect();
    assert_eq!(fds, vec![0x30, 0x80, 0x120]);
    assert_eq!(
        value.entries[0],
        taskmanager_core::OpenFileEntry {
            fd: 0x30,
            kind: OpenFileKind::Other,
            target: None,
        }
    );
    assert_eq!(value.entries[1].kind, OpenFileKind::Pipe);
    assert_eq!(value.entries[2].kind, OpenFileKind::File);
    // One unresolved target plus one overwide handle value.
    assert_eq!(value.unreadable_count, 2);
}

#[test]
fn win_process_affinity_and_threads_providers_query_real_facts() {
    #[cfg(windows)]
    let target = live_frozen(std::process::id());
    #[cfg(not(windows))]
    let target = frozen(std::process::id());
    let mut affinity = WinProcessAffinityProvider;
    let mut threads = WinProcessThreadsProvider;
    // Affinity (GetProcessAffinityMask) and threads (ToolHelp32 snapshot)
    // live behind the audited Windows API boundary; the cross-target build
    // maps its typed `WindowsApiError::Unsupported` to the same honest
    // provider failure instead of fabricating masks or thread rows.
    #[cfg(windows)]
    {
        let cpus = affinity
            .affinity(&target)
            .expect("affinity query should succeed");
        assert!(!cpus.is_empty());

        let snapshot = threads
            .observe(&target, 1000)
            .expect("threads query should succeed");
        assert!(!snapshot.value.threads.is_empty());
        // Rows keep ascending tid order, and the first observation of a tid
        // has no delta yet, so no per-thread rate is published.
        assert!(snapshot.value.threads.is_sorted_by_key(|thread| thread.tid));
        assert!(
            snapshot
                .value
                .threads
                .iter()
                .all(|thread| thread.cpu_percent.is_none())
        );
    }
    #[cfg(not(windows))]
    {
        assert_eq!(
            affinity.affinity(&target),
            Err(ProviderFailure::Unsupported)
        );
        assert_eq!(
            threads.observe(&target, 1000),
            Err(ProviderFailure::Unsupported)
        );
    }
}

#[test]
fn priority_tiers_map_to_the_historical_windows_priority_classes() {
    use taskmanager_windows_api::ProcessPriorityClass;
    // Exactly the classes the legacy nice-threshold mapping produced for
    // the canonical preset values (-10/0/+10); Realtime/High/Idle were
    // never reachable from the neutral vocabulary.
    for (tier, class) in [
        (PriorityTier::High, ProcessPriorityClass::AboveNormal),
        (PriorityTier::Normal, ProcessPriorityClass::Normal),
        (PriorityTier::Low, ProcessPriorityClass::BelowNormal),
    ] {
        assert_eq!(windows_priority_class(tier), class);
    }
}

#[test]
fn foreign_control_mapping_preserves_rejection_and_helper_availability() {
    use taskmanager_escalation::EscalationDenialReason;
    use taskmanager_escalation::polkit::ForeignProcessControlFailure;

    for (failure, expected) in [
        (
            ForeignProcessControlFailure::IdentityChanged,
            ProviderFailure::IdentityChanged,
        ),
        (
            ForeignProcessControlFailure::PermissionDenied,
            ProviderFailure::PermissionDenied,
        ),
        (
            ForeignProcessControlFailure::Unsupported,
            ProviderFailure::Unsupported,
        ),
        (
            ForeignProcessControlFailure::Rejected,
            ProviderFailure::Rejected,
        ),
        (
            ForeignProcessControlFailure::OperationFailed,
            ProviderFailure::ProviderFault,
        ),
    ] {
        assert_eq!(map_foreign_control_failure(failure), expected);
    }

    // The full five-way denial table (ADR-035): authorization-incomplete and
    // helper protocol violations must never collapse into PermissionDenied
    // (or into a retryable escalation), and each transport fact keeps its
    // own provider outcome.
    for (reason, expected) in [
        (
            EscalationDenialReason::Unsupported,
            ProviderFailure::Unsupported,
        ),
        (
            EscalationDenialReason::PermissionDenied,
            ProviderFailure::PermissionDenied,
        ),
        (
            EscalationDenialReason::AuthorizationUnavailable,
            ProviderFailure::TemporarilyUnavailable,
        ),
        (
            EscalationDenialReason::HelperUnavailable,
            ProviderFailure::RequiresEscalation,
        ),
        (
            EscalationDenialReason::HelperProtocolViolation,
            ProviderFailure::ProviderFault,
        ),
    ] {
        assert_eq!(map_escalation_denial(reason), expected);
    }
}

#[test]
fn suspend_batch_reports_typed_results_per_target() {
    let mut provider = WinProcessControlProvider::new();
    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::Suspend,
        scope: Default::default(),
        targets: vec![frozen(1), frozen(2)],
    };
    let result = provider.execute_batch(intent).expect("batch executes");
    assert_eq!(result.targets.len(), 2);
    // The frozen token never matches a live creation time, so the
    // identity-guarded per-thread path rejects both targets before any
    // thread is touched; the cross-target build keeps its typed
    // Unsupported fallback.
    #[cfg(windows)]
    let expected = FailureKind::IdentityChanged;
    #[cfg(not(windows))]
    let expected = FailureKind::Unsupported;
    for (_, outcome) in &result.targets {
        assert_eq!(*outcome, ProcessBatchTargetResult::Failed(expected));
    }
}

#[test]
fn process_control_requests_dispatch_to_provider_methods() {
    // The executor closure must compile for every request variant without
    // panicking on construction (methods themselves are exercised live).
    let mut provider = WinProcessControlProvider::new();
    assert_eq!(
        provider.send_signal(&frozen(1), ProcessSignal::Hangup),
        Err(ProviderFailure::Unsupported)
    );
    assert_eq!(
        provider.send_signal(&frozen(1), ProcessSignal::Interrupt),
        Err(ProviderFailure::Unsupported)
    );
    let _ = taskmanager_application::ProcessControlRequest::EndTask(frozen(1));
}

#[test]
fn suspend_resume_signals_follow_the_identity_guarded_per_thread_path() {
    // Stop/Continue map onto the audited per-thread suspend/resume boundary;
    // a wrong creation token is rejected before any thread is touched, and
    // the non-Windows build keeps its typed Unsupported fallback.
    let mut provider = WinProcessControlProvider::new();
    #[cfg(windows)]
    let expected = Err(ProviderFailure::IdentityChanged);
    #[cfg(not(windows))]
    let expected = Err(ProviderFailure::Unsupported);
    assert_eq!(
        provider.send_signal(&frozen(1), ProcessSignal::Stop),
        expected
    );
    assert_eq!(
        provider.send_signal(&frozen(1), ProcessSignal::Continue),
        expected
    );
}

#[test]
fn execute_batch_end_action_echoes_intent_and_reports_per_target_failure() {
    // A nonexistent pid is absent from the refreshed sysinfo snapshot on
    // every host, so the exact identity path returns IdentityChanged and
    // preserves that failure in the batch result. This exercises the
    // batch return STRUCTURE (intent
    // echoed, one per-target outcome) without depending on a killable pid.
    let mut provider = WinProcessControlProvider::new();
    let sentinel_a = u32::MAX;
    let sentinel_b = u32::MAX - 1;
    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::End,
        scope: Default::default(),
        targets: vec![frozen(sentinel_a), frozen(sentinel_b)],
    };
    let result = provider
        .execute_batch(intent.clone())
        .expect("batch executes");
    // The intent is echoed back verbatim.
    assert_eq!(result.intent.action, intent.action);
    assert_eq!(result.targets.len(), 2);
    // Each target keeps its identity and carries the typed failure.
    assert_eq!(result.targets[0].0.pid, sentinel_a);
    assert_eq!(result.targets[1].0.pid, sentinel_b);
    for (_, outcome) in &result.targets {
        assert_eq!(
            *outcome,
            ProcessBatchTargetResult::Failed(FailureKind::IdentityChanged)
        );
    }
}

#[test]
fn exact_process_control_reports_identity_changed_for_absent_pid() {
    // A fresh provider owns an empty sysinfo snapshot (no refresh), so any
    // pid lookup misses -> IdentityChanged. This is the direct-source
    // counterpart to the batch path's typed IdentityChanged result and is
    // host-independent (no process table is enumerated).
    let provider = WinProcessControlProvider::new();
    assert_eq!(
        provider.kill_process_from_snapshot(&frozen(u32::MAX)),
        Err(ProviderFailure::IdentityChanged)
    );
}

#[test]
fn win_process_environment_provider_rejects_a_wrong_creation_token() {
    let mut environment = WinProcessEnvironmentProvider;
    let result = environment.observe(&frozen(std::process::id()), 1);
    // The PEB read is bracketed by the same creation-token validation as the
    // other insights, so a wrong token is rejected before any boundary call;
    // off-Windows the whole lane is the typed dormant fallback.
    #[cfg(windows)]
    assert_eq!(result, Err(ProviderFailure::IdentityChanged));
    #[cfg(not(windows))]
    assert_eq!(result, Err(ProviderFailure::Unsupported));
}

#[test]
fn environment_projection_keeps_order_honest_absence_and_truncation() {
    let raw = taskmanager_windows_api::WindowsProcessEnvironmentBlock {
        working_directory: Some(r"C:\work".to_string()),
        entries: vec![
            ("PATH".to_string(), r"C:\bin".to_string()),
            ("TMP".to_string(), r"C:\tmp".to_string()),
        ],
        truncated_count: 3,
    };
    let value = environment_value_from_boundary(raw, 7000);

    assert_eq!(value.state.status, taskmanager_core::DeviceStatus::Healthy);
    assert_eq!(value.state.last_success_ms, Some(7000));
    assert_eq!(
        value.working_directory,
        Some(std::path::PathBuf::from(r"C:\work"))
    );
    // Source order and verbatim key/value pairs pass through untouched.
    assert_eq!(value.entries[0].key, "PATH");
    assert_eq!(value.entries[0].value, r"C:\bin");
    assert_eq!(value.entries[1].key, "TMP");
    assert_eq!(value.entries[1].value, r"C:\tmp");
    assert_eq!(value.truncated_count, 3);

    // An unreadable cwd stays an honest None; it is never "/" or the exe dir.
    let absent = environment_value_from_boundary(
        taskmanager_windows_api::WindowsProcessEnvironmentBlock {
            working_directory: None,
            entries: Vec::new(),
            truncated_count: 0,
        },
        7001,
    );
    assert_eq!(absent.working_directory, None);
    assert_eq!(absent.entries, Vec::new());
}

#[test]
fn windows_environment_budgets_mirror_the_core_contract_caps() {
    // The contract owns the byte/entry budgets; this pins the boundary's
    // copies to it so the two authorities cannot drift apart silently.
    assert_eq!(
        taskmanager_windows_api::MAX_PROCESS_ENVIRONMENT_BYTES,
        taskmanager_core::MAX_ENVIRONMENT_BYTES
    );
    assert_eq!(
        taskmanager_windows_api::MAX_PROCESS_ENVIRONMENT_ENTRIES,
        taskmanager_core::MAX_ENVIRONMENT_ENTRIES
    );
}

#[test]
fn win_process_environment_provider_reads_the_live_process_facts() {
    #[cfg(windows)]
    {
        let target = live_frozen(std::process::id());
        let mut environment = WinProcessEnvironmentProvider;
        let snapshot = environment
            .observe(&target, 1000)
            .expect("environment query should succeed");
        // A live process always carries variables (SystemRoot at minimum) and
        // was launched from a working directory, so both facts are present —
        // and entries keep the boundary's source order.
        assert!(!snapshot.value.entries.is_empty());
        assert!(snapshot.value.working_directory.is_some());
        assert!(
            snapshot
                .value
                .entries
                .iter()
                .all(|entry| !entry.key.is_empty())
        );
    }
    #[cfg(not(windows))]
    {
        let mut environment = WinProcessEnvironmentProvider;
        assert_eq!(
            environment.observe(&frozen(1), 1000),
            Err(ProviderFailure::Unsupported)
        );
    }
}

//! Scenario-specific fixture preparation for the capture evidence route:
//! typed process/service/startup/history/diagnostic actions and the
//! force-kill intent family, split from the shared capture suite to keep
//! every module under the line guard.

use super::*;
use taskmanager_core::core::process::ProcessLiveKey;

#[test]
fn process_and_service_capture_actions_are_typed_and_non_destructive() {
    let mut properties =
        CaptureEvidence::for_test(Some(CaptureScenario::ProcessPropertiesPerformance));
    let mut processes = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(42)
            .name("capture-process".into())
            .build(),
    ];
    assert_eq!(
        properties.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes),
        Some(CaptureProcessAction::Properties(
            ProcessLiveKey::from_parts(42, taskmanager_test_support::fixture_start_token(42))
                .expect("fixture identity"),
            ProcessDetailsSection::Performance
        ))
    );
    assert_eq!(processes[0].cpu_history.len(), 60);
    let mut tree = CaptureEvidence::for_test(Some(CaptureScenario::ProcessTreeConfirm));
    let action = tree
        .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
        .unwrap();
    let CaptureProcessAction::Termination(intent) = action else {
        panic!("expected tree confirmation")
    };
    assert_eq!(intent.action, ProcessTerminationAction::EndProcessTree);
    assert_eq!(intent.descendant_count(), 6);
    let mut service = CaptureEvidence::for_test(Some(CaptureScenario::ServiceDetailsLogs));
    let mut services = Vec::new();
    assert_eq!(
        service.on_services_update(true, &mut services),
        Some(ServiceId::new(
            "fixture.service:taskmanager-capture.service"
        ))
    );
    assert_eq!(services.len(), 1);
    assert!(!service.scenario_ready);
    service.mark_service_details_ready(true);
    assert!(service.scenario_ready);
}

#[test]
fn insights_scenarios_wait_for_exact_dialog_state_and_never_create_control_intents() {
    for scenario in [
        CaptureScenario::ProcessNetworkDetails,
        CaptureScenario::ProcessGpuDetails,
        CaptureScenario::ProcessResourceLimits,
        CaptureScenario::ProcessIsolation,
    ] {
        let mut evidence = CaptureEvidence::for_test(Some(scenario));
        let mut snapshot = SystemSnapshot::default();
        evidence.on_snapshot(&mut snapshot);
        let mut processes = Vec::new();
        let action = evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .expect("strict insights scenario should prepare a render fixture");
        let CaptureProcessAction::Insights { identity, state } = action else {
            panic!("insights captures must not create a process-control intent")
        };
        assert_eq!(
            identity,
            ProcessLiveKey::from_parts(4242, 987_654).expect("fixture identity")
        );
        assert!(
            processes
                .iter()
                .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
        );
        assert!(matches!(state, ProcessInsightsState::Ready(_)));
        assert!(!evidence.scenario_ready);
        evidence.mark_process_insights_ready(false);
        assert!(!evidence.scenario_ready);
        evidence.mark_process_insights_ready(true);
        assert!(evidence.scenario_ready);
        processes.clear();
        assert!(
            evidence
                .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
                .is_none()
        );
        assert!(
            processes
                .iter()
                .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
        );
    }
}

#[test]
fn batch_capture_freezes_three_identities_without_executing() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ProcessBatchConfirm));
    let mut processes = Vec::new();
    let action = evidence
        .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
        .unwrap();
    let CaptureProcessAction::Batch(intent) = action else {
        panic!("expected typed batch confirmation")
    };
    assert_eq!(intent.action, ProcessBatchAction::Suspend);
    assert_eq!(
        intent
            .targets
            .iter()
            .map(|target| target.pid)
            .collect::<Vec<_>>(),
        [91_001, 91_002, 91_003]
    );
    assert!(
        intent
            .targets
            .iter()
            .all(|target| target.start_time_secs > 0)
    );
    assert!(!evidence.scenario_ready);
    evidence.mark_process_batch_ready(true, intent.targets.len());
    assert!(evidence.scenario_ready);
    assert!(
        evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .is_none()
    );
}

#[test]
fn startup_capture_distinguishes_measured_from_unknown() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::StartupImpact));
    let mut entries = Vec::new();
    let mut boot_evidence = None;
    assert!(!evidence.on_startup_update(false, &mut entries, &mut boot_evidence));
    assert!(evidence.on_startup_update(true, &mut entries, &mut boot_evidence));
    // The impact-only scenario seeds the list but never the waterfall pair.
    assert!(boot_evidence.is_none());
    assert!(evidence.startup_boot_baseline().is_none());
    assert_eq!(entries.len(), 2);
    assert!(matches!(
        entries[0].impact_evidence,
        StartupImpactEvidence::Measured { duration_ms: 842 }
    ));
    assert!(matches!(
        entries[1].impact_evidence,
        StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented
        }
    ));
    assert!(!evidence.scenario_ready);
    evidence.mark_startup_impact_ready(true, &entries);
    assert!(evidence.scenario_ready);
}

#[test]
fn startup_failure_evidence_capture_seeds_failed_units_and_chain() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::StartupFailureEvidence));
    let mut entries = Vec::new();
    let mut boot_evidence = None;
    assert!(evidence.on_startup_update(true, &mut entries, &mut boot_evidence));
    let snapshot = boot_evidence.clone().expect("failure evidence fixture");
    assert_eq!(snapshot.failed_units.len(), 3);
    assert_eq!(snapshot.critical_chain.len(), 2);
    assert!(evidence.startup_boot_baseline().is_none());
    assert!(!evidence.scenario_ready);
    evidence.restore_startup_fixture(&mut entries, &mut boot_evidence);
    let restored = boot_evidence.expect("fixture must survive a later platform batch");
    assert_eq!(restored.failed_units.len(), 3);
    evidence.mark_startup_failure_evidence_ready(true, Some(&snapshot));
    assert!(evidence.scenario_ready);
}

#[test]
fn boot_markers_capture_seeds_waterfall_and_baseline_pair() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::StartupBootMarkers));
    let mut entries = Vec::new();
    let mut boot_evidence = None;
    assert!(evidence.on_startup_update(true, &mut entries, &mut boot_evidence));
    // The waterfall evidence carries a measured critical chain...
    let evidence_snapshot =
        boot_evidence.expect("markers scenario seeds the boot evidence snapshot");
    assert_eq!(evidence_snapshot.critical_chain.len(), 3);
    assert!(evidence_snapshot.critical_chain_failure.is_none());
    // ...and the baseline covers the same units, so segment deltas exist and
    // cover all three chip states (slower / faster / unchanged).
    let baseline = evidence
        .startup_boot_baseline()
        .expect("markers scenario seeds the comparison baseline");
    let units: Vec<&str> = baseline
        .segments
        .iter()
        .map(|segment| segment.unit.as_str())
        .collect();
    assert_eq!(units.len(), 3);
    for unit in evidence_snapshot
        .critical_chain
        .iter()
        .map(|n| n.unit.as_str())
    {
        assert!(units.contains(&unit), "baseline must cover unit {unit}");
    }
    let current = taskmanager_core::core::startup::BootTimeline::from_critical_chain(
        &evidence_snapshot.critical_chain,
        taskmanager_core::core::startup::DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS,
        taskmanager_core::core::startup::DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    );
    let deltas: Vec<i64> = taskmanager_core::core::startup::segment_deltas(&current, baseline)
        .into_iter()
        .map(|delta| delta.delta_ms)
        .collect();
    assert!(deltas.contains(&200), "one unit slower: {deltas:?}");
    assert!(deltas.contains(&-300), "one unit faster: {deltas:?}");
    assert!(deltas.contains(&0), "one unit unchanged: {deltas:?}");
    // Readiness requires BOTH the page and the seeded pair.
    assert!(!evidence.scenario_ready);
    evidence.mark_startup_boot_markers_ready(true, false);
    assert!(!evidence.scenario_ready);
    evidence.mark_startup_boot_markers_ready(true, true);
    assert!(evidence.scenario_ready);
}

#[test]
fn history_replay_capture_opens_once_and_marks_ready_only_when_loaded() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::HistoryReplay));
    // Before readiness the open request must stay refused.
    assert!(!evidence.history_replay_open_requested());
    evidence.telemetry_ready = true;
    assert!(!evidence.history_replay_open_requested());
    evidence.ui_data_ready = true;
    assert!(evidence.history_replay_open_requested());
    // Opening latches: the request never fires twice (the panel must not be
    // toggled closed again on a later tick).
    evidence.note_history_replay_opened();
    assert!(!evidence.history_replay_open_requested());
    // Readiness needs rows actually loaded, not just the panel open.
    evidence.mark_history_replay_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_history_replay_ready(true);
    assert!(evidence.scenario_ready);
}

#[test]
fn application_history_capture_requires_ready_non_empty_durable_projection() {
    use taskmanager_application::ApplicationHistoryStatus;

    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ApplicationHistoryReplay));
    evidence.mark_application_history_replay_ready(true, ApplicationHistoryStatus::Ready, 3);
    assert!(
        !evidence.scenario_ready,
        "normal capture facts are still pending"
    );

    evidence.telemetry_ready = true;
    evidence.ui_data_ready = true;
    evidence.mark_application_history_replay_ready(false, ApplicationHistoryStatus::Ready, 3);
    assert!(
        !evidence.scenario_ready,
        "the application-history page must be active"
    );
    evidence.mark_application_history_replay_ready(true, ApplicationHistoryStatus::Collecting, 3);
    assert!(
        !evidence.scenario_ready,
        "an active empty reader is not replay evidence"
    );
    evidence.mark_application_history_replay_ready(true, ApplicationHistoryStatus::Ready, 0);
    assert!(
        !evidence.scenario_ready,
        "Ready without joined rows is not evidence"
    );
    evidence.mark_application_history_replay_ready(true, ApplicationHistoryStatus::Ready, 3);
    assert!(evidence.scenario_ready);
}

#[test]
fn diagnostic_capture_requests_preview_but_never_confirms_write() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::DiagnosticPreview));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();
    assert!(
        evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .is_none()
    );
    assert_eq!(processes.len(), 1);
    assert!(processes[0].cmdline.contains("/home/<user>"));
    assert!(evidence.diagnostic_preview_requested());
    evidence.mark_diagnostic_preview_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_diagnostic_preview_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.diagnostic_preview_requested());
}

#[test]
fn diagnostic_failure_capture_prepares_ui_state_without_worker_action() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::DiagnosticFailure));
    assert!(!evidence.diagnostic_failure_requested());
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();
    assert!(
        evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .is_none()
    );
    assert!(evidence.diagnostic_failure_requested());
    evidence.mark_diagnostic_failure_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_diagnostic_failure_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.diagnostic_failure_requested());
}

#[test]
fn force_kill_scenario_only_returns_one_non_executing_intent() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ProcessForceKill));
    let mut processes = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1)
            .name("init".into())
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(4242)
            .name("capture-worker".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(42_420, 1),
                ..Default::default()
            })
            .build(),
    ];
    assert!(
        evidence
            .on_processes_update(false, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .is_none()
    );
    assert_eq!(
        evidence.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes),
        Some(CaptureProcessAction::Termination(
            crate::gpui_app::root::termination::snapshot_single_process(
                ProcessTerminationAction::ForceKill,
                ProcessLiveKey::from_parts(4242, 42_420).expect("fixture identity"),
                &processes,
            )
            .expect("capture fixture has an authoritative start token")
        ))
    );
    assert!(evidence.ui_data_ready);
    assert!(evidence.scenario_ready);
    assert!(
        evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .is_none()
    );
}

#[test]
fn force_kill_capture_prefers_a_readable_process_name() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ProcessForceKill));
    let mut processes = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(20)
            .name("worker/u65:0-btrfs-endio-meta".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(2_000, 1),
                ..Default::default()
            })
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(30)
            .name("bash".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(3_000, 1),
                ..Default::default()
            })
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(40)
            .name("taskmanager".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(4_000, 1),
                ..Default::default()
            })
            .build(),
    ];
    let action = evidence
        .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
        .unwrap();
    let CaptureProcessAction::Termination(intent) = action else {
        panic!("expected termination capture action")
    };
    assert_eq!(intent.root.pid, 40);
    assert_eq!(intent.action, ProcessTerminationAction::ForceKill);
}

#[test]
fn standard_evidence_marks_updates_without_mutating_data() {
    let mut evidence = CaptureEvidence::for_test(None);
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    assert!(snapshot.disks.is_empty());
    assert!(evidence.telemetry_ready);
    assert!(
        evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut Vec::new())
            .is_none()
    );
    assert!(evidence.ui_data_ready);
    assert!(!evidence.scenario_ready);
}

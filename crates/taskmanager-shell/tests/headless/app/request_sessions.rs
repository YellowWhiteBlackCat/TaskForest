//! Cross-boundary terminal filtering for the three remaining request sessions.

use super::super::*;
use taskmanager_application::{
    CapabilityId, CorrelatedEvent, DeviceGeneration, DeviceId, EventSequence, GpuEngineRowsEvent,
    GpuEngineRowsSnapshot, PlatformEventContext, ProcessAffinityEvent, ProcessBatchAction,
    ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult, ProcessEvent,
    ProcessGroupScope, ProviderFailure, RequestId, ShellEvent, ShellUiActionIntent, SmartEvent,
    SmartObservationBatch, SmartSelfTestIntent, SmartSelfTestKind, SmartStateRevision,
    StorageDeviceKey, UrlOpenRequest,
};

fn request(value: u64) -> RequestId {
    RequestId::new(value).expect("fixture request id is non-zero")
}

fn process(pid: u32, token: u64) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, format!("process-{pid}"), 10, token)
        .expect("fixture process identity is authoritative")
}

fn context(request_id: RequestId, capability: CapabilityId, sequence: u64) -> PlatformEventContext {
    PlatformEventContext {
        request_id,
        capability,
        provider: None,
        sequence: EventSequence::new(sequence),
        observed_at_ms: sequence,
    }
}

fn batch_intent(target: FrozenProcessIdentity, action: ProcessBatchAction) -> ProcessBatchIntent {
    ProcessBatchIntent {
        action,
        scope: ProcessGroupScope::PidAdjacency,
        targets: vec![target],
    }
}

fn smart_intent(generation: u64) -> SmartSelfTestIntent {
    SmartSelfTestIntent {
        device_id: DeviceId::new("disk:fixture"),
        device_generation: DeviceGeneration::new(generation),
        device_key: StorageDeviceKey::new("nvme0n1"),
        display_name: "Fixture disk".into(),
        kind: SmartSelfTestKind::Short,
    }
}

#[test]
fn direct_track_filters_wrong_identity_late_and_duplicate_terminals_once() {
    let mut track = DirectTrackState::default();

    let affinity_target = process(42, 100);
    let affinity_attempt = track.begin_process_affinity_read(affinity_target.clone());
    assert!(track.accept_process_affinity_read(affinity_attempt, request(1)));
    let mut wrong_affinity = PlatformEventBatch::default();
    wrong_affinity
        .process_affinity_events
        .push(CorrelatedEvent::new(
            context(request(1), CapabilityId::PROCESS_AFFINITY, 1),
            ProcessAffinityEvent::Snapshot {
                target: process(42, 200),
                cpus: vec![7],
            },
        ));
    assert!(
        track
            .apply_platform_batch(wrong_affinity)
            .process_affinity_results
            .is_empty()
    );
    assert!(matches!(
        track.process_affinity_state(),
        taskmanager_application::ProcessAffinityState::Loading { .. }
    ));

    let mut affinity = PlatformEventBatch::default();
    affinity.process_affinity_events.push(CorrelatedEvent::new(
        context(request(1), CapabilityId::PROCESS_AFFINITY, 2),
        ProcessAffinityEvent::Snapshot {
            target: affinity_target.clone(),
            cpus: vec![0, 2],
        },
    ));
    let accepted = track.apply_platform_batch(affinity.clone());
    assert_eq!(accepted.process_affinity_results.len(), 1);
    assert!(
        track
            .apply_platform_batch(affinity)
            .process_affinity_results
            .is_empty()
    );

    let batch_target = process(77, 700);
    let batch_intent = batch_intent(batch_target.clone(), ProcessBatchAction::Suspend);
    let batch_attempt = track.begin_process_batch(batch_intent.clone());
    assert!(track.accept_process_batch(batch_attempt, request(2)));
    let result = ProcessBatchResult {
        intent: batch_intent,
        targets: vec![(batch_target, ProcessBatchTargetResult::Applied)],
    };
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent::new(
        context(request(2), CapabilityId::PROCESS_CONTROL, 3),
        ProcessEvent::BatchCompleted(result),
    ));
    let accepted = track.apply_platform_batch(batch.clone());
    assert_eq!(accepted.batch_results.len(), 1);
    assert!(track.apply_platform_batch(batch).batch_results.is_empty());

    let smart = smart_intent(3);
    let smart_attempt = track.begin_smart_self_test(smart.clone());
    assert!(track.accept_smart_self_test(smart_attempt, request(3)));
    let mut wrong_smart = PlatformEventBatch::default();
    wrong_smart.smart_events.push(CorrelatedEvent::new(
        context(request(3), CapabilityId::SMART_CONTROL, 4),
        SmartEvent::Batch(SmartObservationBatch {
            revision: SmartStateRevision::new(1),
            subject: Some(smart_intent(4).target()),
            ..SmartObservationBatch::default()
        }),
    ));
    assert!(
        track
            .apply_platform_batch(wrong_smart)
            .smart_self_test_results
            .is_empty()
    );
    let mut smart_batch = PlatformEventBatch::default();
    smart_batch.smart_events.push(CorrelatedEvent::new(
        context(request(3), CapabilityId::SMART_CONTROL, 5),
        SmartEvent::Batch(SmartObservationBatch {
            revision: SmartStateRevision::new(2),
            subject: Some(smart.target()),
            ..SmartObservationBatch::default()
        }),
    ));
    let accepted = track.apply_platform_batch(smart_batch.clone());
    assert_eq!(accepted.smart_self_test_results.len(), 1);
    assert!(
        track
            .apply_platform_batch(smart_batch)
            .smart_self_test_results
            .is_empty()
    );
}

#[test]
fn direct_track_correlates_gpu_network_and_ui_action_terminals_once() {
    let mut track = DirectTrackState::default();
    let gpu = DeviceId::new("gpu:0");
    let gpu_attempt = track.begin_gpu_engine_rows_request(gpu.clone());
    assert!(track.accept_gpu_engine_rows_request(gpu_attempt, request(10)));

    let mut wrong_gpu = PlatformEventBatch::default();
    wrong_gpu.gpu_engine_rows_events.push(CorrelatedEvent::new(
        context(request(10), CapabilityId::TELEMETRY_GPU_ENGINES, 10),
        GpuEngineRowsEvent::Update(GpuEngineRowsSnapshot::success(
            DeviceId::new("gpu:1"),
            vec![],
        )),
    ));
    assert!(
        !track
            .apply_platform_batch(wrong_gpu)
            .changes
            .gpu_engine_rows
    );

    let gpu_terminal = CorrelatedEvent::new(
        context(request(10), CapabilityId::TELEMETRY_GPU_ENGINES, 11),
        GpuEngineRowsEvent::Update(GpuEngineRowsSnapshot::success(gpu.clone(), vec![])),
    );
    let gpu_batch = PlatformEventBatch {
        gpu_engine_rows_events: vec![gpu_terminal.clone(), gpu_terminal],
        ..PlatformEventBatch::default()
    };
    let accepted = track.apply_platform_batch(gpu_batch);
    assert!(accepted.changes.gpu_engine_rows);
    assert!(matches!(
        track.gpu_engine_rows_state(),
        taskmanager_application::GpuEngineRowsState::Ready(ready)
            if ready.snapshot.device_id == gpu
    ));

    let ui_attempt = track.begin_shell_ui_action(ShellUiActionIntent::OpenUrl(UrlOpenRequest {
        url: "https://example.invalid".into(),
    }));
    assert!(track.accept_shell_ui_action(ui_attempt, request(11)));
    let mut shell_batch = PlatformEventBatch::default();
    shell_batch.shell_events.push(CorrelatedEvent::new(
        context(request(11), CapabilityId::URL_OPEN, 12),
        ShellEvent::CommandLaunched { pid: 99 },
    ));
    shell_batch.shell_events.push(CorrelatedEvent::new(
        context(request(11), CapabilityId::URL_OPEN, 13),
        ShellEvent::TargetOpened,
    ));
    let accepted = track.apply_platform_batch(shell_batch);
    assert_eq!(accepted.shell_events.len(), 1);
    assert!(matches!(
        track.shell_ui_action_state(),
        taskmanager_application::ShellUiActionState::Ready(_)
    ));

    let network_attempt = track.begin_network_escalation();
    assert!(track.accept_network_escalation(network_attempt, request(12)));
    let network_terminal = CorrelatedEvent::new(
        context(request(12), CapabilityId::PROCESS_NETWORK_ESCALATION, 14),
        ProcessEvent::NetworkCaptureEscalated,
    );
    let network_batch = PlatformEventBatch {
        process_events: vec![network_terminal.clone(), network_terminal],
        ..PlatformEventBatch::default()
    };
    let accepted = track.apply_platform_batch(network_batch);
    assert_eq!(accepted.network_capture_escalations, vec![request(12)]);
    assert!(matches!(
        track.network_escalation_state(),
        taskmanager_application::NetworkEscalationState::Ready(_)
    ));
}

#[test]
fn shell_app_uses_the_same_close_and_late_terminal_rules() {
    let mut app = ShellApp::new();
    let gpu = DeviceId::new("gpu:0");
    let attempt = app.begin_gpu_engine_rows_request(gpu.clone());
    assert!(app.accept_gpu_engine_rows_request(attempt, request(20)));
    app.close_gpu_engine_rows_request();

    let mut batch = PlatformEventBatch::default();
    batch.gpu_engine_rows_events.push(CorrelatedEvent::new(
        context(request(20), CapabilityId::TELEMETRY_GPU_ENGINES, 20),
        GpuEngineRowsEvent::Update(GpuEngineRowsSnapshot::success(gpu, vec![])),
    ));
    app.apply_platform_batch(batch);
    assert!(matches!(
        app.gpu_engine_rows_state(),
        taskmanager_application::GpuEngineRowsState::Closed
    ));
}

#[test]
fn direct_track_routes_operation_failures_to_the_exact_active_session() {
    let mut track = DirectTrackState::default();
    let gpu_attempt = track.begin_gpu_engine_rows_request(DeviceId::new("gpu:0"));
    assert!(track.accept_gpu_engine_rows_request(gpu_attempt, request(30)));
    let network_attempt = track.begin_network_escalation();
    assert!(track.accept_network_escalation(network_attempt, request(31)));
    let ui_attempt = track.begin_shell_ui_action(ShellUiActionIntent::OpenUrl(UrlOpenRequest {
        url: "https://example.invalid".into(),
    }));
    assert!(track.accept_shell_ui_action(ui_attempt, request(32)));

    let operation_failure =
        |request_id, capability, kind| taskmanager_application::OperationFailure {
            request_id,
            capability,
            sequence: EventSequence::new(request_id.get()),
            kind,
            retry: ProviderFailure::from_kind(kind).retry(),
            provider: None,
            observed_at_ms: request_id.get(),
        };
    let mut batch = PlatformEventBatch::default();
    batch.failures.push(operation_failure(
        request(30),
        CapabilityId::TELEMETRY_GPU_ENGINES,
        taskmanager_application::FailureKind::PermissionDenied,
    ));
    batch.failures.push(operation_failure(
        request(31),
        CapabilityId::PROCESS_NETWORK_ESCALATION,
        taskmanager_application::FailureKind::Rejected,
    ));
    batch.failures.push(operation_failure(
        request(32),
        CapabilityId::RESOURCE_REVEAL,
        taskmanager_application::FailureKind::ProviderFault,
    ));
    batch.failures.push(operation_failure(
        request(32),
        CapabilityId::URL_OPEN,
        taskmanager_application::FailureKind::ProviderFault,
    ));
    let _ = track.apply_platform_batch(batch);

    assert!(matches!(
        track.gpu_engine_rows_state(),
        taskmanager_application::GpuEngineRowsState::Failed(_)
    ));
    assert!(matches!(
        track.network_escalation_state(),
        taskmanager_application::NetworkEscalationState::Failed(_)
    ));
    assert!(matches!(
        track.shell_ui_action_state(),
        taskmanager_application::ShellUiActionState::Failed(_)
    ));
}

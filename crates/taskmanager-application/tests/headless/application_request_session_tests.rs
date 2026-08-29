use taskmanager_application::{
    CommandLaunchRequest, GpuEngineRowsRequestFailure, GpuEngineRowsSession, GpuEngineRowsState,
    NetworkEscalationSession, NetworkEscalationState, ProcessAffinitySession, ProcessAffinityState,
    ProcessBatchSession, ProcessBatchState, RequestCorrelation, ResourceRevealRequest, ShellEvent,
    ShellUiActionIntent, ShellUiActionReceipt, ShellUiActionSession, ShellUiActionState,
    SmartSelfTestSession, SmartSelfTestState, UrlOpenRequest,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::metrics::GpuEngineRowsSnapshot;
use taskmanager_core::core::process::{
    FrozenProcessIdentity, PriorityTier, ProcessBatchAction, ProcessBatchIntent,
    ProcessBatchResult, ProcessBatchTargetResult, ProcessGroupScope,
};
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_core::core::target::StorageDeviceKey;
use taskmanager_platform_contract::{CapabilityId, RequestId};

fn request(value: u64) -> RequestId {
    RequestId::new(value).expect("fixture request id is non-zero")
}

fn process(pid: u32, token: u64) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, format!("process-{pid}"), 10, token)
        .expect("fixture process identity is authoritative")
}

fn batch(target: FrozenProcessIdentity, action: ProcessBatchAction) -> ProcessBatchIntent {
    ProcessBatchIntent {
        action,
        scope: ProcessGroupScope::PidAdjacency,
        targets: vec![target],
    }
}

fn smart(generation: u64) -> SmartSelfTestIntent {
    SmartSelfTestIntent {
        device_id: DeviceId::new("disk:fixture"),
        device_generation: DeviceGeneration::new(generation),
        device_key: StorageDeviceKey::new("nvme0n1"),
        display_name: "Fixture disk".into(),
        kind: SmartSelfTestKind::Short,
    }
}

#[test]
fn affinity_session_covers_replace_close_retry_and_exact_identity_terminals() {
    let first = process(42, 100);
    let reused_pid = process(42, 200);
    let mut session = ProcessAffinitySession::default();

    let old_attempt = session.begin_attempt(first.clone());
    let new_attempt = session.begin_attempt(reused_pid.clone());
    assert!(!session.accept_attempt(old_attempt, request(1)));
    assert!(session.accept_attempt(new_attempt, request(2)));
    assert!(!session.complete(request(2), first.clone(), vec![0]));
    session.close();
    assert!(!session.complete(request(2), reused_pid.clone(), vec![0]));

    let attempt = session.begin_attempt(reused_pid.clone());
    assert!(session.accept_attempt(attempt, request(3)));
    assert!(session.complete(request(3), reused_pid.clone(), vec![0, 2]));
    assert!(!session.complete(request(3), reused_pid.clone(), vec![1]));

    let refresh = session.begin_attempt(reused_pid.clone());
    assert!(session.accept_attempt(refresh, request(4)));
    assert!(session.fail(request(4), FailureKind::TimedOut));
    let ProcessAffinityState::Failed {
        last_good: Some(last_good),
        ..
    } = session.state()
    else {
        panic!("same-target failure retains the last-good mask")
    };
    assert_eq!(last_good.cpus, vec![0, 2]);
    assert!(session.retry().is_some());

    let _ = session.begin_attempt(first);
    let ProcessAffinityState::Loading { last_good, .. } = session.state() else {
        panic!("replacement starts a new loading generation")
    };
    assert!(
        last_good.is_none(),
        "identity change cannot inherit another target's mask"
    );
}

#[test]
fn batch_session_accepts_one_matching_intent_and_retries_submission_failure() {
    let target = process(77, 700);
    let first = batch(target.clone(), ProcessBatchAction::Suspend);
    let different = batch(
        target.clone(),
        ProcessBatchAction::SetPriority(PriorityTier::High),
    );
    let mut session = ProcessBatchSession::default();

    let stale_attempt = session.begin_attempt(first.clone());
    let active_attempt = session.begin_attempt(first.clone());
    assert!(!session.accept_attempt(stale_attempt, request(10)));
    assert!(session.accept_attempt(active_attempt, request(11)));
    assert!(!session.complete(
        request(11),
        ProcessBatchResult {
            intent: different,
            targets: vec![(target.clone(), ProcessBatchTargetResult::Applied)],
        },
    ));
    let result = ProcessBatchResult {
        intent: first.clone(),
        targets: vec![(target, ProcessBatchTargetResult::Applied)],
    };
    assert!(session.complete(request(11), result.clone()));
    assert!(!session.complete(request(11), result));
    session.close();
    assert!(matches!(session.state(), ProcessBatchState::Idle));

    let rejected = session.begin_attempt(first.clone());
    assert!(session.reject_attempt(rejected, FailureKind::TemporarilyUnavailable));
    let retry = session.retry().expect("failed intent can be retried");
    assert!(matches!(
        session.state(),
        ProcessBatchState::Loading(loading)
            if loading.correlation == RequestCorrelation::Attempt(retry)
                && loading.intent == first
    ));
}

#[test]
fn smart_session_requires_request_and_device_generation_and_drops_late_terminal() {
    let first = smart(1);
    let replacement = smart(2);
    let mut session = SmartSelfTestSession::default();

    let stale_attempt = session.begin_attempt(first.clone());
    let active_attempt = session.begin_attempt(replacement.clone());
    assert!(!session.accept_attempt(stale_attempt, request(20)));
    assert!(session.accept_attempt(active_attempt, request(21)));
    assert!(!session.complete(request(21), &first.target()));
    assert!(session.complete(request(21), &replacement.target()));
    assert!(!session.complete(request(21), &replacement.target()));
    session.close();
    assert!(!session.fail(request(21), FailureKind::ProviderFault));

    let attempt = session.begin_attempt(replacement.clone());
    assert!(session.accept_attempt(attempt, request(22)));
    assert!(session.fail(request(22), FailureKind::PermissionDenied));
    let retry = session.retry().expect("failed self-test can be retried");
    assert!(matches!(
        session.state(),
        SmartSelfTestState::Loading(loading)
            if loading.correlation == RequestCorrelation::Attempt(retry)
                && loading.intent == replacement
    ));
}

#[test]
fn gpu_engine_rows_session_correlates_device_replace_close_and_provider_failure() {
    let gpu_0 = DeviceId::new("gpu:0");
    let gpu_1 = DeviceId::new("gpu:1");
    let mut session = GpuEngineRowsSession::default();

    let stale_attempt = session.begin_attempt(gpu_0.clone());
    let active_attempt = session.begin_attempt(gpu_1.clone());
    assert!(!session.accept_attempt(stale_attempt, request(30)));
    assert!(session.accept_attempt(active_attempt, request(31)));
    assert!(!session.complete(
        request(31),
        GpuEngineRowsSnapshot::success(gpu_0.clone(), vec![]),
    ));
    session.close();
    assert!(!session.complete(request(31), GpuEngineRowsSnapshot::success(gpu_1, vec![]),));

    let attempt = session.begin_attempt(gpu_0.clone());
    assert!(session.accept_attempt(attempt, request(32)));
    assert!(session.complete(
        request(32),
        GpuEngineRowsSnapshot::success(gpu_0.clone(), vec![]),
    ));
    assert!(!session.complete(
        request(32),
        GpuEngineRowsSnapshot::success(gpu_0.clone(), vec![]),
    ));

    let refresh = session.begin_attempt(gpu_0.clone());
    assert!(session.accept_attempt(refresh, request(33)));
    assert!(session.complete(
        request(33),
        GpuEngineRowsSnapshot::failed(
            gpu_0,
            FailureKind::PermissionDenied,
            "permission prompt dismissed",
        ),
    ));
    let GpuEngineRowsState::Failed(failed) = session.state() else {
        panic!("provider failure must be a typed terminal")
    };
    assert!(matches!(
        failed.failure,
        GpuEngineRowsRequestFailure::Provider(_)
    ));
    assert!(failed.last_good.is_some());
    assert!(session.retry().is_some());
}

#[test]
fn network_escalation_session_drops_replaced_closed_and_duplicate_terminals() {
    let mut session = NetworkEscalationSession::default();

    let stale_attempt = session.begin_attempt();
    let active_attempt = session.begin_attempt();
    assert!(!session.accept_attempt(stale_attempt, request(40)));
    assert!(session.accept_attempt(active_attempt, request(41)));
    session.close();
    assert!(!session.complete(request(41)));

    let attempt = session.begin_attempt();
    assert!(session.accept_attempt(attempt, request(42)));
    assert!(session.complete(request(42)));
    assert!(!session.complete(request(42)));
    assert!(matches!(
        session.state(),
        NetworkEscalationState::Ready(ready) if ready.request_id == request(42)
    ));

    let attempt = session.begin_attempt();
    assert!(session.accept_attempt(attempt, request(43)));
    assert!(session.fail(request(43), FailureKind::PermissionDenied));
    assert!(session.retry().is_some());
}

#[test]
fn shell_ui_action_session_requires_matching_request_and_terminal_kind() {
    let mut session = ShellUiActionSession::default();
    let command = ShellUiActionIntent::Command(CommandLaunchRequest {
        command: "top".into(),
    });
    let reveal = ShellUiActionIntent::Reveal(ResourceRevealRequest {
        target: process(91, 901),
        cached_executable: None,
    });

    let stale_attempt = session.begin_attempt(command);
    let active_attempt = session.begin_attempt(reveal.clone());
    assert!(!session.accept_attempt(stale_attempt, request(50)));
    assert!(session.accept_attempt(active_attempt, request(51)));
    assert!(!session.complete(
        request(51),
        &CapabilityId::RESOURCE_REVEAL,
        &ShellEvent::CommandLaunched { pid: 4 },
    ));
    assert!(!session.complete(
        request(51),
        &CapabilityId::URL_OPEN,
        &ShellEvent::TargetOpened,
    ));
    assert!(session.complete(
        request(51),
        &CapabilityId::RESOURCE_REVEAL,
        &ShellEvent::TargetOpened,
    ));
    assert!(!session.complete(
        request(51),
        &CapabilityId::RESOURCE_REVEAL,
        &ShellEvent::TargetOpened,
    ));
    assert!(matches!(
        session.state(),
        ShellUiActionState::Ready(ready)
            if ready.intent == reveal && ready.receipt == ShellUiActionReceipt::TargetOpened
    ));

    let open_url = ShellUiActionIntent::OpenUrl(UrlOpenRequest {
        url: "https://example.invalid".into(),
    });
    let attempt = session.begin_attempt(open_url);
    assert!(session.accept_attempt(attempt, request(52)));
    assert!(!session.fail(
        request(52),
        &CapabilityId::RESOURCE_REVEAL,
        FailureKind::Rejected,
    ));
    assert!(session.fail(request(52), &CapabilityId::URL_OPEN, FailureKind::Rejected,));
    assert!(session.retry().is_some());
    session.close();
    assert!(!session.complete(
        request(52),
        &CapabilityId::URL_OPEN,
        &ShellEvent::TargetOpened,
    ));
}

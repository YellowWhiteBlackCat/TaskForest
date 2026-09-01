use super::*;
use taskmanager_core::core::ScalarObservation;
use taskmanager_core::core::process::{
    ProcessCategory, ProcessItem, ProcessLiveKey, ProcessScalarObservations,
};
use taskmanager_platform_contract::CapabilityStatus;

fn live_process(pid: u32, parent_pid: Option<u32>, token: u64) -> ProcessItem {
    let mut process = ProcessItem::new(pid, format!("process-{pid}"));
    process.parent_pid = parent_pid;
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(token, 1),
        ..ProcessScalarObservations::default()
    });
    process
}

fn live_key(pid: u32, token: u64) -> ProcessLiveKey {
    ProcessLiveKey::from_parts(pid, token).expect("authoritative fixture identity")
}

impl LatestProcessControlRequest {
    #[must_use]
    pub(crate) const fn pending(&self) -> Option<&PendingProcessControl> {
        self.pending.as_ref()
    }
}

fn target(pid: u32) -> FrozenProcessIdentity {
    target_with_token(pid, 7_500)
}

fn target_with_token(pid: u32, token: u64) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, "fixture-worker", 100, token)
        .expect("valid fixture identity")
}

fn request_id(value: u64) -> RequestId {
    RequestId::new(value).expect("non-zero fixture request id")
}

#[test]
fn control_completion_requires_request_id_and_complete_live_identity_echo() {
    let mut requests = LatestProcessControlRequest::default();
    requests.begin(request_id(7), target(42), ProcessControlKind::EndTask);

    // Wrong request id, correct identity: rejected.
    assert!(requests.accept(request_id(8), &target(42)).is_none());
    // Correct request id, wrong pid: rejected (fail-closed like GPUI).
    assert!(requests.accept(request_id(7), &target(99)).is_none());
    // Correct request id and pid, but a reused PID with a new start token:
    // rejected as a different live row.
    assert!(
        requests
            .accept(request_id(7), &target_with_token(42, 8_500))
            .is_none()
    );
    assert!(requests.pending().is_some());

    // Exact echo removes and returns the pending submission.
    let pending = requests
        .accept(request_id(7), &target(42))
        .expect("exact echo must be accepted");
    assert_eq!(pending.target.pid, 42);
    assert!(requests.pending().is_none());
    // A repeated stale completion cannot land twice.
    assert!(requests.accept(request_id(7), &target(42)).is_none());
}

#[test]
fn control_failure_takes_only_the_matching_request_id() {
    let mut requests = LatestProcessControlRequest::default();
    requests.begin(
        request_id(7),
        target(42),
        ProcessControlKind::Affinity(vec![0, 1]),
    );

    assert!(requests.take(request_id(8)).is_none());
    assert!(requests.pending().is_some());
    assert!(requests.take(request_id(7)).is_some());
    assert!(requests.pending().is_none());
}

#[test]
fn process_control_availability_has_one_target_scope_projection() {
    let processes = vec![
        live_process(1, None, 101),
        live_process(2, Some(1), 202),
        live_process(3, None, 303),
    ];
    let root = live_key(1, 101);
    let child = live_key(2, 202);

    assert_eq!(
        process_control_availability(
            &processes,
            Some(crate::ProcessRowId::Application(root)),
            &[],
            Some(CapabilityStatus::Available),
        ),
        ProcessControlAvailability::Ready {
            scope: ProcessControlScope::Tree,
            target_count: 2,
        }
    );
    assert_eq!(
        process_control_availability(
            &processes,
            Some(crate::ProcessRowId::Process(root)),
            &[],
            Some(CapabilityStatus::Available),
        ),
        ProcessControlAvailability::Ready {
            scope: ProcessControlScope::Single,
            target_count: 1,
        }
    );
    assert_eq!(
        process_control_availability(
            &processes,
            Some(crate::ProcessRowId::Process(root)),
            &[root, child],
            Some(CapabilityStatus::PermissionRequired),
        ),
        ProcessControlAvailability::Ready {
            scope: ProcessControlScope::Batch,
            target_count: 2,
        }
    );
    assert_eq!(
        process_control_availability(
            &processes,
            Some(crate::ProcessRowId::Category(ProcessCategory::Application)),
            &[],
            Some(CapabilityStatus::Available),
        ),
        ProcessControlAvailability::NoSelection
    );
}

#[test]
fn process_control_availability_fails_closed_for_stale_identity_or_capability() {
    let processes = [live_process(1, None, 101)];
    let live = live_key(1, 101);
    let reused = live_key(1, 999);

    assert_eq!(
        process_control_availability(
            &processes,
            Some(crate::ProcessRowId::Process(reused)),
            &[],
            Some(CapabilityStatus::Available),
        ),
        ProcessControlAvailability::IdentityUnavailable
    );
    assert_eq!(
        process_control_availability(
            &processes,
            Some(crate::ProcessRowId::Process(live)),
            &[reused],
            Some(CapabilityStatus::Available),
        ),
        ProcessControlAvailability::IdentityUnavailable,
        "a stale marked set must not fall back to the active row"
    );

    let unavailable = process_control_availability(
        &processes,
        Some(crate::ProcessRowId::Process(live)),
        &[],
        Some(CapabilityStatus::Unsupported),
    );
    assert_eq!(
        unavailable,
        ProcessControlAvailability::CapabilityUnavailable {
            status: Some(CapabilityStatus::Unsupported),
            scope: ProcessControlScope::Single,
            target_count: 1,
        }
    );
    assert!(!unavailable.is_ready());
    assert_eq!(unavailable.target_count(), 1);
    assert_eq!(unavailable.scope(), Some(ProcessControlScope::Single));
}

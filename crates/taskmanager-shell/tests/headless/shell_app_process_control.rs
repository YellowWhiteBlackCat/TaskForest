use super::*;

impl LatestProcessControlRequest {
    #[must_use]
    pub(crate) const fn pending(&self) -> Option<&PendingProcessControl> {
        self.pending.as_ref()
    }
}

fn target(pid: u32) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, "fixture-worker", 100, 7_500)
        .expect("valid fixture identity")
}

fn request_id(value: u64) -> RequestId {
    RequestId::new(value).expect("non-zero fixture request id")
}

#[test]
fn control_completion_requires_request_id_and_pid_echo() {
    let mut requests = LatestProcessControlRequest::default();
    requests.begin(request_id(7), target(42), ProcessControlKind::EndTask);

    // Wrong request id, correct pid: rejected.
    assert!(requests.accept(request_id(8), 42).is_none());
    // Correct request id, wrong pid: rejected (fail-closed like GPUI).
    assert!(requests.accept(request_id(7), 99).is_none());
    assert!(requests.pending().is_some());

    // Exact echo removes and returns the pending submission.
    let pending = requests
        .accept(request_id(7), 42)
        .expect("exact echo must be accepted");
    assert_eq!(pending.target.pid, 42);
    assert!(requests.pending().is_none());
    // A repeated stale completion cannot land twice.
    assert!(requests.accept(request_id(7), 42).is_none());
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

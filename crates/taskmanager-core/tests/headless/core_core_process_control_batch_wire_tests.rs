use super::{
    FailureKind, FrozenProcessIdentity, ProcessBatchIntent, ProcessBatchResult,
    ProcessBatchTargetResult, ProcessGroupScope, ProcessItem,
    parse_process_batch_failure_wire_code, process_batch_failure_wire_code,
};
use crate::core::metrics::ScalarObservation;
use crate::core::process::ProcessBatchAction;

fn live(pid: u32) -> ProcessItem {
    let mut item = ProcessItem::new(pid, "fixture");
    item.apply_scalar_observations(crate::core::ProcessScalarObservations {
        start_token: ScalarObservation::available(7_500, pid as u64 * 10),
        ..crate::core::ProcessScalarObservations::default()
    });
    item
}

#[test]
fn every_failure_kind_round_trips_through_its_wire_code() {
    // Driven by the forward mapper so the code table can never drift.
    for kind in [
        FailureKind::Unsupported,
        FailureKind::PermissionDenied,
        FailureKind::MissingDependency,
        FailureKind::TimedOut,
        FailureKind::IdentityChanged,
        FailureKind::TemporarilyUnavailable,
        FailureKind::Rejected,
        FailureKind::ProviderFault,
    ] {
        let code = process_batch_failure_wire_code(kind);
        assert_eq!(
            parse_process_batch_failure_wire_code(code),
            Some(kind),
            "wire code {code:?} must decode back to {kind:?}"
        );
    }
    // RequiresEscalation is the one intentional one-way fold: the legacy
    // schema has no escalation token, so it encodes as permission_denied
    // and decodes as the plain denial.
    assert_eq!(
        process_batch_failure_wire_code(FailureKind::RequiresEscalation),
        "permission_denied"
    );
    assert_eq!(parse_process_batch_failure_wire_code("no-such-code"), None);
    assert_eq!(parse_process_batch_failure_wire_code(""), None);
}

#[test]
fn pid_zero_never_authorizes_even_with_a_token() {
    // The pid > 0 guard: a zero pid with a start token is invalid input
    // (a `>`→`>=` mutation of the guard would authorize it).
    let zero = FrozenProcessIdentity::from_authoritative_parts(0, "x", 1, 1);
    assert_eq!(
        zero.and_then(|f| f.authoritative_start_token()),
        None,
        "pid 0 must fail closed"
    );
}

#[test]
fn deserialized_pid_zero_with_a_token_still_fails_closed() {
    // serde does not pre-validate pid, so a pid==0 + Some(token) payload
    // can be decoded — the in-method guard is the last line of defense
    // (a `guard → true` mutation would authorize this payload).
    let decoded: FrozenProcessIdentity =
        serde_json::from_str(r#"{"pid":0,"name":"x","start_time_secs":1,"start_token":5}"#)
            .expect("schema-v2 payload with zero pid decodes");
    assert_eq!(
        decoded.authoritative_start_token(),
        None,
        "decoded pid 0 must never authorize"
    );

    let legit: FrozenProcessIdentity = serde_json::from_str(
        r#"{"pid":42,"name":"worker","start_time_secs":1720000000,"start_token":7500}"#,
    )
    .expect("normal payload decodes");
    assert_eq!(legit.authoritative_start_token(), Some(7_500));
}

#[test]
fn freeze_keeps_only_live_selected_pids_in_sorted_order() {
    let processes = [live(2), live(1), live(3)];
    let intent = ProcessBatchIntent::freeze(
        &processes,
        [3, 1, 3, 99], // 99 does not exist; 3 duplicated
        ProcessBatchAction::Suspend,
    );
    let pids: Vec<_> = intent.targets.iter().map(|f| f.pid).collect();
    assert_eq!(pids, vec![1, 3], "only live pids, deduped, sorted");
}

#[test]
fn applied_count_counts_only_applied_targets() {
    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::Kill,
        scope: ProcessGroupScope::PidAdjacency,
        targets: vec![
            FrozenProcessIdentity::from_authoritative_parts(1, "a", 1, 1).expect("fixture"),
        ],
    };
    let result = ProcessBatchResult {
        intent,
        targets: vec![
            (
                FrozenProcessIdentity::from_authoritative_parts(1, "a", 1, 1).expect("f"),
                ProcessBatchTargetResult::Applied,
            ),
            (
                FrozenProcessIdentity::from_authoritative_parts(2, "b", 1, 1).expect("f"),
                ProcessBatchTargetResult::Applied,
            ),
            (
                FrozenProcessIdentity::from_authoritative_parts(3, "c", 1, 1).expect("f"),
                ProcessBatchTargetResult::Failed(FailureKind::TimedOut),
            ),
        ],
    };
    assert_eq!(
        result.applied_count(),
        2,
        "must count every applied target (a `→ 1` mutation is caught by using two)"
    );
}

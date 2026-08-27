use super::*;
use crate::core::{PriorityTier, ProcessScalarObservations, ScalarObservation};

fn identity(pid: u32, name: &str) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(
        pid,
        name,
        u64::from(pid) * 10,
        u64::from(pid) * 100,
    )
    .expect("authoritative fixture identity")
}

fn live_process(target: &FrozenProcessIdentity, start_token: u64) -> ProcessItem {
    let mut process = ProcessItem::new(target.pid, target.name.clone());
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(start_token, 10),
        start_time_secs: ScalarObservation::available(target.start_time_secs, 10),
        ..ProcessScalarObservations::default()
    });
    process
}

fn result(
    completed_pid: u32,
    action: ProcessBatchAction,
    outcome: ProcessBatchTargetResult,
) -> ProcessBatchResult {
    let target = identity(completed_pid, &format!("process-{completed_pid}"));
    ProcessBatchResult {
        intent: ProcessBatchIntent {
            action,
            scope: Default::default(),
            targets: vec![target.clone()],
        },
        targets: vec![(target, outcome)],
    }
}

#[test]
fn bounded_history_evicts_the_oldest_completed_batch() {
    let mut history = ProcessBatchHistory::new(2);
    assert!(history.record_result(
        100,
        result(
            1,
            ProcessBatchAction::Suspend,
            ProcessBatchTargetResult::Applied
        ),
    ));
    assert!(history.record_result(
        200,
        result(
            2,
            ProcessBatchAction::Resume,
            ProcessBatchTargetResult::Applied
        ),
    ));
    assert!(history.record_result(
        300,
        result(
            3,
            ProcessBatchAction::End,
            ProcessBatchTargetResult::Applied
        ),
    ));

    assert_eq!(history.len(), 2);
    assert_eq!(history.entries()[0].completed_at_unix_ms, 200);
    assert_eq!(history.entries()[1].targets[0].identity.pid, 3);
}

#[test]
fn empty_history_has_stable_json_and_header_only_csv_exports() {
    let history = ProcessBatchHistory::default();
    assert_eq!(
        export_process_batch_history(&history, ProcessBatchHistoryFormat::Json)
            .expect("fixed audit DTO must serialize"),
        "{\n  \"schema_version\": 1,\n  \"entries\": []\n}\n"
    );
    assert_eq!(
        export_process_batch_history(&history, ProcessBatchHistoryFormat::Csv)
            .expect("CSV export is infallible"),
        "schema_version,completed_at_unix_ms,action,priority,target_index,target_count,pid,name,start_time_secs,result,error\n"
    );
}

#[test]
fn partial_failure_export_keeps_v1_shape_and_uses_stable_failure_code() {
    let first = identity(7, "alpha,worker");
    let second = identity(8, "beta");
    let third = identity(9, "gamma");
    let mut history = ProcessBatchHistory::new(4);
    history.record_result(
        1_700_000_000_123,
        ProcessBatchResult {
            intent: ProcessBatchIntent {
                action: ProcessBatchAction::SetPriority(PriorityTier::High),
                scope: Default::default(),
                targets: vec![first.clone(), second.clone(), third.clone()],
            },
            targets: vec![
                (first, ProcessBatchTargetResult::Applied),
                (
                    second,
                    ProcessBatchTargetResult::Failed(FailureKind::PermissionDenied),
                ),
                (third, ProcessBatchTargetResult::IdentityChanged),
            ],
        },
    );

    let json = export_process_batch_history(&history, ProcessBatchHistoryFormat::Json)
        .expect("fixed audit DTO must serialize");
    assert_eq!(
        json,
        export_process_batch_history(&history, ProcessBatchHistoryFormat::Json)
            .expect("repeated serialization must succeed")
    );
    assert!(json.contains("\"kind\": \"set_priority\""));
    assert!(
        json.contains("\"priority\": -10"),
        "the High tier must export its canonical nice value"
    );
    assert!(json.contains("\"status\": \"failed\""));
    assert!(json.contains("\"error\": \"permission_denied\""));
    assert!(json.contains("\"schema_version\": 1"));

    let csv = export_process_batch_history(&history, ProcessBatchHistoryFormat::Csv)
        .expect("CSV export is infallible");
    assert!(csv.contains("\"alpha,worker\""));
    assert!(csv.contains(",failed,permission_denied\n"));
    assert!(csv.contains(",identity_changed,"));
    assert_eq!(csv.lines().count(), 4);
    assert!(csv.starts_with(
            "schema_version,completed_at_unix_ms,action,priority,target_index,target_count,pid,name,start_time_secs,result,error\n"
        ));
}

#[test]
fn typed_failures_serialize_to_stable_tokens_without_changing_outer_shape() {
    let cases = [
        (FailureKind::PermissionDenied, "permission_denied"),
        (FailureKind::IdentityChanged, "not_found_or_reused"),
        (FailureKind::Unsupported, "unsupported"),
        (FailureKind::MissingDependency, "missing_dependency"),
        (FailureKind::TimedOut, "timed_out"),
        (FailureKind::TemporarilyUnavailable, "provider_unavailable"),
        (FailureKind::Rejected, "rejected"),
        (FailureKind::ProviderFault, "other"),
    ];

    for (failure, token) in cases {
        assert_eq!(process_batch_failure_wire_code(failure), token);
        assert_eq!(
            serde_json::to_string(&ProcessBatchTargetResult::Failed(failure))
                .expect("typed target result must serialize"),
            format!(r#"{{"Failed":"{token}"}}"#)
        );
    }
}

#[test]
fn legacy_and_canonical_failure_tokens_deserialize_to_shared_kinds() {
    let cases = [
        ("not_found_or_reused", FailureKind::IdentityChanged),
        ("identity_changed", FailureKind::IdentityChanged),
        ("provider_unavailable", FailureKind::TemporarilyUnavailable),
        (
            "temporarily_unavailable",
            FailureKind::TemporarilyUnavailable,
        ),
        ("other", FailureKind::ProviderFault),
        ("provider_fault", FailureKind::ProviderFault),
        ("missing_dependency", FailureKind::MissingDependency),
        ("timed_out", FailureKind::TimedOut),
        ("rejected", FailureKind::Rejected),
    ];
    for (token, expected) in cases {
        let serialized = format!(r#"{{"Failed":"{token}"}}"#);
        assert_eq!(
            serde_json::from_str::<ProcessBatchTargetResult>(&serialized)
                .expect("known legacy/canonical target failure should deserialize"),
            ProcessBatchTargetResult::Failed(expected)
        );
    }
}

#[test]
fn executor_preserves_every_shared_failure_kind_without_translation() {
    let target = identity(20, "typed-target");
    let live = [live_process(
        &target,
        target
            .authoritative_start_token()
            .expect("fixture start token"),
    )];
    for failure in [
        FailureKind::Unsupported,
        FailureKind::PermissionDenied,
        FailureKind::MissingDependency,
        FailureKind::TimedOut,
        FailureKind::IdentityChanged,
        FailureKind::TemporarilyUnavailable,
        FailureKind::Rejected,
        FailureKind::ProviderFault,
    ] {
        let result = execute_process_batch_with(
            ProcessBatchIntent {
                action: ProcessBatchAction::End,
                scope: Default::default(),
                targets: vec![target.clone()],
            },
            &live,
            |_action, _pid| Err(failure),
        );
        assert_eq!(
            result.targets[0].1,
            ProcessBatchTargetResult::Failed(failure)
        );
    }
}

#[test]
fn pid_reuse_is_rejected_before_the_executor_runs() {
    let frozen = identity(20, "same-name");
    let live = [live_process(
        &frozen,
        frozen
            .authoritative_start_token()
            .expect("fixture start token")
            + 1,
    )];
    let result = execute_process_batch_with(
        ProcessBatchIntent {
            action: ProcessBatchAction::Kill,
            scope: Default::default(),
            targets: vec![frozen],
        },
        &live,
        |_action, _pid| panic!("PID-reused target must not reach native control"),
    );

    assert_eq!(
        result.targets[0].1,
        ProcessBatchTargetResult::IdentityChanged
    );
}

#[test]
fn execution_result_is_recorded_automatically_including_partial_failure() {
    let targets = vec![identity(21, "first"), identity(22, "second")];
    let live = targets
        .iter()
        .map(|target| {
            live_process(
                target,
                target
                    .authoritative_start_token()
                    .expect("fixture start token"),
            )
        })
        .collect::<Vec<_>>();
    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::Suspend,
        scope: Default::default(),
        targets,
    };
    let mut history = ProcessBatchHistory::new(8);

    let result = execute_process_batch_recording_with(
        &mut history,
        456,
        intent,
        &live,
        |_action, target| {
            if target.pid == 22 {
                Err(FailureKind::PermissionDenied)
            } else {
                Ok(())
            }
        },
    );

    assert_eq!(result.applied_count(), 1);
    assert_eq!(history.len(), 1);
    assert_eq!(history.entries()[0].completed_at_unix_ms, 456);
    assert!(matches!(
        history.entries()[0].targets[1].result,
        ProcessBatchTargetResult::Failed(FailureKind::PermissionDenied)
    ));
}

#[test]
fn zero_capacity_history_is_inert() {
    let mut history = ProcessBatchHistory::new(0);
    assert!(!history.record_result(
        100,
        result(
            1,
            ProcessBatchAction::End,
            ProcessBatchTargetResult::Applied
        ),
    ));
    assert!(history.is_empty());
}

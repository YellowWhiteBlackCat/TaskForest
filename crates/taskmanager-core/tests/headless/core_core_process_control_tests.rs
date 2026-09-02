use super::*;
use crate::core::{ProcessScalarObservations, ScalarObservation};

fn live_process(token: u64) -> ProcessItem {
    ProcessItem::new(42, "worker").with_scalar_observations(ProcessScalarObservations {
        start_time_secs: ScalarObservation::available(1_720_000_000, 10),
        start_token: ScalarObservation::available(token, 10),
        ..ProcessScalarObservations::default()
    })
}

fn live_key(pid: u32, token: u64) -> ProcessLiveKey {
    ProcessLiveKey::from_parts(pid, token).expect("fixture live identity")
}

#[test]
fn priority_tier_all_covers_every_variant_exactly_once() {
    let mut seen: Vec<PriorityTier> = Vec::new();
    for tier in PriorityTier::ALL {
        assert!(
            seen.iter().all(|seen_tier| seen_tier != &tier),
            "ALL must not repeat {tier:?}"
        );
        seen.push(tier);
    }
    // The snake_case wire vocabulary is exactly one token per variant, so
    // a missing variant has no ALL entry and a duplicated one is caught
    // above.
    for tier in seen {
        let wire = serde_json::to_string(&tier).expect("tier serializes");
        let decoded: PriorityTier = serde_json::from_str(&wire).expect("tier wire decodes");
        assert_eq!(decoded, tier);
    }
    assert_eq!(
        serde_json::to_string(&PriorityTier::Low).expect("serialize low"),
        "\"low\""
    );
    assert_eq!(PriorityTier::default(), PriorityTier::Normal);
}

#[test]
fn priority_tier_keys_and_canonical_nice_are_stable() {
    let keys: Vec<&'static str> = PriorityTier::ALL.iter().map(|t| t.i18n_key()).collect();
    assert_eq!(keys, ["proc.high", "proc.normal", "proc.low"]);
    let nice: Vec<i32> = PriorityTier::ALL
        .iter()
        .map(|t| t.canonical_nice())
        .collect();
    assert_eq!(nice, [-10, 0, 10]);
}

#[test]
fn new_freeze_requires_exact_current_start_token() {
    let frozen =
        FrozenProcessIdentity::from_process(&live_process(7_500)).expect("current identity");
    assert_eq!(frozen.authoritative_start_token(), Some(7_500));

    let mut unavailable = live_process(7_500);
    unavailable.scalar_observations.start_token =
        ScalarObservation::unavailable(FailureKind::IdentityChanged);
    assert_eq!(FrozenProcessIdentity::from_process(&unavailable), None);
    assert_eq!(
        FrozenProcessIdentity::from_authoritative_parts(42, "worker", 10, 0),
        None
    );
}

#[test]
fn freeze_tree_orders_descendants_leaf_first_and_fails_closed_for_missing_root() {
    let item = |pid, parent_pid, token| {
        let mut item = ProcessItem::new(pid, format!("p{pid}"));
        item.parent_pid = parent_pid;
        item.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(token, 1),
            ..ProcessScalarObservations::default()
        });
        item
    };
    let processes = vec![
        item(1, None, 11),
        item(2, Some(1), 22),
        item(3, Some(2), 33),
        item(4, Some(1), 44),
    ];

    let intent =
        ProcessBatchIntent::freeze_tree(&processes, live_key(1, 11), ProcessBatchAction::End);
    assert_eq!(
        intent
            .targets
            .iter()
            .map(|target| target.pid)
            .collect::<Vec<_>>(),
        vec![3, 2, 4, 1]
    );

    let missing =
        ProcessBatchIntent::freeze_tree(&processes, live_key(99, 1), ProcessBatchAction::End);
    assert!(missing.targets.is_empty());
}

/// 同一律 for the tree traversal: `freeze_tree` targets are exactly
/// [`descendant_live_keys`] is the SAME leaf-first
/// order — the order logic has one home, so the two can never drift.
#[test]
fn descendant_live_keys_is_the_one_order_freeze_tree_freezes() {
    let item = |pid, parent_pid, token| {
        let mut item = ProcessItem::new(pid, format!("p{pid}"));
        item.parent_pid = parent_pid;
        item.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(token, 1),
            ..ProcessScalarObservations::default()
        });
        item
    };
    let processes = vec![
        item(1, None, 11),
        item(2, Some(1), 22),
        item(3, Some(2), 33),
        item(4, Some(1), 44),
        item(9, None, 99),
    ];

    let identities = descendant_live_keys(&processes, live_key(1, 11));
    assert_eq!(
        identities
            .iter()
            .map(|identity| identity.pid())
            .collect::<Vec<_>>(),
        vec![3, 2, 4, 1]
    );
    let intent =
        ProcessBatchIntent::freeze_tree(&processes, live_key(1, 11), ProcessBatchAction::End);
    assert_eq!(
        intent
            .targets
            .iter()
            .map(|target| target.pid)
            .collect::<Vec<_>>(),
        identities
            .iter()
            .map(|identity| identity.pid())
            .collect::<Vec<_>>(),
        "freeze_tree must freeze descendant_live_keys' exact leaf-first order"
    );

    // Cyclic parent chains stay total and duplicate-free.
    let cycle = vec![
        item(10, Some(12), 1010),
        item(11, Some(10), 1011),
        item(12, Some(11), 1012),
    ];
    assert_eq!(
        descendant_live_keys(&cycle, live_key(10, 1010))
            .iter()
            .map(|identity| identity.pid())
            .collect::<Vec<_>>(),
        vec![12, 11, 10]
    );

    // An unknown root fails closed in both spellings.
    assert!(descendant_live_keys(&processes, live_key(99, 1)).is_empty());
    assert!(
        ProcessBatchIntent::freeze_tree(&processes, live_key(99, 1), ProcessBatchAction::End)
            .targets
            .is_empty()
    );
}

#[test]
fn freeze_tree_fails_closed_when_a_snapshot_repeats_a_pid() {
    let item = |pid, parent_pid, token| {
        let mut item = ProcessItem::new(pid, format!("p{pid}-{token}"));
        item.parent_pid = parent_pid;
        item.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(token, 1),
            ..ProcessScalarObservations::default()
        });
        item
    };
    let processes = vec![
        item(1, None, 11),
        // A duplicate PID makes the parent_pid graph ambiguous. In
        // particular, a plain PID map must not replace the selected root's
        // exact start token with this later incarnation.
        item(1, None, 12),
        item(2, Some(1), 22),
    ];
    let root = live_key(1, 11);

    assert!(
        descendant_live_keys(&processes, root).is_empty(),
        "ambiguous PID topology must not produce actionable identities"
    );
    assert!(
        ProcessBatchIntent::freeze_tree(&processes, root, ProcessBatchAction::Kill)
            .targets
            .is_empty(),
        "tree freezing must fail closed on duplicate provider rows"
    );
}

#[test]
fn freeze_and_freeze_tree_default_to_pid_adjacency_scope() {
    let processes = [live_process(7_500)];
    let identity = live_key(42, 7_500);
    let batch = ProcessBatchIntent::freeze(&processes, [identity], ProcessBatchAction::End);
    let tree = ProcessBatchIntent::freeze_tree(&processes, identity, ProcessBatchAction::End);
    assert_eq!(batch.scope, ProcessGroupScope::PidAdjacency);
    assert_eq!(tree.scope, ProcessGroupScope::PidAdjacency);
    assert_eq!(
        ProcessBatchIntent::freeze_tree(&processes, live_key(99, 1), ProcessBatchAction::End).scope,
        ProcessGroupScope::PidAdjacency,
        "the fail-closed empty intent also stays PidAdjacency"
    );
}

#[test]
fn group_scope_wire_tags_are_snake_case_and_round_trip() {
    assert_eq!(
        serde_json::to_string(&ProcessGroupScope::PidAdjacency).expect("serialize"),
        r#"{"kind":"pid_adjacency"}"#
    );
    let native = ProcessGroupScope::NativeGroup {
        family: "cgroup.v2".to_owned(),
        locator: "/unified.slice/app.scope".to_owned(),
    };
    let wire = serde_json::to_string(&native).expect("serialize native group");
    assert_eq!(
        wire,
        r#"{"kind":"native_group","family":"cgroup.v2","locator":"/unified.slice/app.scope"}"#
    );
    let decoded: ProcessGroupScope = serde_json::from_str(&wire).expect("decode native group");
    assert_eq!(decoded, native);
    assert_eq!(
        ProcessGroupScope::default(),
        ProcessGroupScope::PidAdjacency
    );
}

#[test]
fn intent_scope_is_wire_compatible_with_payloads_missing_the_field() {
    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::End,
        scope: ProcessGroupScope::NativeGroup {
            family: "job_object".to_owned(),
            locator: "{88abd32a-1e14-4b1f-9c0d-0d9e7a4b6c21}".to_owned(),
        },
        targets: vec![
            FrozenProcessIdentity::from_authoritative_parts(42, "worker", 1, 7_500)
                .expect("fixture"),
        ],
    };
    let mut wire: serde_json::Value = serde_json::to_value(&intent).expect("intent serializes");
    assert_eq!(
        wire.get("scope").and_then(|scope| scope.get("kind")),
        Some(&serde_json::json!("native_group")),
        "a scope-carrying intent keeps the internally tagged shape"
    );
    wire.as_object_mut()
        .expect("intent wire is an object")
        .remove("scope");
    let legacy: ProcessBatchIntent =
        serde_json::from_value(wire).expect("legacy payload without scope decodes");
    assert_eq!(legacy.scope, ProcessGroupScope::PidAdjacency);
    assert_eq!(legacy.targets.len(), 1);
}

#[test]
fn schema_v1_payload_reads_but_never_authorizes_control() {
    let legacy: FrozenProcessIdentity =
        serde_json::from_str(r#"{"pid":42,"name":"worker","start_time_secs":1720000000}"#)
            .expect("schema-v1 frozen identity");
    assert_eq!(legacy.authoritative_start_token(), None);

    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::Kill,
        scope: ProcessGroupScope::PidAdjacency,
        targets: vec![legacy.clone()],
    };
    let mut executed = false;
    let result = execute_process_batch_with(intent, &[live_process(7_500)], |_, _| {
        executed = true;
        Ok(())
    });
    assert!(!executed);
    assert_eq!(
        result.targets,
        vec![(legacy, ProcessBatchTargetResult::IdentityUnavailable)]
    );
}

#[test]
fn schema_v2_payload_round_trips_exact_token() {
    let frozen =
        FrozenProcessIdentity::from_process(&live_process(7_500)).expect("current identity");
    let json = serde_json::to_string(&frozen).expect("serialize frozen identity");
    assert!(json.contains(r#""start_token":7500"#));
    let decoded: FrozenProcessIdentity =
        serde_json::from_str(&json).expect("deserialize frozen identity");
    assert_eq!(decoded, frozen);
}

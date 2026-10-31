use super::*;
use crate::core::process::{ProcessBatchAction, ProcessBatchIntent};
use crate::core::{FailureKind, ProcessScalarObservations, ScalarObservation};

fn live_process(pid: u32, parent_pid: Option<u32>, token: u64) -> ProcessItem {
    let mut process = ProcessItem::new(pid, format!("p{pid}"));
    process.parent_pid = parent_pid;
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(token, 1),
        ..ProcessScalarObservations::default()
    });
    process
}

fn unidentified_process(pid: u32, parent_pid: Option<u32>) -> ProcessItem {
    let mut process = ProcessItem::new(pid, format!("p{pid}"));
    process.parent_pid = parent_pid;
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::unavailable(FailureKind::IdentityChanged),
        ..ProcessScalarObservations::default()
    });
    process
}

fn live_key(pid: u32, token: u64) -> ProcessLiveKey {
    ProcessLiveKey::from_parts(pid, token).expect("fixture live identity")
}

fn drawn(intent: &ProcessBatchIntent) -> Vec<(u32, u32)> {
    intent
        .collateral_rows()
        .iter()
        .map(|row| (row.pid(), row.depth()))
        .collect()
}

fn executed(intent: &ProcessBatchIntent) -> Vec<u32> {
    intent.targets.iter().map(|target| target.pid).collect()
}

#[test]
fn frozen_collateral_names_the_root_and_every_descendant_with_its_depth() {
    let processes = vec![
        live_process(1, None, 11),
        live_process(2, Some(1), 22),
        live_process(3, Some(2), 33),
        live_process(4, Some(1), 44),
    ];

    let intent = ProcessBatchIntent::freeze_tree(
        &processes,
        live_key(1, 11),
        ProcessBatchAction::EndProcessTree,
    );

    assert_eq!(
        drawn(&intent),
        vec![(1, 0), (2, 1), (3, 2), (4, 1)],
        "the frozen closure starts at the root and carries every descendant's depth"
    );
    assert_eq!(
        intent.collateral.root().map(|root| root.pid),
        Some(1),
        "the frozen root is the tree root, not the first executed target"
    );
    assert_eq!(intent.collateral_rows()[1].name(), "p2");
    assert_eq!(intent.collateral.len(), 4);
    assert!(!intent.collateral.is_empty());
}

#[test]
fn frozen_collateral_of_a_root_without_descendants_holds_only_the_root() {
    let processes = vec![live_process(7, None, 77)];

    let intent =
        ProcessBatchIntent::freeze_tree(&processes, live_key(7, 77), ProcessBatchAction::End);

    assert_eq!(drawn(&intent), vec![(7, 0)]);
    assert_eq!(intent.collateral.root().map(|root| root.pid), Some(7));
    assert_eq!(
        executed(&intent),
        vec![7],
        "a root without descendants is still one exact execution target"
    );
}

#[test]
fn frozen_collateral_of_an_unknown_root_is_empty() {
    let processes = vec![live_process(1, None, 11)];

    let intent =
        ProcessBatchIntent::freeze_tree(&processes, live_key(99, 1), ProcessBatchAction::End);

    assert!(
        intent.collateral_rows().is_empty(),
        "an unknown root has no honest collateral to draw"
    );
    assert!(intent.collateral.root().is_none());
    assert!(intent.targets.is_empty());
}

#[test]
fn frozen_collateral_survives_a_later_refresh_of_the_live_rows() {
    let mut processes = vec![
        live_process(1, None, 11),
        live_process(2, Some(1), 22),
        live_process(3, Some(2), 33),
    ];
    let intent = ProcessBatchIntent::freeze_tree(
        &processes,
        live_key(1, 11),
        ProcessBatchAction::EndProcessTree,
    );
    let frozen = drawn(&intent);
    assert_eq!(frozen, vec![(1, 0), (2, 1), (3, 2)]);

    // The provider refreshes between arming and rendering: the child exits
    // and a different process appears under the same root.
    processes.retain(|process| process.pid != 2);
    processes.push(live_process(4, Some(1), 44));

    assert_eq!(
        drawn(&intent),
        frozen,
        "the frozen closure must not follow a later refresh of the live rows"
    );
    assert_eq!(executed(&intent), vec![3, 2, 1]);

    // The refresh really changed the live topology, so the frozen view is
    // not vacuously equal to a live re-read.
    let refreshed = ProcessBatchIntent::freeze_tree(
        &processes,
        live_key(1, 11),
        ProcessBatchAction::EndProcessTree,
    );
    assert_eq!(drawn(&refreshed), vec![(1, 0), (4, 1)]);
    assert_ne!(drawn(&refreshed), frozen);
}

#[test]
fn frozen_collateral_is_exactly_the_frozen_execution_set() {
    let processes = vec![
        live_process(1, None, 11),
        live_process(2, Some(1), 22),
        live_process(3, Some(2), 33),
        live_process(9, None, 99),
    ];

    let intent = ProcessBatchIntent::freeze_tree(
        &processes,
        live_key(1, 11),
        ProcessBatchAction::EndProcessTree,
    );

    let mut targets = executed(&intent);
    let mut collateral: Vec<u32> = intent
        .collateral_rows()
        .iter()
        .map(|row| row.pid())
        .collect();
    targets.sort_unstable();
    collateral.sort_unstable();
    assert_eq!(
        collateral, targets,
        "the collateral view never introduces a target the execution set lacks, nor drops one"
    );
    assert_eq!(
        targets,
        vec![1, 2, 3],
        "the unrelated row is not collateral"
    );
}

#[test]
fn the_drawn_closure_and_the_execution_order_come_from_one_expansion() {
    let processes = vec![
        live_process(1, None, 11),
        live_process(2, Some(1), 22),
        live_process(3, Some(2), 33),
        live_process(4, Some(1), 44),
    ];

    let intent = ProcessBatchIntent::freeze_tree(
        &processes,
        live_key(1, 11),
        ProcessBatchAction::EndProcessTree,
    );

    assert_eq!(
        executed(&intent),
        vec![3, 2, 4, 1],
        "the execution set ends the deepest branch before its parent"
    );
    assert_eq!(
        drawn(&intent),
        vec![(1, 0), (2, 1), (3, 2), (4, 1)],
        "the drawn closure starts at the root and keeps the same members"
    );
}

#[test]
fn a_descendant_without_an_exact_identity_is_absent_from_both_views() {
    let processes = vec![
        live_process(1, None, 11),
        unidentified_process(2, Some(1)),
        live_process(3, Some(2), 33),
    ];

    let intent = ProcessBatchIntent::freeze_tree(
        &processes,
        live_key(1, 11),
        ProcessBatchAction::EndProcessTree,
    );

    assert_eq!(
        drawn(&intent),
        vec![(1, 0), (3, 2)],
        "a row without an exact identity is not a collateral row, and the depth stays topological"
    );
    assert_eq!(
        executed(&intent),
        vec![3, 1],
        "the execution set drops the same row the collateral view drops"
    );
}

#[test]
fn a_selection_batch_carries_no_hierarchy() {
    let processes = vec![live_process(1, None, 11), live_process(2, Some(1), 22)];

    let intent =
        ProcessBatchIntent::freeze(&processes, [live_key(2, 22)], ProcessBatchAction::Kill);

    assert!(
        intent.collateral_rows().is_empty(),
        "a selection is not a tree, so it has no root and no depth to draw"
    );
    assert!(intent.collateral.root().is_none());
    assert_eq!(executed(&intent), vec![2]);
}

#[test]
fn the_frozen_collateral_round_trips_through_the_intent_wire() {
    let processes = vec![live_process(1, None, 11), live_process(2, Some(1), 22)];
    let intent = ProcessBatchIntent::freeze_tree(
        &processes,
        live_key(1, 11),
        ProcessBatchAction::EndProcessTree,
    );

    let wire = serde_json::to_string(&intent).expect("the intent serializes");
    let decoded: ProcessBatchIntent = serde_json::from_str(&wire).expect("the intent decodes");
    assert_eq!(decoded, intent);
    assert_eq!(drawn(&decoded), vec![(1, 0), (2, 1)]);

    // A payload without the frozen view decodes to an empty view: there is no
    // hierarchy to draw, which is a valid state and not an error.
    let mut bare: serde_json::Value = serde_json::to_value(&intent).expect("the intent encodes");
    bare.as_object_mut()
        .expect("the intent wire is an object")
        .remove("collateral");
    let decoded: ProcessBatchIntent =
        serde_json::from_value(bare).expect("a payload without the view decodes");
    assert!(decoded.collateral_rows().is_empty());
    assert_eq!(decoded.targets.len(), intent.targets.len());
}

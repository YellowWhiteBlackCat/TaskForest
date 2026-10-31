//! The shell's frozen process-batch gate carries the indented closure.
//!
//! The shell freezes the intent every confirmation surface renders. The
//! frozen view must name exactly the processes the batch will end, carry
//! their depth below the root, and survive a later refresh of the live rows
//! — the frozen-gate contract.

use super::*;
use taskforest_core::core::ScalarObservation;
use taskforest_core::core::process::ProcessScalarObservations;

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

fn drawn(intent: &ProcessBatchIntent) -> Vec<(u32, u32)> {
    intent
        .collateral_rows()
        .iter()
        .map(|row| (row.pid(), row.depth()))
        .collect()
}

fn tree_intent(processes: &[ProcessItem], root: ProcessLiveKey) -> ProcessBatchIntent {
    process_control_intent(
        processes,
        Some(ProcessRowId::Application(root)),
        &[],
        ProcessBatchAction::End,
    )
    .expect("the tree intent is frozen")
}

#[test]
fn the_frozen_tree_intent_carries_the_indented_closure() {
    let processes = vec![
        live_process(1, None, 101),
        live_process(2, Some(1), 202),
        live_process(3, Some(2), 303),
        live_process(4, Some(1), 404),
    ];

    let intent = tree_intent(&processes, live_key(1, 101));

    assert_eq!(
        intent.action,
        ProcessBatchAction::EndProcessTree,
        "an application row keeps the tree verb"
    );
    assert_eq!(
        drawn(&intent),
        vec![(1, 0), (2, 1), (3, 2), (4, 1)],
        "the frozen closure starts at the root and carries every descendant's depth"
    );
    assert_eq!(intent.collateral.root().map(|root| root.pid), Some(1));
    assert_eq!(
        intent
            .targets
            .iter()
            .map(|target| target.pid)
            .collect::<Vec<_>>(),
        vec![3, 2, 4, 1],
        "the execution set ends the deepest branch before its parent"
    );
}

#[test]
fn the_frozen_tree_intent_ignores_a_later_refresh_of_the_live_rows() {
    let mut processes = vec![live_process(1, None, 101), live_process(2, Some(1), 202)];

    let intent = tree_intent(&processes, live_key(1, 101));
    let frozen = drawn(&intent);
    assert_eq!(frozen, vec![(1, 0), (2, 1)]);

    // The provider refreshes between arming and rendering: the child exits
    // and a different process appears under the same root.
    processes.retain(|process| process.pid != 2);
    processes.push(live_process(7, Some(1), 707));

    assert_eq!(
        drawn(&intent),
        frozen,
        "the frozen closure must not follow a later refresh of the live rows"
    );
    assert_eq!(
        drawn(&tree_intent(&processes, live_key(1, 101))),
        vec![(1, 0), (7, 1)],
        "the refreshed rows really changed the live topology"
    );
}

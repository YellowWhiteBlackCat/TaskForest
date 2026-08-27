use std::collections::HashSet;

use super::{ProcessTerminationAction, snapshot_process_tree};
use crate::core::ScalarObservation;
use crate::core::process::{ProcessItem, ProcessScalarObservations};

fn process(pid: u32, parent_pid: Option<u32>, name: &str, start: u64) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .parent_pid(parent_pid)
        .name(name.into())
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(start * 10, 1),
            ..ProcessScalarObservations::default()
        })
        .current_start_time_secs(start)
        .build()
}

#[test]
fn tree_snapshot_is_leaf_first_deterministic_and_frozen() {
    let mut processes = vec![
        process(1, None, "root", 100),
        process(2, Some(1), "child-a", 200),
        process(3, Some(1), "child-b", 300),
        process(4, Some(2), "leaf", 400),
        process(9, None, "unrelated", 900),
    ];

    let intent = snapshot_process_tree(&processes, 1).expect("complete frozen tree");
    assert_eq!(intent.action, ProcessTerminationAction::EndProcessTree);
    assert_eq!(intent.root.name, "root");
    assert_eq!(intent.root.start_time_secs, 100);
    assert_eq!(intent.execution_pids(), vec![4, 2, 3, 1]);
    assert_eq!(intent.descendant_count(), 3);

    // A later live-list refresh cannot rename, remove, or add targets to the
    // confirmation snapshot.
    processes[1].name = "reused-name".into();
    processes.retain(|item| item.pid != 4);
    processes.push(process(5, Some(1), "late-child", 500));
    assert_eq!(intent.execution_pids(), vec![4, 2, 3, 1]);
    assert_eq!(intent.descendants_leaf_first[1].name, "child-a");
}

#[test]
fn tree_snapshot_terminates_on_cycles_and_never_duplicates_root() {
    let processes = vec![
        process(10, Some(12), "root", 10),
        process(11, Some(10), "child", 11),
        process(12, Some(11), "cycle", 12),
    ];
    let intent = snapshot_process_tree(&processes, 10).expect("complete frozen cycle");
    let pids = intent.execution_pids();
    assert_eq!(pids, vec![12, 11, 10]);
    let unique: HashSet<u32> = pids.iter().copied().collect();
    assert_eq!(unique.len(), pids.len());
}

#[test]
fn tree_snapshot_of_an_unknown_root_fails_closed_like_the_core_freeze() {
    // A dead root has no honest targets: parity with the shell track's
    // `freeze_tree` (empty closure) instead of sweeping up orphans whose
    // parent chain merely points at the missing pid.
    let processes = vec![
        process(1, None, "root", 100),
        process(2, Some(99), "orphan", 200),
    ];
    assert!(snapshot_process_tree(&processes, 99).is_none());
}

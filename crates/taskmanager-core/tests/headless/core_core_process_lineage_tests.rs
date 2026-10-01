use super::*;

fn test_proc(pid: u32, parent_pid: Option<u32>, name: &str) -> ProcessItem {
    let mut item = ProcessItem::new(pid, name);
    item.parent_pid = parent_pid;
    item
}

#[test]
fn lineage_walks_up_to_root_and_orders_top_down() {
    let procs = vec![
        test_proc(1, None, "systemd"),
        test_proc(800, Some(1), "sway"),
        test_proc(1000, Some(800), "alacritty"),
        test_proc(1234, Some(1000), "bash"),
    ];

    let lineage = process_ancestor_lineage(&procs, 1234);
    assert_eq!(
        lineage,
        vec![
            ProcessAncestorNode {
                pid: 1,
                name: "systemd".to_string(),
            },
            ProcessAncestorNode {
                pid: 800,
                name: "sway".to_string(),
            },
            ProcessAncestorNode {
                pid: 1000,
                name: "alacritty".to_string(),
            },
        ]
    );
    assert_eq!(
        format_ancestor_lineage(&lineage),
        "systemd (1) > sway (800) > alacritty (1000)"
    );
}

#[test]
fn root_and_missing_processes_yield_empty_lineage() {
    let procs = vec![
        test_proc(1, None, "systemd"),
        test_proc(2, Some(0), "kthreadd"),
    ];

    let root_lineage = process_ancestor_lineage(&procs, 1);
    assert!(root_lineage.is_empty());
    assert_eq!(format_ancestor_lineage(&root_lineage), "—");

    let kthread_lineage = process_ancestor_lineage(&procs, 2);
    assert!(kthread_lineage.is_empty());
    assert_eq!(format_ancestor_lineage(&kthread_lineage), "—");

    let missing = process_ancestor_lineage(&procs, 999);
    assert!(missing.is_empty());
    assert_eq!(format_ancestor_lineage(&missing), "—");
}

#[test]
fn disconnected_parent_is_preserved_as_pid_placeholder() {
    let procs = vec![test_proc(500, Some(400), "child")];
    let lineage = process_ancestor_lineage(&procs, 500);
    assert_eq!(
        lineage,
        vec![ProcessAncestorNode {
            pid: 400,
            name: String::new(),
        }]
    );
    assert_eq!(format_ancestor_lineage(&lineage), "PID 400");
}

#[test]
fn cyclic_parent_chain_terminates_safely() {
    let procs = vec![
        test_proc(10, Some(20), "p10"),
        test_proc(20, Some(10), "p20"),
    ];
    let lineage = process_ancestor_lineage(&procs, 10);
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].pid, 20);
}

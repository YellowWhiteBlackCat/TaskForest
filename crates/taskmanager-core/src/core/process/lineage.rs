//! Process ancestor-lineage resolution and presentation.
//!
//! Provides the typed parent-chain traversal from a target process up to the root
//! (e.g. `systemd (1)` / init), formatting chains as `systemd (1) > sway (800) > alacritty (1000)`.
//! Safe against cycles, self-references, and missing parent entries.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::ProcessItem;

/// An ancestor node in a process's parent chain up to root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessAncestorNode {
    pub pid: u32,
    pub name: String,
}

/// Trace the parent chain of a process up to the root (e.g. PID 1 / init).
///
/// Returns an ordered list of ancestor nodes starting from the top-most root
/// (e.g., `systemd (1)`) down to the target's immediate parent.
///
/// If `target_pid` has no known parent, or is the root itself, or is not found
/// in `processes`, an empty vector is returned.
/// The traversal tracks visited PIDs to terminate safely on cyclic or broken chains.
#[must_use]
pub fn process_ancestor_lineage(
    processes: &[ProcessItem],
    target_pid: u32,
) -> Vec<ProcessAncestorNode> {
    if target_pid == 0 {
        return Vec::new();
    }
    let by_pid: HashMap<u32, &ProcessItem> = processes.iter().map(|p| (p.pid, p)).collect();
    let mut current = match by_pid.get(&target_pid) {
        Some(item) => *item,
        None => return Vec::new(),
    };

    let mut ancestors = Vec::new();
    let mut visited = HashSet::new();
    visited.insert(target_pid);

    while let Some(parent_pid) = current.parent_pid {
        if parent_pid == 0 || parent_pid == current.pid || !visited.insert(parent_pid) {
            break;
        }
        if let Some(parent_item) = by_pid.get(&parent_pid) {
            ancestors.push(ProcessAncestorNode {
                pid: parent_item.pid,
                name: parent_item.name.clone(),
            });
            current = parent_item;
        } else {
            ancestors.push(ProcessAncestorNode {
                pid: parent_pid,
                name: String::new(),
            });
            break;
        }
    }

    ancestors.reverse();
    ancestors
}

/// Format the ancestor lineage for display: `systemd (1) > sway (800) > alacritty (1000)`.
/// Returns `"—"` if the ancestor lineage is empty.
#[must_use]
pub fn format_ancestor_lineage(ancestors: &[ProcessAncestorNode]) -> String {
    if ancestors.is_empty() {
        "—".to_string()
    } else {
        ancestors
            .iter()
            .map(|node| {
                if node.name.is_empty() {
                    format!("PID {}", node.pid)
                } else {
                    format!("{} ({})", node.name, node.pid)
                }
            })
            .collect::<Vec<_>>()
            .join(" > ")
    }
}

#[cfg(test)]
mod tests {
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
}

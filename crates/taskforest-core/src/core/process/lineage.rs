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
#[path = "../../../tests/headless/core_core_process_lineage_tests.rs"]
mod tests;

//! The frozen collateral set of a process batch: the tree root and every
//! affected descendant, each with its depth below the root.
//!
//! Ending a process tree affects the root and every process below it. The
//! execution set of a batch is flat, so a confirmation surface cannot draw
//! that closure as an indented list without re-reading the live rows — which
//! the frozen-gate contract forbids: an armed confirmation must render the
//! set that was frozen at arming time, never a live re-read.
//!
//! This module owns that typed fact. The closure is captured from the SAME
//! expansion the execution set uses, so the drawn hierarchy can never
//! disagree with what will be killed, and it is captured once, so a later
//! refresh of the live rows cannot change it.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{FrozenProcessIdentity, ProcessItem, ProcessLiveKey};

/// One member of a frozen collateral set: the exact frozen identity of an
/// affected process and its depth below the frozen root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenProcessCollateralMember {
    /// The frozen identity of the affected process.
    pub identity: FrozenProcessIdentity,
    /// The nesting depth below the frozen root; `0` is the root itself.
    pub depth: u32,
}

impl FrozenProcessCollateralMember {
    /// The frozen identity of the affected process.
    #[must_use]
    pub const fn identity(&self) -> &FrozenProcessIdentity {
        &self.identity
    }

    /// The nesting depth below the frozen root (`0` is the root itself).
    #[must_use]
    pub const fn depth(&self) -> u32 {
        self.depth
    }

    /// The process id of the affected process.
    #[must_use]
    pub const fn pid(&self) -> u32 {
        self.identity.pid
    }

    /// The process name frozen at capture time.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.identity.name
    }
}

/// The frozen collateral set of one process batch.
///
/// The set is a VIEW of the frozen execution targets: it never names a
/// process the execution set does not contain, and it never drops one. It
/// adds the hierarchy the flat target list cannot carry — the tree root and
/// each affected descendant's depth below that root.
///
/// A batch that was not frozen from a tree root (a plain selection batch)
/// has no hierarchy: the set is empty, which is a valid state and not an
/// error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenProcessCollateral {
    /// The frozen root identity; `None` when the batch was not frozen from a
    /// tree root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root: Option<FrozenProcessIdentity>,
    /// The ordered members: the root first, then every affected descendant
    /// in depth-first order (siblings by ascending process id).
    #[serde(default)]
    members: Vec<FrozenProcessCollateralMember>,
}

impl FrozenProcessCollateral {
    /// The frozen root identity, when the batch was frozen from a tree root.
    #[must_use]
    pub const fn root(&self) -> Option<&FrozenProcessIdentity> {
        self.root.as_ref()
    }

    /// The ordered members a confirmation surface renders: the root first,
    /// then every affected descendant in depth-first order.
    #[must_use]
    pub fn members(&self) -> &[FrozenProcessCollateralMember] {
        &self.members
    }

    /// Whether the frozen collateral set holds no member at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// The number of frozen collateral members.
    #[must_use]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Freeze the collateral view of one expansion.
    ///
    /// The members are the frozen identities of the SAME closure the
    /// execution set uses, so the view can never introduce a target the
    /// batch will not touch, nor lose one. Members whose live row carries no
    /// exact identity are absent from both views — an unavailable identity
    /// never becomes an actionable row.
    pub(crate) fn from_closure(processes: &[ProcessItem], closure: &ProcessTreeClosure) -> Self {
        let members: Vec<FrozenProcessCollateralMember> = closure
            .ordered()
            .iter()
            .filter_map(|(identity, depth)| {
                let process = processes
                    .iter()
                    .find(|process| ProcessLiveKey::from_process(process) == Some(*identity))?;
                Some(FrozenProcessCollateralMember {
                    identity: FrozenProcessIdentity::from_process(process)?,
                    depth: *depth,
                })
            })
            .collect();
        let root = members
            .iter()
            .find(|member| member.depth == 0)
            .map(|member| member.identity.clone());
        Self { root, members }
    }
}

/// The ONE `parent_pid` expansion of one tree root (同一律).
///
/// The closure carries the same members in the two orders the product needs:
/// the root-first depth-first order a confirmation surface draws, and the
/// leaf-first order the execution set kills in. Both come from one walk, so
/// the drawn hierarchy can never disagree with what is executed.
pub(crate) struct ProcessTreeClosure {
    /// Every member of the closure in root-first depth-first order, with its
    /// depth below the root.
    ordered: Vec<(ProcessLiveKey, u32)>,
    /// The same members in leaf-first order — the execution order.
    execution: Vec<ProcessLiveKey>,
}

impl ProcessTreeClosure {
    /// Expand the `parent_pid` closure of one root.
    ///
    /// The walk is total on cyclic `parent_pid` chains (the visited set makes
    /// it terminate) and fails closed on an ambiguous snapshot: a provider
    /// snapshot that repeats one PID, or a root that is not present, yields
    /// an EMPTY closure — a control intent over a dead or ambiguous row has
    /// no honest targets.
    pub(crate) fn freeze(processes: &[ProcessItem], root: ProcessLiveKey) -> Self {
        let Some(root_process) = processes
            .iter()
            .find(|process| ProcessLiveKey::from_process(process) == Some(root))
        else {
            return Self::empty();
        };
        let mut by_pid = HashMap::with_capacity(processes.len());
        for process in processes {
            // Parent links are PID-based, so a provider snapshot containing two
            // rows for one PID is ambiguous even when their start tokens differ.
            // Refuse the whole tree rather than allowing the lookup map to pick a
            // different incarnation than the root identity that was selected.
            if by_pid.insert(process.pid, process).is_some() {
                return Self::empty();
            }
        }
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for process in processes {
            if let Some(parent) = process.parent_pid
                && process.pid != root.pid()
            {
                children.entry(parent).or_default().push(process.pid);
            }
        }
        for child_pids in children.values_mut() {
            child_pids.sort_unstable();
        }

        fn visit(
            pid: u32,
            depth: u32,
            children: &HashMap<u32, Vec<u32>>,
            visited: &mut HashSet<u32>,
            by_pid: &HashMap<u32, &ProcessItem>,
            ordered: &mut Vec<(ProcessLiveKey, u32)>,
            execution: &mut Vec<ProcessLiveKey>,
        ) {
            if !visited.insert(pid) {
                return;
            }
            let identity = by_pid
                .get(&pid)
                .and_then(|process| ProcessLiveKey::from_process(process));
            if let Some(identity) = identity {
                // Root-first order: the drawn hierarchy starts at the root.
                ordered.push((identity, depth));
            }
            if let Some(child_pids) = children.get(&pid) {
                for child_pid in child_pids {
                    visit(
                        *child_pid,
                        depth.saturating_add(1),
                        children,
                        visited,
                        by_pid,
                        ordered,
                        execution,
                    );
                }
            }
            if let Some(identity) = identity {
                // Leaf-first order: the execution set ends the deepest
                // process of a branch before its parent.
                execution.push(identity);
            }
        }

        let mut ordered = Vec::new();
        let mut execution = Vec::new();
        visit(
            root_process.pid,
            0,
            &children,
            &mut HashSet::new(),
            &by_pid,
            &mut ordered,
            &mut execution,
        );
        Self { ordered, execution }
    }

    /// The closure members in root-first depth-first order, with their depth
    /// below the root.
    pub(crate) fn ordered(&self) -> &[(ProcessLiveKey, u32)] {
        &self.ordered
    }

    /// The closure members in leaf-first order — the execution order.
    pub(crate) fn execution(&self) -> &[ProcessLiveKey] {
        &self.execution
    }

    /// The fail-closed empty closure: no root, no members, no targets.
    fn empty() -> Self {
        Self {
            ordered: Vec::new(),
            execution: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_collateral_tests.rs"]
mod tests;

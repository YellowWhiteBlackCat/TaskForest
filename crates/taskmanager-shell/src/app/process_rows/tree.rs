//! Shared, toolkit-neutral category/tree row structure.
//!
//! This module owns the row shape that all frontends render. It deliberately
//! stores visible-list indexes rather than borrowed process values, so a
//! frontend may cache the structure without extending a borrow of the shell's
//! process snapshot. The index is valid only with the visible list used to
//! build the projection; the caller's process revision is the cache boundary.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use taskmanager_application::process_category_projection::{
    category_buckets, category_expansion_key, process_memory_observation_for_display,
};
use taskmanager_application::process_sort::compare_processes;
use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_core::core::process::aggregate::aggregate_u32_widened;
use taskmanager_core::core::process::{
    ProcessCategory, ProcessItem, ProcessLiveKey, ProcessNode, build_process_tree, process_category,
};

use super::ProcessRowId;
use crate::app::{SortCol, SortDir, sort_axis};

/// Expansion-key prefix for an application-root row.
pub const APP_TREE_EXPANSION_KEY_PREFIX: &str = "app-tree:";

/// The two aggregate facts shared by every group header.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRowAggregate {
    cpu: AggregateMetric<f32>,
    memory: AggregateMetric<u64>,
    memory_pss: AggregateMetric<u64>,
    swap: AggregateMetric<u64>,
    disk_read: AggregateMetric<u64>,
    disk_write: AggregateMetric<u64>,
    cpu_time: AggregateMetric<u64>,
    threads: AggregateMetric<u64>,
    fds: AggregateMetric<u64>,
}

impl ProcessRowAggregate {
    /// Availability-bearing CPU total.
    #[must_use]
    pub const fn cpu(&self) -> &AggregateMetric<f32> {
        &self.cpu
    }

    /// Availability-bearing memory total. The display policy is PSS when a
    /// current PSS observation exists, otherwise typed RSS.
    #[must_use]
    pub const fn memory(&self) -> &AggregateMetric<u64> {
        &self.memory
    }

    /// Availability-bearing PSS total without an RSS fallback.
    #[must_use]
    pub const fn memory_pss(&self) -> &AggregateMetric<u64> {
        &self.memory_pss
    }

    /// Availability-bearing swap total.
    #[must_use]
    pub const fn swap(&self) -> &AggregateMetric<u64> {
        &self.swap
    }

    /// Availability-bearing disk-read rate total.
    #[must_use]
    pub const fn disk_read(&self) -> &AggregateMetric<u64> {
        &self.disk_read
    }

    /// Availability-bearing disk-write rate total.
    #[must_use]
    pub const fn disk_write(&self) -> &AggregateMetric<u64> {
        &self.disk_write
    }

    /// Availability-bearing cumulative CPU-time total.
    #[must_use]
    pub const fn cpu_time(&self) -> &AggregateMetric<u64> {
        &self.cpu_time
    }

    /// Availability-bearing thread-count total.
    #[must_use]
    pub const fn threads(&self) -> &AggregateMetric<u64> {
        &self.threads
    }

    /// Availability-bearing file-descriptor total.
    #[must_use]
    pub const fn fds(&self) -> &AggregateMetric<u64> {
        &self.fds
    }
}

/// Owned structure of one visible Applications row.
///
/// The row kind, hierarchy, expansion state, aggregate facts, and target
/// identity are shared. Text formatting, widget handles, focus state, and
/// toolkit-specific cells remain frontend concerns. A process without a
/// current start token is retained with `row_key == None`; it is displayable
/// but cannot become an exact control target.
#[derive(Clone, Debug, PartialEq)]
pub enum ProcessTreeRow {
    /// One non-empty process category. It is structural and never actionable.
    Category {
        category: ProcessCategory,
        expansion_key: String,
        representative_index: usize,
        expanded: bool,
        member_count: usize,
        aggregate: ProcessRowAggregate,
    },
    /// One application tree-root aggregate. The visible index resolves its
    /// display label and root facts from the same visible process list.
    Application {
        visible_index: usize,
        row_key: Option<ProcessRowId>,
        expansion_key: String,
        parent_key: ProcessRowId,
        expanded: bool,
        member_count: usize,
        aggregate: ProcessRowAggregate,
        has_children: bool,
    },
    /// One real process node in the recursive hierarchy.
    Process {
        visible_index: usize,
        row_key: Option<ProcessRowId>,
        parent_key: Option<ProcessRowId>,
        depth: usize,
        has_children: bool,
        collapsed: bool,
    },
}

impl ProcessTreeRow {
    /// The position in the visible process list for rows backed by a process.
    #[must_use]
    pub const fn visible_index(&self) -> Option<usize> {
        match self {
            Self::Category { .. } => None,
            Self::Application { visible_index, .. } | Self::Process { visible_index, .. } => {
                Some(*visible_index)
            }
        }
    }

    /// The typed actionable identity, if the row has one.
    #[must_use]
    pub const fn row_key(&self) -> Option<ProcessRowId> {
        match self {
            Self::Category { .. } => None,
            Self::Application { row_key, .. } | Self::Process { row_key, .. } => *row_key,
        }
    }

    /// The structural parent. Category parents are retained even though they
    /// are not actionable, so adapters can derive nearest selectable parents
    /// without rebuilding the tree.
    #[must_use]
    pub const fn parent_key(&self) -> Option<ProcessRowId> {
        match self {
            Self::Category { .. } => None,
            Self::Application { parent_key, .. } => Some(*parent_key),
            Self::Process { parent_key, .. } => *parent_key,
        }
    }

    /// The row depth in the shared category-first hierarchy.
    #[must_use]
    pub const fn depth(&self) -> usize {
        match self {
            Self::Category { .. } => 0,
            Self::Application { .. } => 1,
            Self::Process { depth, .. } => *depth,
        }
    }

    /// Whether this row is a group header rather than an individual process.
    #[must_use]
    pub const fn is_group(&self) -> bool {
        matches!(self, Self::Category { .. } | Self::Application { .. })
    }

    /// Expansion token for a category or application header.
    #[must_use]
    pub fn expansion_key(&self) -> Option<&str> {
        match self {
            Self::Category { expansion_key, .. } | Self::Application { expansion_key, .. } => {
                Some(expansion_key)
            }
            Self::Process { .. } => None,
        }
    }

    /// Aggregate facts for a group header; process rows have no aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> Option<&ProcessRowAggregate> {
        match self {
            Self::Category { aggregate, .. } | Self::Application { aggregate, .. } => {
                Some(aggregate)
            }
            Self::Process { .. } => None,
        }
    }
}

/// Build the one shared category-first process hierarchy.
///
/// `processes` must already be the shell's filtered and sorted visible list.
/// The function never fabricates a category for an empty bucket, never drops
/// a process solely because its identity/metric is unavailable, and performs
/// the application-root and recursive ordering through the shared comparator.
#[must_use]
pub fn project_process_tree_rows(
    processes: &[&ProcessItem],
    expanded_groups: &HashSet<String>,
    collapsed_processes: &HashSet<ProcessLiveKey>,
    sort: (SortCol, SortDir),
    observed_at_ms: u64,
) -> Vec<ProcessTreeRow> {
    let visible_index_by_pid: HashMap<u32, usize> = processes
        .iter()
        .enumerate()
        .map(|(index, process)| (process.pid, index))
        .collect();
    let mut rows = Vec::new();

    for bucket in category_buckets(processes, |process| process_category(process)) {
        let category = bucket.category();
        let members: Vec<&ProcessItem> = bucket.members().iter().map(|member| **member).collect();
        let expansion_key = category_expansion_key(category);
        let expanded = expanded_groups.contains(&expansion_key);
        let Some(aggregate) = aggregate_members(&members, observed_at_ms) else {
            // `members` is non-empty by category_buckets' contract. Keeping
            // this guard makes the projection fail closed if that contract
            // changes, without inventing a row or a numeric value.
            continue;
        };
        let Some(representative_index) = representative_index(&members, &visible_index_by_pid)
        else {
            continue;
        };
        rows.push(ProcessTreeRow::Category {
            category,
            expansion_key,
            representative_index,
            expanded,
            member_count: members.len(),
            aggregate,
        });
        if !expanded {
            continue;
        }

        let mut tree = build_process_tree(&members);
        sort_tree_nodes(&mut tree, sort);
        if category == ProcessCategory::Application {
            let roots = sort_application_roots(tree, sort, observed_at_ms);
            for (root, aggregate) in roots {
                let root_identity = ProcessLiveKey::from_process(root.item);
                let row_key = root_identity.map(ProcessRowId::Application);
                let process_index = visible_index_by_pid.get(&root.item.pid).copied();
                let Some(visible_index) = process_index else {
                    continue;
                };
                let app_expansion_key = app_tree_expansion_key(root.item);
                let app_expanded = expanded_groups.contains(&app_expansion_key);
                rows.push(ProcessTreeRow::Application {
                    visible_index,
                    row_key,
                    expansion_key: app_expansion_key,
                    parent_key: ProcessRowId::Category(category),
                    expanded: app_expanded,
                    member_count: tree_size(&root),
                    aggregate,
                    // Expanding an application aggregate always reveals its
                    // root process, even when that root has no descendants.
                    has_children: true,
                });
                if app_expanded {
                    push_process_rows(
                        &root,
                        2,
                        collapsed_processes,
                        &visible_index_by_pid,
                        row_key,
                        &mut rows,
                    );
                }
            }
        } else {
            for root in &tree {
                push_process_rows(
                    root,
                    1,
                    collapsed_processes,
                    &visible_index_by_pid,
                    Some(ProcessRowId::Category(category)),
                    &mut rows,
                );
            }
        }
    }

    rows
}

/// Build the locale-neutral expansion token for one application aggregate.
/// Unknown identity is explicitly snapshot-local and cannot be mistaken for a
/// reusable live key.
#[must_use]
pub fn app_tree_expansion_key(process: &ProcessItem) -> String {
    ProcessLiveKey::from_process(process).map_or_else(
        || format!("{APP_TREE_EXPANSION_KEY_PREFIX}pid:{}:unknown", process.pid),
        app_tree_expansion_key_for_identity,
    )
}

/// Build an application expansion token from an already validated live key.
/// This keeps frontends from re-spelling the identity-bearing key format.
#[must_use]
pub fn app_tree_expansion_key_for_identity(identity: ProcessLiveKey) -> String {
    format!("{APP_TREE_EXPANSION_KEY_PREFIX}{}", identity.stable_key())
}

fn aggregate_members(members: &[&ProcessItem], observed_at_ms: u64) -> Option<ProcessRowAggregate> {
    let bucket = category_buckets(members, |_| ProcessCategory::Application)
        .into_iter()
        .next()?;
    Some(ProcessRowAggregate {
        cpu: bucket.aggregate_f32(observed_at_ms, |process| {
            &process.scalar_observations().cpu_percentage
        })?,
        memory: bucket.aggregate_u64(observed_at_ms, |process| {
            process_memory_observation_for_display(process)
        })?,
        memory_pss: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().memory_pss_bytes
        })?,
        swap: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().swap_bytes
        })?,
        disk_read: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().disk_read_bytes_per_sec
        })?,
        disk_write: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().disk_write_bytes_per_sec
        })?,
        cpu_time: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().cpu_time_secs
        })?,
        threads: aggregate_u32_widened(
            members
                .iter()
                .map(|process| &process.scalar_observations().threads),
            observed_at_ms,
        )?,
        fds: aggregate_u32_widened(
            members
                .iter()
                .map(|process| &process.scalar_observations().fds),
            observed_at_ms,
        )?,
    })
}

fn representative_index(
    members: &[&ProcessItem],
    visible_index_by_pid: &HashMap<u32, usize>,
) -> Option<usize> {
    members
        .iter()
        .min_by_key(|process| process.pid)
        .and_then(|process| visible_index_by_pid.get(&process.pid).copied())
}

fn sort_tree_nodes(nodes: &mut [ProcessNode<'_>], sort: (SortCol, SortDir)) {
    let (column, direction) = sort;
    let ascending = direction == SortDir::Asc;
    let axis = sort_axis(column);
    nodes.sort_by(|left, right| compare_processes(left.item, right.item, axis, ascending));
    for node in nodes {
        sort_tree_nodes(&mut node.children, sort);
    }
}

fn sort_application_roots<'a>(
    tree: Vec<ProcessNode<'a>>,
    sort: (SortCol, SortDir),
    observed_at_ms: u64,
) -> Vec<(ProcessNode<'a>, ProcessRowAggregate)> {
    let (column, direction) = sort;
    let ascending = direction == SortDir::Asc;
    let mut roots: Vec<_> = tree
        .into_iter()
        .filter_map(|root| {
            aggregate_for_tree(&root, observed_at_ms).map(|aggregate| (root, aggregate))
        })
        .collect();
    roots.sort_by(
        |(left_root, left_aggregate), (right_root, right_aggregate)| {
            let ordering = match column {
                SortCol::Cpu => compare_optional_f32(
                    left_aggregate.cpu().current_value(),
                    right_aggregate.cpu().current_value(),
                ),
                SortCol::Memory => compare_optional_u64(
                    left_aggregate.memory().current_value(),
                    right_aggregate.memory().current_value(),
                ),
                SortCol::Pss => compare_optional_u64(
                    left_aggregate.memory_pss().current_value(),
                    right_aggregate.memory_pss().current_value(),
                ),
                SortCol::Swap => compare_optional_u64(
                    left_aggregate.swap().current_value(),
                    right_aggregate.swap().current_value(),
                ),
                SortCol::DiskRead => compare_optional_u64(
                    left_aggregate.disk_read().current_value(),
                    right_aggregate.disk_read().current_value(),
                ),
                SortCol::DiskWrite => compare_optional_u64(
                    left_aggregate.disk_write().current_value(),
                    right_aggregate.disk_write().current_value(),
                ),
                SortCol::CpuTime => compare_optional_u64(
                    left_aggregate.cpu_time().current_value(),
                    right_aggregate.cpu_time().current_value(),
                ),
                SortCol::Threads => compare_optional_u64(
                    left_aggregate.threads().current_value(),
                    right_aggregate.threads().current_value(),
                ),
                SortCol::Fds => compare_optional_u64(
                    left_aggregate.fds().current_value(),
                    right_aggregate.fds().current_value(),
                ),
                _ => compare_processes(
                    left_root.item,
                    right_root.item,
                    sort_axis(column),
                    ascending,
                ),
            };
            if matches!(
                column,
                SortCol::Cpu
                    | SortCol::Memory
                    | SortCol::Pss
                    | SortCol::Swap
                    | SortCol::DiskRead
                    | SortCol::DiskWrite
                    | SortCol::CpuTime
                    | SortCol::Threads
                    | SortCol::Fds
            ) {
                directed(ordering, ascending)
                    .then_with(|| left_root.item.pid.cmp(&right_root.item.pid))
            } else {
                ordering
            }
        },
    );
    roots
}

fn aggregate_for_tree(root: &ProcessNode<'_>, observed_at_ms: u64) -> Option<ProcessRowAggregate> {
    let mut members = Vec::new();
    collect_tree_members(root, &mut members);
    aggregate_members(&members, observed_at_ms)
}

fn collect_tree_members<'a>(root: &ProcessNode<'a>, members: &mut Vec<&'a ProcessItem>) {
    members.push(root.item);
    for child in &root.children {
        collect_tree_members(child, members);
    }
}

fn tree_size(root: &ProcessNode<'_>) -> usize {
    1 + root.children.iter().map(tree_size).sum::<usize>()
}

fn push_process_rows(
    root: &ProcessNode<'_>,
    depth_offset: usize,
    collapsed_processes: &HashSet<ProcessLiveKey>,
    visible_index_by_pid: &HashMap<u32, usize>,
    parent_key: Option<ProcessRowId>,
    rows: &mut Vec<ProcessTreeRow>,
) {
    let identity = ProcessLiveKey::from_process(root.item);
    let row_key = identity.map(ProcessRowId::Process);
    let Some(visible_index) = visible_index_by_pid.get(&root.item.pid).copied() else {
        return;
    };
    let has_children = !root.children.is_empty();
    let collapsed = has_children && identity.is_some_and(|key| collapsed_processes.contains(&key));
    rows.push(ProcessTreeRow::Process {
        visible_index,
        row_key,
        parent_key,
        depth: depth_offset.saturating_add(root.depth),
        has_children,
        collapsed,
    });
    if collapsed {
        return;
    }
    for child in &root.children {
        push_process_rows(
            child,
            depth_offset,
            collapsed_processes,
            visible_index_by_pid,
            row_key,
            rows,
        );
    }
}

fn directed(ordering: Ordering, ascending: bool) -> Ordering {
    if ascending {
        ordering
    } else {
        ordering.reverse()
    }
}

fn compare_optional_f32(left: Option<&f32>, right: Option<&f32>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.total_cmp(right),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_optional_u64(left: Option<&u64>, right: Option<&u64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(right),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/shell_process_tree_projection.rs"]
mod tests;

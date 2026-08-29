//! Platform-neutral process snapshots, identity contracts, and list algorithms.

pub use super::process_batch_history::{
    DEFAULT_PROCESS_BATCH_HISTORY_CAPACITY, ProcessBatchHistory, ProcessBatchHistoryEntry,
    ProcessBatchHistoryExportError, ProcessBatchHistoryFormat, ProcessBatchHistoryTarget,
    execute_process_batch_recording_with, export_process_batch_history,
};

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod aggregate;
mod application;
mod control;
mod history;
pub mod identity;
mod metadata;
mod scalars;
mod wire;
pub use application::{
    ApplicationIconAsset, ApplicationIconFormat, MAX_APPLICATION_ICON_BYTES,
    ProcessApplicationIdentity, ProcessCategory, process_category,
};
pub(crate) use control::process_batch_failure_wire_code;
pub use control::{
    FrozenProcessIdentity, PriorityTier, ProcessBatchAction, ProcessBatchIntent,
    ProcessBatchResult, ProcessBatchTargetResult, ProcessGroupScope, ProcessSignal,
    descendant_pids, execute_process_batch_with,
};
pub use history::{ProcessHistorySample, ProcessHistorySnapshot, ProcessHistoryStore};
pub use metadata::{
    ProcessMetadataAvailability, ProcessMetadataFailure, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessOwner, ProcessOwnerIdentity,
};
pub use scalars::ProcessScalarObservations;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessItem {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub cmdline: String,
    pub status: String,
    metadata_observations: ProcessMetadataObservations,
    /// Typed desktop-entry identity for a process that is known to belong to
    /// an application. Older payloads remain `Unknown`; confirmed absence is
    /// distinct from a provider failure.
    application_identity: ProcessMetadataObservation<ProcessApplicationIdentity>,
    /// The sole process-row scalar authority. Schema-v1 numeric fields exist
    /// only in the private wire DTO.
    scalar_observations: ProcessScalarObservations,
    pub cpu_history: Vec<f32>,
    pub mem_history: Vec<f32>,
    pub disk_history: Vec<f32>,
    pub disk_read_history: Vec<f32>,
    pub disk_write_history: Vec<f32>,
}

impl ProcessItem {
    #[must_use]
    pub fn new(pid: u32, name: impl Into<String>) -> Self {
        Self {
            pid,
            name: name.into(),
            ..Self::default()
        }
    }

    /// Replace the typed metadata group. Legacy fields are projected only by
    /// the serializer and never stored in the domain model.
    pub fn apply_metadata_observations(&mut self, observations: ProcessMetadataObservations) {
        self.metadata_observations = observations;
    }

    #[must_use]
    pub fn with_metadata_observations(mut self, observations: ProcessMetadataObservations) -> Self {
        self.apply_metadata_observations(observations);
        self
    }

    #[must_use]
    pub const fn metadata_observations(&self) -> &ProcessMetadataObservations {
        &self.metadata_observations
    }

    pub fn apply_application_identity(
        &mut self,
        observation: ProcessMetadataObservation<ProcessApplicationIdentity>,
    ) {
        self.application_identity = observation;
    }

    #[must_use]
    pub fn with_application_identity_observation(
        mut self,
        observation: ProcessMetadataObservation<ProcessApplicationIdentity>,
    ) -> Self {
        self.apply_application_identity(observation);
        self
    }

    #[must_use]
    pub const fn application_identity_observation(
        &self,
    ) -> &ProcessMetadataObservation<ProcessApplicationIdentity> {
        &self.application_identity
    }

    /// Read the current owner label from canonical typed metadata.
    #[must_use]
    pub fn current_user(&self) -> Option<String> {
        self.metadata_observations
            .owner
            .current_value()
            .map(ProcessOwner::display_value)
    }

    /// Read the current executable path from canonical typed metadata.
    #[must_use]
    pub fn current_exe_path(&self) -> Option<&std::path::Path> {
        self.metadata_observations
            .executable_path
            .current_value()
            .map(PathBuf::as_path)
    }

    /// Return the current verified desktop-entry identity, never a stale or
    /// unavailable association.
    #[must_use]
    pub const fn current_application_identity(&self) -> Option<&ProcessApplicationIdentity> {
        self.application_identity.current_value()
    }

    /// Return the current verified application display name, if one exists.
    #[must_use]
    pub fn current_application_name(&self) -> Option<&str> {
        self.current_application_identity()
            .map(|identity| identity.display_name.as_str())
    }
}

/// A process-tree view node. Holds a *reference* into the caller's process
/// slice: the tree is a pure view structure, so building it never clones
/// items (the previous owned `ProcessItem` copied every process per frame in
/// Tree mode).
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessNode<'a> {
    pub item: &'a ProcessItem,
    pub depth: usize,
    pub children_pids: Vec<u32>,
    pub children: Vec<ProcessNode<'a>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppGroup {
    pub name: String,
    pub main_pid: u32,
    #[serde(default)]
    pub application_identity: Option<ProcessApplicationIdentity>,
    pub pids: Vec<u32>,
    pub total_cpu_usage: f32,
    pub total_memory_bytes: u64,
    pub process_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSortKey {
    Pid,
    Name,
    CpuUsage,
    Memory,
    DiskRead,
    DiskWrite,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserAggregate {
    pub user: String,
    pub total_cpu_usage: f32,
    pub total_memory_bytes: u64,
    pub process_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessType {
    Userspace,
    Kernel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlatTreeNode<'a> {
    pub item: &'a ProcessItem,
    pub depth: usize,
    pub has_children: bool,
}

#[must_use]
pub fn normalize_app_name(name: &str, cmdline: &str) -> String {
    // All needles are ASCII, so a byte-wise ASCII case-fold is behaviorally
    // identical to `to_lowercase().contains(...)` (Unicode lowercasing never
    // changes ASCII bytes except for exotic special cases like U+FB00 ligature
    // folds). This runs once per process while building application aggregates — the
    // two `to_lowercase` allocations would be 20k heap churn on a 10k list.
    for (needle, label) in [
        ("chrome", "Google Chrome"),
        ("code", "VS Code"),
        ("vscode", "VS Code"),
        ("zed", "Zed"),
        ("firefox", "Firefox"),
        ("discord", "Discord"),
        ("slack", "Slack"),
        ("spotify", "Spotify"),
        ("steam", "Steam"),
        ("thunderbird", "Thunderbird"),
    ] {
        if crate::core::text::contains_ascii_ci(name, needle)
            || crate::core::text::contains_ascii_ci(cmdline, needle)
        {
            return label.to_owned();
        }
    }
    if name.trim().is_empty() {
        "Unknown".to_owned()
    } else {
        name.to_owned()
    }
}

#[must_use]
pub fn build_process_tree<'a>(items: &[&'a ProcessItem]) -> Vec<ProcessNode<'a>> {
    let by_pid: HashMap<u32, &ProcessItem> = items.iter().map(|item| (item.pid, *item)).collect();
    let mut children: HashMap<u32, Vec<&ProcessItem>> = HashMap::new();
    for item in items {
        if let Some(parent) = item.parent_pid {
            children.entry(parent).or_default().push(*item);
        }
    }
    let roots: Vec<&ProcessItem> = items
        .iter()
        .copied()
        .filter(|item| {
            item.parent_pid
                .is_none_or(|parent| parent == item.pid || !by_pid.contains_key(&parent))
        })
        .collect();
    let mut visited = HashSet::new();
    fn node<'a>(
        item: &'a ProcessItem,
        depth: usize,
        children: &HashMap<u32, Vec<&'a ProcessItem>>,
        visited: &mut HashSet<u32>,
    ) -> ProcessNode<'a> {
        visited.insert(item.pid);
        let mut child_nodes = Vec::new();
        if let Some(candidates) = children.get(&item.pid) {
            for child in candidates {
                if !visited.contains(&child.pid) {
                    child_nodes.push(node(child, depth + 1, children, visited));
                }
            }
        }
        ProcessNode {
            item,
            depth,
            children_pids: child_nodes.iter().map(|child| child.item.pid).collect(),
            children: child_nodes,
        }
    }
    roots
        .into_iter()
        .map(|item| node(item, 0, &children, &mut visited))
        .collect()
}

#[must_use]
pub fn aggregate_apps(items: &[&ProcessItem]) -> Vec<AppGroup> {
    let mut grouped: HashMap<String, Vec<&ProcessItem>> = HashMap::new();
    for item in items {
        grouped
            .entry(application_group_name(item))
            .or_default()
            .push(*item);
    }
    let mut groups: Vec<_> = grouped
        .into_iter()
        .map(|(name, processes)| {
            let mut pids: Vec<_> = processes.iter().map(|process| process.pid).collect();
            pids.sort_unstable();
            let pid_set: HashSet<_> = pids.iter().copied().collect();
            let main_pid = processes
                .iter()
                .filter(|process| process.parent_pid.is_none_or(|pid| !pid_set.contains(&pid)))
                .map(|process| process.pid)
                .min()
                .unwrap_or_else(|| pids[0]);
            let application_identity = processes
                .iter()
                .find_map(|process| process.current_application_identity())
                .cloned();
            AppGroup {
                name,
                main_pid,
                application_identity,
                total_cpu_usage: processes
                    .iter()
                    .filter_map(|process| process.current_cpu_percentage())
                    .sum(),
                total_memory_bytes: processes
                    .iter()
                    .filter_map(|process| process.current_memory_bytes())
                    .sum(),
                process_count: processes.len(),
                pids,
            }
        })
        .collect();
    groups.sort_by(|left, right| {
        right
            .total_cpu_usage
            .partial_cmp(&left.total_cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.total_memory_bytes.cmp(&left.total_memory_bytes))
            .then_with(|| left.main_pid.cmp(&right.main_pid))
    });
    groups
}

/// Resolve the stable application label when the provider has proved one;
/// otherwise preserve the legacy process-name normalization fallback.
#[must_use]
pub fn application_group_name(item: &ProcessItem) -> String {
    item.current_application_name()
        .map(str::to_owned)
        .unwrap_or_else(|| normalize_app_name(&item.name, &item.cmdline))
}

#[must_use]
pub fn aggregate_by_user(processes: &[ProcessItem]) -> Vec<UserAggregate> {
    let mut values: HashMap<String, (f32, u64, usize)> = HashMap::new();
    for process in processes {
        let entry = values
            .entry(process.current_user().unwrap_or_default())
            .or_default();
        entry.0 += process.current_cpu_percentage().unwrap_or(0.0);
        entry.1 = entry
            .1
            .saturating_add(process.current_memory_bytes().unwrap_or(0));
        entry.2 += 1;
    }
    let mut rows: Vec<_> = values
        .into_iter()
        .map(
            |(user, (total_cpu_usage, total_memory_bytes, process_count))| UserAggregate {
                user,
                total_cpu_usage,
                total_memory_bytes,
                process_count,
            },
        )
        .collect();
    rows.sort_by(|left, right| {
        right
            .total_cpu_usage
            .partial_cmp(&left.total_cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.total_memory_bytes.cmp(&left.total_memory_bytes))
            .then_with(|| left.user.cmp(&right.user))
    });
    rows
}

#[must_use]
pub fn classify_process_type(item: &ProcessItem) -> ProcessType {
    if item.name.trim_start().starts_with('[') {
        ProcessType::Kernel
    } else {
        ProcessType::Userspace
    }
}

/// Stable English label for one process-class kind. This is the group `name`
/// used by [`aggregate_by_type`] so renderers treat type-groups and app-groups
/// uniformly (the label rides on [`AppGroup::name`]); callers do not need this
/// directly unless labeling a [`ProcessType`] without a group.
#[must_use]
pub const fn process_type_label(process_type: ProcessType) -> &'static str {
    match process_type {
        ProcessType::Userspace => "Userspace",
        ProcessType::Kernel => "Kernel",
    }
}

/// Group processes by kernel/userspace classification, mirroring [`aggregate_apps`]
/// in shape and sort (CPU% desc → memory desc → main_pid asc) so a renderer can
/// treat app-groups and type-groups with the same code path. The group `name` is
/// the stable type label from [`process_type_label`]; `application_identity` is
/// `None` because a type is not an application.
#[must_use]
pub fn aggregate_by_type(items: &[&ProcessItem]) -> Vec<AppGroup> {
    let mut grouped: HashMap<&'static str, Vec<&ProcessItem>> = HashMap::new();
    for item in items {
        grouped
            .entry(process_type_label(classify_process_type(item)))
            .or_default()
            .push(*item);
    }
    let mut groups: Vec<_> = grouped
        .into_iter()
        .map(|(label, members)| {
            let mut pids: Vec<_> = members.iter().map(|process| process.pid).collect();
            pids.sort_unstable();
            AppGroup {
                name: label.to_string(),
                main_pid: pids.first().copied().unwrap_or(0),
                application_identity: None,
                total_cpu_usage: members
                    .iter()
                    .filter_map(|process| process.current_cpu_percentage())
                    .sum(),
                total_memory_bytes: members
                    .iter()
                    .filter_map(|process| process.current_memory_bytes())
                    .sum(),
                process_count: members.len(),
                pids,
            }
        })
        .collect();
    groups.sort_by(|left, right| {
        right
            .total_cpu_usage
            .partial_cmp(&left.total_cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.total_memory_bytes.cmp(&left.total_memory_bytes))
            .then_with(|| left.main_pid.cmp(&right.main_pid))
    });
    groups
}

#[must_use]
pub fn flatten_tree_visible<'a>(
    nodes: &[ProcessNode<'a>],
    collapsed: &HashSet<u32>,
) -> Vec<FlatTreeNode<'a>> {
    fn visit<'a>(
        nodes: &[ProcessNode<'a>],
        collapsed: &HashSet<u32>,
        rows: &mut Vec<FlatTreeNode<'a>>,
    ) {
        for node in nodes {
            let has_children = !node.children.is_empty();
            rows.push(FlatTreeNode {
                item: node.item,
                depth: node.depth,
                has_children,
            });
            if has_children && !collapsed.contains(&node.item.pid) {
                visit(&node.children, collapsed, rows);
            }
        }
    }
    let mut rows = Vec::new();
    visit(nodes, collapsed, &mut rows);
    rows
}

pub fn compare_process_items(
    left: &ProcessItem,
    right: &ProcessItem,
    key: ProcessSortKey,
) -> std::cmp::Ordering {
    let ordering = match key {
        ProcessSortKey::Pid => left.pid.cmp(&right.pid),
        ProcessSortKey::Name => left
            .name
            .bytes()
            .map(|b| b.to_ascii_lowercase())
            .cmp(right.name.bytes().map(|b| b.to_ascii_lowercase())),
        ProcessSortKey::CpuUsage => left
            .current_cpu_percentage()
            .partial_cmp(&right.current_cpu_percentage())
            .unwrap_or(std::cmp::Ordering::Equal),
        ProcessSortKey::Memory => left
            .current_memory_bytes()
            .cmp(&right.current_memory_bytes()),
        ProcessSortKey::DiskRead => left
            .current_disk_read_bytes_per_sec()
            .cmp(&right.current_disk_read_bytes_per_sec()),
        ProcessSortKey::DiskWrite => left
            .current_disk_write_bytes_per_sec()
            .cmp(&right.current_disk_write_bytes_per_sec()),
    };
    ordering.then_with(|| left.pid.cmp(&right.pid))
}

fn directed(ordering: std::cmp::Ordering, ascending: bool) -> std::cmp::Ordering {
    if ascending {
        ordering
    } else {
        ordering.reverse()
    }
}

pub fn sort_processes(items: &mut [ProcessItem], key: ProcessSortKey, ascending: bool) {
    items.sort_by(|left, right| directed(compare_process_items(left, right, key), ascending));
}

pub fn sort_nodes<'a>(nodes: &mut [ProcessNode<'a>], key: ProcessSortKey, ascending: bool) {
    nodes.sort_by(|left, right| {
        directed(compare_process_items(left.item, right.item, key), ascending)
    });
    for node in nodes {
        sort_nodes(&mut node.children, key, ascending);
    }
}

pub fn sort_apps(groups: &mut [AppGroup], key: ProcessSortKey, ascending: bool) {
    groups.sort_by(|left, right| {
        let ordering = match key {
            ProcessSortKey::Pid => left.main_pid.cmp(&right.main_pid),
            ProcessSortKey::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
            ProcessSortKey::CpuUsage => left
                .total_cpu_usage
                .partial_cmp(&right.total_cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessSortKey::Memory | ProcessSortKey::DiskRead | ProcessSortKey::DiskWrite => {
                left.total_memory_bytes.cmp(&right.total_memory_bytes)
            }
        }
        .then_with(|| left.main_pid.cmp(&right.main_pid));
        directed(ordering, ascending)
    });
}

#[must_use]
pub fn fuzzy_match(target: &str, query: &str) -> bool {
    let target = target.to_lowercase();
    let query = query.to_lowercase();
    if query.is_empty() || target.contains(&query) {
        return true;
    }
    let mut target_chars = target.chars();
    query
        .chars()
        .all(|query_char| target_chars.any(|target_char| target_char == query_char))
}

#[must_use]
pub fn fuzzy_filter_processes(items: &[ProcessItem], query: &str) -> Vec<ProcessItem> {
    let query = query.trim();
    if query.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .filter(|process| {
            fuzzy_match(&process.name, query)
                || fuzzy_match(&process.cmdline, query)
                || process.pid.to_string().contains(query)
                || process
                    .current_user()
                    .is_some_and(|user| fuzzy_match(&user, query))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_process_aggregate_by_type_tests.rs"]
mod aggregate_by_type_tests;

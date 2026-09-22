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
mod anomaly;
mod application;
mod control;
pub mod group_aggregate;
mod history;
pub mod identity;
mod metadata;
mod scalars;
pub mod scheduling;
mod wire;
pub use anomaly::{
    FD_PRESSURE_THRESHOLD, MEMORY_GROWTH_MIN_SAMPLES, ProcessAnomaly, ProcessAnomalyKind,
    ZOMBIE_STORM_THRESHOLD, detect_process_anomalies,
};
pub use application::{
    ApplicationIconAsset, ApplicationIconFormat, MAX_APPLICATION_ICON_BYTES,
    ProcessApplicationIdentity, ProcessCategory, process_category,
};
pub(crate) use control::process_batch_failure_wire_code;
pub use control::{
    FrozenProcessIdentity, PriorityTier, ProcessBatchAction, ProcessBatchIntent,
    ProcessBatchResult, ProcessBatchTargetResult, ProcessGroupScope, ProcessSignal,
    descendant_live_keys, execute_process_batch_with,
};
pub use history::{ProcessHistorySample, ProcessHistorySnapshot, ProcessHistoryStore};
pub use identity::ProcessLiveKey;
pub use metadata::{
    ProcessMetadataAvailability, ProcessMetadataFailure, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessOwner, ProcessOwnerIdentity,
};
pub use scalars::ProcessScalarObservations;
pub use scheduling::ProcessSchedulingPolicy;

/// Provider-neutral process scheduler state used by aggregate diagnostics.
/// Linux `D` is kept distinct from ordinary sleeping: it denotes an
/// uninterruptible wait, usually blocked on I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatusKind {
    Running,
    Sleeping,
    Uninterruptible,
    Stopped,
    Zombie,
    #[default]
    Other,
}

impl ProcessStatusKind {
    #[must_use]
    pub fn from_provider_status(status: &str) -> Self {
        match status.trim().to_ascii_lowercase().as_str() {
            "r" | "running" | "runnable" => Self::Running,
            "s" | "sleeping" => Self::Sleeping,
            "d" | "uninterruptibledisksleep" | "uninterruptible disk sleep" => {
                Self::Uninterruptible
            }
            "t" | "stopped" | "tracing" => Self::Stopped,
            "z" | "zombie" => Self::Zombie,
            _ => Self::Other,
        }
    }

    #[must_use]
    pub const fn is_uninterruptible(self) -> bool {
        matches!(self, Self::Uninterruptible)
    }

    /// Whether this process is a zombie whose parent has not reaped it.
    #[must_use]
    pub const fn is_zombie(self) -> bool {
        matches!(self, Self::Zombie)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessItem {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub cmdline: String,
    pub status: String,
    /// Linux/OS scheduling policy (SCHED_OTHER, SCHED_BATCH, SCHED_IDLE, etc.).
    pub scheduling_policy: Option<ProcessSchedulingPolicy>,
    /// Linux OOM score (0..1000).
    pub oom_score: Option<u32>,
    /// Cumulative minor page faults (loaded without I/O).
    pub minor_page_faults: Option<u64>,
    /// Cumulative major page faults (loaded from disk I/O).
    pub major_page_faults: Option<u64>,
    /// Cumulative writes discarded by the kernel after an overwrite or
    /// truncation (`/proc/<pid>/io:cancelled_write_bytes`).
    pub cancelled_write_bytes: Option<u64>,
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

    /// Classify the provider status without making frontend code parse a
    /// platform-specific spelling. Linux `D` remains a first-class state.
    #[must_use]
    pub fn status_kind(&self) -> ProcessStatusKind {
        ProcessStatusKind::from_provider_status(&self.status)
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

    /// Return the active scheduling policy (SCHED_OTHER, SCHED_BATCH, SCHED_IDLE, etc.).
    #[must_use]
    pub const fn current_scheduling_policy(&self) -> Option<ProcessSchedulingPolicy> {
        self.scheduling_policy
    }

    /// Return the current OOM killer score (0..1000), if known.
    #[must_use]
    pub const fn current_oom_score(&self) -> Option<u32> {
        self.oom_score
    }

    /// Return the current (minor, major) page fault counts, if known.
    #[must_use]
    pub const fn current_page_faults(&self) -> Option<(u64, u64)> {
        match (self.minor_page_faults, self.major_page_faults) {
            (Some(minor), Some(major)) => Some((minor, major)),
            _ => None,
        }
    }

    /// Return the cumulative amount of process writes cancelled by the kernel.
    #[must_use]
    pub const fn current_cancelled_write_bytes(&self) -> Option<u64> {
        self.cancelled_write_bytes
    }

    /// Return whether Proportional Set Size (PSS) is currently available for this process.
    #[must_use]
    pub fn has_memory_pss(&self) -> bool {
        self.current_memory_pss_bytes().is_some()
    }

    /// Return whether Unique Set Size (USS) is currently available for this process.
    #[must_use]
    pub fn has_memory_uss(&self) -> bool {
        self.current_memory_uss_bytes().is_some()
    }

    /// Return whether the current transparent huge-page charge is available.
    #[must_use]
    pub fn has_memory_anon_huge_pages(&self) -> bool {
        self.current_memory_anon_huge_pages_bytes().is_some()
    }

    /// Return the observed anonymous huge-page charge as a percentage of RSS.
    /// The ratio is only available when both counters are current and RSS is
    /// non-zero; a missing denominator never becomes a made-up percentage.
    #[must_use]
    pub fn current_memory_anon_huge_pages_ratio(&self) -> Option<f32> {
        match (
            self.current_memory_anon_huge_pages_bytes(),
            self.current_memory_bytes(),
        ) {
            (Some(huge_pages), Some(rss)) if rss > 0 => {
                Some((huge_pages as f64 / rss as f64 * 100.0).min(100.0) as f32)
            }
            _ => None,
        }
    }

    /// Return the current comprehensive memory breakdown for this process.
    ///
    /// Presents a unified snapshot of Resident Set Size (RSS), Proportional Set
    /// Size (PSS), Unique Set Size (USS), derived shared physical memory, and swap.
    /// Unmeasured or unavailable metrics remain `None` to preserve typed
    /// availability honesty without fabricated zeroes.
    #[must_use]
    pub fn current_memory_breakdown(&self) -> ProcessMemoryBreakdown {
        ProcessMemoryBreakdown::new(
            self.current_memory_bytes(),
            self.current_memory_pss_bytes(),
            self.current_memory_uss_bytes(),
            self.current_swap_bytes(),
        )
    }

    /// Return the estimated shared physical memory in bytes (`RSS - USS`).
    ///
    /// Represents the portion of this process's resident physical memory that is
    /// shared with other processes (e.g., dynamically linked shared libraries,
    /// shared memory mappings).
    ///
    /// Returns `Some(rss.saturating_sub(uss))` only when both RSS
    /// ([`Self::current_memory_bytes`]) and USS ([`Self::current_memory_uss_bytes`])
    /// observations are currently available. Returns `None` if either observation
    /// is missing or unavailable.
    #[must_use]
    pub fn current_memory_shared_bytes(&self) -> Option<u64> {
        match (self.current_memory_bytes(), self.current_memory_uss_bytes()) {
            (Some(rss), Some(uss)) => Some(rss.saturating_sub(uss)),
            _ => None,
        }
    }

    /// Return the estimated proportional share of shared physical memory in bytes (`PSS - USS`).
    ///
    /// In PSS accounting, `PSS = USS + (shared / sharing_count)`. The difference
    /// `PSS - USS` therefore quantifies the proportional burden of shared pages
    /// attributed to this process.
    ///
    /// Returns `Some(pss.saturating_sub(uss))` only when both PSS
    /// ([`Self::current_memory_pss_bytes`]) and USS ([`Self::current_memory_uss_bytes`])
    /// observations are currently available. Returns `None` if either observation
    /// is missing or unavailable.
    #[must_use]
    pub fn current_memory_proportional_shared_bytes(&self) -> Option<u64> {
        match (
            self.current_memory_pss_bytes(),
            self.current_memory_uss_bytes(),
        ) {
            (Some(pss), Some(uss)) => Some(pss.saturating_sub(uss)),
            _ => None,
        }
    }

    /// Return the fraction of resident memory (RSS) uniquely dedicated to this
    /// process (`USS / RSS`).
    ///
    /// Returns a ratio in the range `0.0..=1.0`. A ratio close to `1.0` indicates
    /// the process memory is almost entirely private/unshared; a lower ratio
    /// indicates heavy reliance on shared libraries or memory segments.
    ///
    /// Returns `None` if either RSS or USS is unavailable, or if RSS is 0.
    #[must_use]
    pub fn current_memory_uss_ratio(&self) -> Option<f32> {
        match (self.current_memory_bytes(), self.current_memory_uss_bytes()) {
            (Some(rss), Some(uss)) if rss > 0 => {
                let ratio = uss as f32 / rss as f32;
                Some(ratio.clamp(0.0, 1.0))
            }
            _ => None,
        }
    }

    /// Return the fraction of resident memory (RSS) accounted for by PSS (`PSS / RSS`).
    ///
    /// Returns a ratio in the range `0.0..=1.0`. Represents the relative weight
    /// of proportional memory versus total resident set size.
    ///
    /// Returns `None` if either RSS or PSS is unavailable, or if RSS is 0.
    #[must_use]
    pub fn current_memory_pss_ratio(&self) -> Option<f32> {
        match (self.current_memory_bytes(), self.current_memory_pss_bytes()) {
            (Some(rss), Some(pss)) if rss > 0 => {
                let ratio = pss as f32 / rss as f32;
                Some(ratio.clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

/// Comprehensive physical and virtual memory breakdown for a single process.
///
/// Under modern operating system memory accounting (e.g. Linux `/proc/[pid]/smaps_rollup`,
/// Windows Working Set private/shared, macOS task memory info), process memory consists
/// of several distinct layers:
/// - **RSS** (Resident Set Size): Total physical RAM currently mapped into the process's
///   address space, encompassing private unshared memory as well as all shared pages.
/// - **PSS** (Proportional Set Size): Proportional share of physical RAM, calculated by
///   dividing each shared memory page by the number of processes mapping it and adding
///   that fraction to the process's private unshared pages.
/// - **USS** (Unique Set Size): Private physical RAM exclusively owned by this process.
///   This represents the exact amount of physical memory that would be returned to the OS
///   if the process terminates immediately.
/// - **Shared**: Estimated shared physical memory (`RSS - USS`), representing pages
///   shared among multiple processes (such as dynamic libraries or shared memory).
/// - **Swap**: Virtual memory paged out to disk for this process.
///
/// All fields preserve typed availability: unmeasured or temporarily unavailable
/// metrics remain `None` rather than fabricated zeroes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProcessMemoryBreakdown {
    /// Resident Set Size (RSS) in bytes, if measured and available.
    pub rss_bytes: Option<u64>,
    /// Proportional Set Size (PSS) in bytes, if measured and available.
    pub pss_bytes: Option<u64>,
    /// Unique Set Size (USS) in bytes, if measured and available.
    pub uss_bytes: Option<u64>,
    /// Derived shared physical memory (`RSS - USS`) in bytes, if both RSS and USS are available.
    pub shared_bytes: Option<u64>,
    /// Swap memory in bytes charged to this process, if measured and available.
    pub swap_bytes: Option<u64>,
}

impl ProcessMemoryBreakdown {
    /// Construct a memory breakdown with derived shared memory computed automatically.
    #[must_use]
    pub const fn new(
        rss_bytes: Option<u64>,
        pss_bytes: Option<u64>,
        uss_bytes: Option<u64>,
        swap_bytes: Option<u64>,
    ) -> Self {
        let shared_bytes = match (rss_bytes, uss_bytes) {
            (Some(rss), Some(uss)) => Some(rss.saturating_sub(uss)),
            _ => None,
        };
        Self {
            rss_bytes,
            pss_bytes,
            uss_bytes,
            shared_bytes,
            swap_bytes,
        }
    }

    /// Return whether Unique Set Size (USS) is present.
    #[must_use]
    pub const fn has_uss(&self) -> bool {
        self.uss_bytes.is_some()
    }

    /// Return whether Proportional Set Size (PSS) is present.
    #[must_use]
    pub const fn has_pss(&self) -> bool {
        self.pss_bytes.is_some()
    }

    /// Return the proportional share of shared memory (`PSS - USS`) in bytes, if both are present.
    ///
    /// In PSS accounting, `PSS = USS + (shared / sharing_count)`. The difference
    /// `PSS - USS` therefore quantifies the proportional burden of shared pages
    /// attributed to this process.
    #[must_use]
    pub const fn proportional_shared_bytes(&self) -> Option<u64> {
        match (self.pss_bytes, self.uss_bytes) {
            (Some(pss), Some(uss)) => Some(pss.saturating_sub(uss)),
            _ => None,
        }
    }

    /// Return the fraction of resident memory (RSS) that is private/unique to this process (`USS / RSS`).
    ///
    /// Returns a ratio in the range `0.0..=1.0`. A ratio close to `1.0` indicates
    /// the process memory is almost entirely private/unshared; a lower ratio
    /// indicates heavy reliance on shared libraries or memory segments.
    ///
    /// Returns `None` if RSS or USS is unavailable or if RSS is 0.
    #[must_use]
    pub fn uss_to_rss_ratio(&self) -> Option<f32> {
        match (self.rss_bytes, self.uss_bytes) {
            (Some(rss), Some(uss)) if rss > 0 => {
                let ratio = uss as f32 / rss as f32;
                Some(ratio.clamp(0.0, 1.0))
            }
            _ => None,
        }
    }

    /// Return the fraction of resident memory (RSS) accounted for by PSS (`PSS / RSS`).
    ///
    /// Returns a ratio in the range `0.0..=1.0`. Represents the relative weight
    /// of proportional memory versus total resident set size.
    ///
    /// Returns `None` if RSS or PSS is unavailable or if RSS is 0.
    #[must_use]
    pub fn pss_to_rss_ratio(&self) -> Option<f32> {
        match (self.rss_bytes, self.pss_bytes) {
            (Some(rss), Some(pss)) if rss > 0 => {
                let ratio = pss as f32 / rss as f32;
                Some(ratio.clamp(0.0, 1.0))
            }
            _ => None,
        }
    }

    /// Return the fraction of resident memory (RSS) that is shared with other processes (`Shared / RSS`).
    ///
    /// Returns a ratio in the range `0.0..=1.0`.
    /// Returns `None` if RSS or derived shared memory is unavailable, or if RSS is 0.
    #[must_use]
    pub fn shared_to_rss_ratio(&self) -> Option<f32> {
        match (self.rss_bytes, self.shared_bytes) {
            (Some(rss), Some(shared)) if rss > 0 => {
                let ratio = shared as f32 / rss as f32;
                Some(ratio.clamp(0.0, 1.0))
            }
            _ => None,
        }
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

/// Sort key options for ordering process lists and process trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSortKey {
    /// Process ID.
    Pid,
    /// Process name (case-insensitive ASCII order).
    Name,
    /// Instantaneous CPU usage percentage.
    CpuUsage,
    /// Resident Set Size (RSS) memory in bytes.
    Memory,
    /// Proportional Set Size (PSS) memory in bytes.
    MemoryPss,
    /// Unique Set Size (USS) private memory in bytes.
    MemoryUss,
    /// Disk read throughput rate in bytes per second.
    DiskRead,
    /// Disk write throughput rate in bytes per second.
    DiskWrite,
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

/// Resolve the stable application label when the provider has proved one;
/// otherwise preserve the legacy process-name normalization fallback.
/// Resolve the stable application label when the provider has proved one;
/// otherwise preserve the legacy process-name normalization fallback.
#[must_use]
pub fn application_group_name(item: &ProcessItem) -> String {
    item.current_application_name()
        .map(str::to_owned)
        .unwrap_or_else(|| normalize_app_name(&item.name, &item.cmdline))
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
/// the typed type-group aggregate uses so renderers treat type-groups and
/// app-groups uniformly; callers do not need this directly unless labeling a
/// [`ProcessType`] without a group.
#[must_use]
pub const fn process_type_label(process_type: ProcessType) -> &'static str {
    match process_type {
        ProcessType::Userspace => "Userspace",
        ProcessType::Kernel => "Kernel",
    }
}

#[must_use]
pub fn flatten_tree_visible<'a>(
    nodes: &[ProcessNode<'a>],
    collapsed: &HashSet<ProcessLiveKey>,
) -> Vec<FlatTreeNode<'a>> {
    fn visit<'a>(
        nodes: &[ProcessNode<'a>],
        collapsed: &HashSet<ProcessLiveKey>,
        rows: &mut Vec<FlatTreeNode<'a>>,
    ) {
        for node in nodes {
            let has_children = !node.children.is_empty();
            rows.push(FlatTreeNode {
                item: node.item,
                depth: node.depth,
                has_children,
            });
            let is_collapsed = ProcessLiveKey::from_process(node.item)
                .is_some_and(|identity| collapsed.contains(&identity));
            if has_children && !is_collapsed {
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
        ProcessSortKey::MemoryPss => left
            .current_memory_pss_bytes()
            .cmp(&right.current_memory_pss_bytes()),
        ProcessSortKey::MemoryUss => left
            .current_memory_uss_bytes()
            .cmp(&right.current_memory_uss_bytes()),
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

#[cfg(test)]
#[path = "../../tests/headless/core_core_process_memory_tests.rs"]
mod memory_tests;

#[cfg(test)]
#[path = "../../tests/headless/core_core_process_typed_group_tests.rs"]
mod typed_group_tests;

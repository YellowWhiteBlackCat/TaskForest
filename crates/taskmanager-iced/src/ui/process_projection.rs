//! The Applications-page visible-row projection (F09/F10 parity).
//!
//! One pure function, [`ProcessProjection::project`], turns the shared flat
//! sorted process list plus the frontend-local expansion sets
//! into the rows the table actually renders. Both the renderer
//! (`applications_page`) and the keyboard paths (`IcedApp` visual navigation)
//! consume this same projection, so paging can never diverge from pixels
//! (ADR-020 render-time projection: a row the keyboard reaches is a row that
//! was rendered).
//!
//! ## Cross-frame memoization
//!
//! iced repaints at vsync (~60 Hz) while the shared process list advances
//! only ~1 Hz, and the frontend-local view state (status/sort/expand/query)
//! changes only on user input. Most view frames therefore would rebuild an
//! O(N) byte-identical projection. [`ProcessProjectionFingerprint`] captures
//! every input that shapes the rows (the shell's process-domain revision
//! watermark + the status filter + the active sort + the query
//! + the expand sets);
//!   [`IcedApp::projected_rows`](crate::app::IcedApp::projected_rows) compares
//!   it against the previously rendered frame and reuses the cached rows on a
//!   hit (mirrors the round-2 [`crate::ui::process_sparkline`] canvas cache).
//!
//! Category buckets keep the fixed core `ProcessCategory::ALL` order;
//! application aggregates and recursive nodes sort by the active column.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use taskmanager_application::i18n::t;
use taskmanager_application::process_category_projection::{
    category_buckets, category_expansion_key,
};
use taskmanager_application::{
    AppGroup, ProcessCategory, ProcessItem, ProcessNode, build_process_tree, process_category,
    sort_apps,
};
use taskmanager_shell::presentation::{
    bytes, missing_value, optional_bytes, optional_count, optional_duration, optional_nice,
};
use taskmanager_shell::{ProcessRowKey, ProcessStatusFilter, SortCol, SortDir};

mod category;

/// One visible row of the Applications table's canonical hierarchy.
///
/// Every variant carries the row's `flat_index` — its position in the shared
/// `visible_processes()` list — so selection, focus, and the shared action
/// paths (`Enter` properties, `Delete` end-task, batch verbs) always resolve
/// to the process the row actually renders.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectedRow {
    /// A category or application aggregate header. The summed
    /// observation cells are carried here (computed once by the projection)
    /// so the renderer never re-aggregates; `None` fields render as dashes.
    /// `expansion_key` is the membership key in the frontend-local expand
    /// set — a `category:<stable_key>` category token or `app-tree:<pid>`
    /// application token (locale-neutral, so
    /// a language switch never orphans an expansion); `name` is the display
    /// label only.
    GroupHeader {
        flat_index: usize,
        main_pid: u32,
        /// Selectable semantic row identity. Category/type headers are
        /// structural (`None`); application aggregates carry their root key.
        row_key: Option<ProcessRowKey>,
        name: String,
        expansion_key: String,
        member_count: usize,
        expanded: bool,
        /// Sum of the members' CPU% (the aggregator's `total_cpu_usage`).
        cpu: f32,
        /// Sum of the members' PSS observations (the PSS column).
        pss: Option<u64>,
        /// Sum of the members' RSS observations (the Memory column fallback).
        memory_rss: Option<u64>,
        swap: Option<u64>,
        disk_read: Option<u64>,
        disk_write: Option<u64>,
        threads: Option<u32>,
        cpu_time: Option<u64>,
        fds: Option<u32>,
        /// Facts inherited from the group's main process (typed observations —
        /// an unavailable nice/start renders an honest dash, like gpui).
        user: String,
        status: String,
        nice: Option<i32>,
        start_time_secs: Option<u64>,
        start_clock: String,
    },
    /// A process node within the canonical recursive hierarchy.
    Tree {
        flat_index: usize,
        pid: u32,
        depth: usize,
        has_children: bool,
        /// Whether the subtree is currently collapsed (present in the
        /// frontend-local collapsed-pid set).
        collapsed: bool,
        parent_pid: Option<u32>,
        /// Pre-formatted display strings (see [`RowCells`]).
        cells: RowCells,
    },
}

impl ProjectedRow {
    /// The row's position in the shared flat `visible_processes()` list.
    #[must_use]
    pub(crate) const fn flat_index(&self) -> usize {
        match self {
            Self::GroupHeader { flat_index, .. } | Self::Tree { flat_index, .. } => *flat_index,
        }
    }

    #[must_use]
    pub(crate) const fn row_key(&self) -> Option<ProcessRowKey> {
        match self {
            Self::Tree { pid, .. } => Some(ProcessRowKey::Process(*pid)),
            Self::GroupHeader { row_key, .. } => *row_key,
        }
    }
}

/// Per-row display strings, pre-formatted by the projection so the renderer
/// pays an `Rc`-cheap clone instead of ~14 `format!`/`to_string` allocations
/// per row per frame (the projection memoizes; the view re-renders far more
/// often than the process list ticks). Each string is exactly what the row
/// element previously formatted inline, dashes included.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RowCells {
    pub pid: String,
    pub cpu: String,
    pub memory: String,
    pub pss: String,
    pub swap: String,
    pub disk_read: String,
    pub disk_write: String,
    pub cpu_time: String,
    pub threads: String,
    pub user: String,
    pub status: String,
    pub fds: String,
    pub nice: String,
    pub start_clock: String,
}

/// Owned process facts needed by the row renderer after projection. Keeping
/// these beside the memoized rows removes the renderer's dependency on the
/// live `ProcessItem` slice: a lazy Iced table can retain one owned widget tree
/// across frames without borrowing `ShellApp` or cloning process histories on
/// every view call. The history buffer is copied only when the projection
/// itself misses, then shared by every canvas program for that row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcessRowFacts {
    pub pid: u32,
    pub name: String,
    pub cpu_history: Rc<[f32]>,
    pub cpu_zero: bool,
    pub memory_zero: bool,
    pub pss_zero: bool,
    pub swap_zero: bool,
    pub disk_read_zero: bool,
    pub disk_write_zero: bool,
    pub cpu_time_zero: bool,
    pub threads_zero: bool,
    pub fds_zero: bool,
    pub nice_zero: bool,
}

impl ProcessRowFacts {
    fn from_process(process: &ProcessItem) -> Self {
        Self {
            pid: process.pid,
            name: process.name.clone(),
            cpu_history: Rc::from(process.cpu_history.clone().into_boxed_slice()),
            cpu_zero: process.current_cpu_percentage() == Some(0.0),
            memory_zero: process.current_memory_bytes() == Some(0),
            pss_zero: process.current_memory_pss_bytes() == Some(0),
            swap_zero: process.current_swap_bytes() == Some(0),
            disk_read_zero: process.current_disk_read_bytes_per_sec() == Some(0),
            disk_write_zero: process.current_disk_write_bytes_per_sec() == Some(0),
            cpu_time_zero: process.current_cpu_time_secs() == Some(0),
            threads_zero: process.current_threads() == Some(0),
            fds_zero: process.current_fds() == Some(0),
            nice_zero: process.current_nice() == Some(0),
        }
    }
}

/// Visibility: `pub(crate)` so the Apps-page tests can assert the exact
/// pre-formatted cell strings (the old per-frame helper's contract). CPU, fds,
/// nice, and the start clock read the typed `current_*` observations (like the
/// gpui `VisibleRow` projection): an unavailable or stale scalar renders an
/// honest dash instead of the legacy field's zero sentinel.
pub(crate) fn build_row_cells(process: &ProcessItem) -> RowCells {
    build_row_cells_with_rules(
        process,
        &taskmanager_application::LocalTimeRulesObservation::unsupported(0),
    )
}

fn build_row_cells_with_rules(
    process: &ProcessItem,
    local_time_rules: &taskmanager_application::LocalTimeRulesObservation,
) -> RowCells {
    RowCells {
        pid: process.pid.to_string(),
        cpu: process
            .current_cpu_percentage()
            .map_or_else(missing_value, |cpu| format!("{cpu:>5.1}%")),
        memory: process
            .current_memory_bytes()
            .map_or_else(missing_value, bytes),
        pss: process
            .current_memory_pss_bytes()
            .map_or_else(missing_value, bytes),
        swap: process
            .current_swap_bytes()
            .map_or_else(missing_value, bytes),
        disk_read: optional_bytes(process.current_disk_read_bytes_per_sec()),
        disk_write: optional_bytes(process.current_disk_write_bytes_per_sec()),
        cpu_time: optional_duration(process.current_cpu_time_secs()),
        threads: optional_count(process.current_threads()),
        user: process.current_user().unwrap_or_default(),
        status: process.status.clone(),
        fds: optional_count(process.current_fds()),
        nice: optional_nice(process.current_nice()),
        start_clock: taskmanager_shell::presentation::start_clock_local(
            process.current_start_time_secs(),
            local_time_rules,
        ),
    }
}

/// The ordered rows the Applications table renders in the active view mode.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ProcessProjection {
    rows: Vec<ProjectedRow>,
    process_facts: Vec<ProcessRowFacts>,
}

impl ProcessProjection {
    /// Build the projection for the shared flat list and the current
    /// frontend-local view state. Pure: the renderer and the keyboard paths
    /// call it with the same inputs and must get the same rows.
    #[must_use]
    pub(crate) fn project_with_local_time(
        flat: &[&ProcessItem],
        sort: (SortCol, SortDir),
        expanded_groups: &HashSet<String>,
        expanded_tree: &HashSet<u32>,
        local_time_rules: &taskmanager_application::LocalTimeRulesObservation,
    ) -> Self {
        let by_pid: HashMap<u32, usize> = flat
            .iter()
            .enumerate()
            .map(|(index, process)| (process.pid, index))
            .collect();
        let process_facts = flat
            .iter()
            .map(|process| ProcessRowFacts::from_process(process))
            .collect();
        let mut rows = category::category_rows(flat, &by_pid, sort, expanded_groups, expanded_tree);
        for row in &mut rows {
            match row {
                ProjectedRow::Tree {
                    flat_index, cells, ..
                } => {
                    if let Some(process) = flat.get(*flat_index) {
                        *cells = build_row_cells_with_rules(process, local_time_rules);
                    }
                }
                ProjectedRow::GroupHeader {
                    start_time_secs,
                    start_clock,
                    ..
                } => {
                    *start_clock = taskmanager_shell::presentation::start_clock_local(
                        *start_time_secs,
                        local_time_rules,
                    );
                }
            }
        }
        Self {
            rows,
            process_facts,
        }
    }

    /// Every projected row in render order.
    #[must_use]
    pub(crate) fn rows(&self) -> &[ProjectedRow] {
        &self.rows
    }

    /// Resolve the owned facts for one flat visible-row index. The index is
    /// the same coordinate carried by every projected row variant.
    #[must_use]
    pub(crate) fn process_facts(&self, flat_index: usize) -> Option<&ProcessRowFacts> {
        self.process_facts.get(flat_index)
    }

    /// The visual position of the row backed by `flat_index` (the shared
    /// cursor's row), if any.
    #[must_use]
    pub(crate) fn visual_index_of_flat(&self, flat_index: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.flat_index() == flat_index)
    }

    /// The row at one visual position, if any.
    #[must_use]
    pub(crate) fn row_at(&self, visual: usize) -> Option<&ProjectedRow> {
        self.rows.get(visual)
    }

    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }
}

/// The cached identity of one Applications-page projection — every input that
/// shapes the rows [`ProcessProjection::project`] emits. Two projections built
/// from equal fingerprints produce byte-identical rows, so the O(N) rebuild
/// can be skipped between identical fingerprints (the round-3 vsync-frame
/// memo on [`IcedApp`](crate::app::IcedApp)).
///
/// Inputs:
/// - `data_revision` — the shell's process-domain revision, bumped when a new
///   process snapshot is folded. Unrelated system, service, startup, and
///   session batches must not invalidate this O(N) projection.
/// - `status_filter` — the active six-state process bucket (shared shell
///   projection).
/// - `sort` — the active process-table column + direction (shared shell
///   state, drives both the data-layer `visible_processes` sort and the
///   view-layer group/tree comparators).
/// - `query` — the shared Applications-page filter (drives the `visible_processes`
///   filter; non-empty changes which processes the projection sees).
/// - `expanded_groups` — frontend-local category/application expand set.
/// - `expanded_tree` — frontend-local collapsed-pid set.
///
/// Notably the fingerprint does NOT carry the visible list itself — that list
/// is fully determined by `(data_revision, query, sort)` through the pure
/// shell filter + sort, so the three scalars are a sufficient change signal
/// (verifiable by reading [`taskmanager_shell::ShellApp::visible_processes`]).
/// This keeps the fingerprint O(1) in the process count for the scalar parts
/// and O(|expand sets|) for the membership parts (both bounded by user
/// interaction, not by the system process count).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ProcessProjectionFingerprint {
    data_revision: u64,
    status_filter: ProcessStatusFilter,
    sort: (SortCol, SortDir),
    query: String,
    expanded_groups: HashSet<String>,
    expanded_tree: HashSet<u32>,
    local_time_rules: Option<taskmanager_application::LocalTimeRulesCacheKey>,
}

impl ProcessProjectionFingerprint {
    /// Build a fingerprint with the active process-state bucket. The
    /// compatibility-shaped [`Self::build`] keeps pure projection tests that
    /// do not model the optional filter concise; the live Iced cache uses this
    /// explicit form so changing a pill cannot reuse stale rows.
    #[must_use]
    pub(crate) fn build_with_status(
        data_revision: u64,
        status_filter: ProcessStatusFilter,
        sort: (SortCol, SortDir),
        query: &str,
        expanded_groups: &HashSet<String>,
        expanded_tree: &HashSet<u32>,
    ) -> Self {
        Self {
            data_revision,
            status_filter,
            sort,
            query: query.to_owned(),
            expanded_groups: expanded_groups.clone(),
            expanded_tree: expanded_tree.clone(),
            local_time_rules: None,
        }
    }

    #[must_use]
    pub(crate) fn with_local_time_rules(
        mut self,
        rules: &taskmanager_application::LocalTimeRulesObservation,
    ) -> Self {
        self.local_time_rules = Some(rules.cache_key());
        self
    }
}

/// PSS-preferred memory read for one process, mirroring gpui's
/// `memory_for_display` so aggregate values and group sorts agree with the
/// baseline frontend.
fn memory_for_display(process: &ProcessItem) -> Option<u64> {
    process
        .current_memory_pss_bytes()
        .or_else(|| process.current_memory_bytes())
}

/// Summed observations of a group's members, each `None` when every member
/// lacks the observation (all-or-nothing, like gpui's `sum_opt_*`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct GroupTotals {
    pss: Option<u64>,
    memory_rss: Option<u64>,
    swap: Option<u64>,
    disk_read: Option<u64>,
    disk_write: Option<u64>,
    threads: Option<u32>,
    cpu_time: Option<u64>,
    fds: Option<u32>,
}

fn sum_opt_u64(acc: Option<u64>, value: Option<u64>) -> Option<u64> {
    match (acc, value) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn sum_opt_u32(acc: Option<u32>, value: Option<u32>) -> Option<u32> {
    match (acc, value) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn group_totals(
    group: &AppGroup,
    by_pid: &HashMap<u32, usize>,
    flat: &[&ProcessItem],
) -> GroupTotals {
    let mut totals = GroupTotals::default();
    for pid in &group.pids {
        let Some(process) = by_pid.get(pid).and_then(|index| flat.get(*index)).copied() else {
            continue;
        };
        totals.pss = sum_opt_u64(totals.pss, process.current_memory_pss_bytes());
        totals.memory_rss = sum_opt_u64(totals.memory_rss, process.current_memory_bytes());
        totals.swap = sum_opt_u64(totals.swap, process.current_swap_bytes());
        totals.disk_read = sum_opt_u64(totals.disk_read, process.current_disk_read_bytes_per_sec());
        totals.disk_write = sum_opt_u64(
            totals.disk_write,
            process.current_disk_write_bytes_per_sec(),
        );
        totals.threads = sum_opt_u32(totals.threads, process.current_threads());
        totals.cpu_time = sum_opt_u64(totals.cpu_time, process.current_cpu_time_secs());
        totals.fds = sum_opt_u32(totals.fds, process.current_fds());
    }
    totals
}

/// The metric a group sorts by for one memory-shaped column (PSS-preferred
/// like gpui's `group_metric`).
fn group_metric(
    group: &AppGroup,
    by_pid: &HashMap<u32, usize>,
    flat: &[&ProcessItem],
    column: SortCol,
) -> Option<u64> {
    group
        .pids
        .iter()
        .filter_map(|pid| by_pid.get(pid).and_then(|index| flat.get(*index)).copied())
        .fold(None, |total, process| {
            let value = match column {
                SortCol::Memory | SortCol::Pss => memory_for_display(process),
                SortCol::Swap => process.current_swap_bytes(),
                _ => None,
            };
            sum_opt_u64(total, value)
        })
}

/// Re-sort the aggregate groups by the active table column, mirroring gpui's
/// `sort_groups`: memory-shaped columns sum PSS-preferred metrics; every
/// other column rides the shared `sort_apps` with the shell's canonical
/// `aggregate_sort_key` fallback.
fn sort_groups(
    groups: &mut [AppGroup],
    by_pid: &HashMap<u32, usize>,
    flat: &[&ProcessItem],
    sort: (SortCol, SortDir),
) {
    let (column, direction) = sort;
    let ascending = direction == SortDir::Asc;
    if !matches!(column, SortCol::Memory | SortCol::Pss | SortCol::Swap) {
        sort_apps(
            groups,
            taskmanager_shell::aggregate_sort_key(column),
            ascending,
        );
        return;
    }
    groups.sort_by(|left, right| {
        let ordering = group_metric(left, by_pid, flat, column)
            .cmp(&group_metric(right, by_pid, flat, column))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.main_pid.cmp(&right.main_pid));
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
}

/// Recursively sort a process tree by the active column. EVERY column sorts
/// through the neutral comparator (`taskmanager_application::process_sort`)
/// via the shell's single `sort_axis` translation — the same ordering the
/// GPUI canonical tree and shell `visible_processes` paths apply — so recursive
/// process ordering cannot drift between frontends.
fn sort_tree(nodes: &mut [ProcessNode<'_>], sort: (SortCol, SortDir)) {
    let (column, direction) = sort;
    let ascending = direction == SortDir::Asc;
    let axis = taskmanager_shell::sort_axis(column);
    // The comparator already carries the direction (the neutral
    // `compare_processes` applies it plus the direction-independent pid
    // tie-break); the recursion below only forwards it.
    sort_nodes_by(nodes, &|left, right| {
        taskmanager_application::process_sort::compare_processes(left, right, axis, ascending)
    });
}

fn sort_nodes_by<'a, F>(nodes: &mut [ProcessNode<'a>], cmp: &F)
where
    F: Fn(&ProcessItem, &ProcessItem) -> std::cmp::Ordering,
{
    nodes.sort_by(|left, right| cmp(left.item, right.item));
    for node in nodes.iter_mut() {
        sort_nodes_by(&mut node.children, cmp);
    }
}

/// Depth-first flatten that also records each node's parent pid, so Left on a
/// collapsed node can move the cursor up to its parent (gpui parity).
fn flatten_with_parents<'a>(
    nodes: &[ProcessNode<'a>],
    collapsed: &HashSet<u32>,
    by_pid: &HashMap<u32, usize>,
    rows: &mut Vec<ProjectedRow>,
    depth: usize,
    parent_pid: Option<u32>,
) {
    for node in nodes {
        let has_children = !node.children.is_empty();
        if let Some(flat_index) = by_pid.get(&node.item.pid).copied() {
            rows.push(ProjectedRow::Tree {
                flat_index,
                pid: node.item.pid,
                depth,
                has_children,
                collapsed: has_children && collapsed.contains(&node.item.pid),
                parent_pid,
                cells: build_row_cells(node.item),
            });
        }
        if has_children && !collapsed.contains(&node.item.pid) {
            flatten_with_parents(
                &node.children,
                collapsed,
                by_pid,
                rows,
                depth + 1,
                Some(node.item.pid),
            );
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/process_projection/tests.rs"]
mod tests;

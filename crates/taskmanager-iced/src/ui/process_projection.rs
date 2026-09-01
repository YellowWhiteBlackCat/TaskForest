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

use std::collections::HashSet;
use std::rc::Rc;

use taskmanager_application::i18n::t;
use taskmanager_core::core::process::{ProcessCategory, ProcessItem, ProcessLiveKey};
use taskmanager_core::core::time::LocalTimeRulesObservation;

use taskmanager_shell::presentation::{
    bytes, missing_value, optional_bytes, optional_count, optional_duration, optional_nice,
};
use taskmanager_shell::{
    ProcessRowAggregate, ProcessRowId, ProcessStatusFilter, ProcessTreeRow, SortCol, SortDir,
    project_process_tree_rows,
};

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
    /// set — a `category:<stable_key>` category token or `app-tree:<live-key>`
    /// application token (locale-neutral, so
    /// a language switch never orphans an expansion); `name` is the display
    /// label only.
    GroupHeader {
        flat_index: usize,
        main_pid: u32,
        /// Selectable semantic row identity. Category/type headers are
        /// structural (`None`); application aggregates carry their root key.
        row_key: Option<ProcessRowId>,
        name: String,
        expansion_key: String,
        member_count: usize,
        expanded: bool,
        /// All group scalar cells retain their typed availability and
        /// coverage. Renderers may choose a current-value string, but they
        /// cannot mistake an unavailable aggregate for a measured zero.
        metrics: Box<ProcessRowAggregate>,
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
        /// Selectable semantic row identity (CORE-01: pid + start token).
        row_key: Option<ProcessRowId>,
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
    pub(crate) const fn row_key(&self) -> Option<ProcessRowId> {
        match self {
            Self::Tree { row_key, .. } => *row_key,
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

fn category_label(category: ProcessCategory) -> &'static str {
    match category {
        ProcessCategory::Application => t("proc.category_apps"),
        ProcessCategory::Background => t("proc.category_background"),
        ProcessCategory::Uncategorized => t("proc.category_uncategorized"),
    }
}

fn projected_row_from_shared(
    row: ProcessTreeRow,
    flat: &[&ProcessItem],
    local_time_rules: &LocalTimeRulesObservation,
) -> Option<ProjectedRow> {
    match row {
        ProcessTreeRow::Category {
            category,
            expansion_key,
            representative_index,
            expanded,
            member_count,
            aggregate,
        } => group_header_from_shared(
            flat.get(representative_index)?,
            GroupHeaderInput {
                row_key: None,
                expansion_key,
                name: category_label(category).to_owned(),
                expanded,
                member_count,
                aggregate,
                flat_index: representative_index,
                local_time_rules,
            },
        ),
        ProcessTreeRow::Application {
            visible_index,
            row_key,
            expansion_key,
            expanded,
            member_count,
            aggregate,
            ..
        } => {
            let root = flat.get(visible_index)?;
            group_header_from_shared(
                root,
                GroupHeaderInput {
                    row_key,
                    expansion_key,
                    name: root
                        .current_application_name()
                        .unwrap_or(root.name.as_str())
                        .to_owned(),
                    expanded,
                    member_count,
                    aggregate,
                    flat_index: visible_index,
                    local_time_rules,
                },
            )
        }
        ProcessTreeRow::Process {
            visible_index,
            row_key,
            parent_key,
            depth,
            has_children,
            collapsed,
        } => {
            let process = flat.get(visible_index)?;
            let parent_pid = parent_key.and_then(|key| match key {
                ProcessRowId::Process(identity) => Some(identity.pid()),
                ProcessRowId::Category(_) | ProcessRowId::Application(_) => None,
            });
            Some(ProjectedRow::Tree {
                flat_index: visible_index,
                pid: process.pid,
                row_key,
                depth,
                has_children,
                collapsed,
                parent_pid,
                cells: build_row_cells_with_rules(process, local_time_rules),
            })
        }
    }
}

struct GroupHeaderInput<'a> {
    row_key: Option<ProcessRowId>,
    expansion_key: String,
    name: String,
    expanded: bool,
    member_count: usize,
    aggregate: ProcessRowAggregate,
    flat_index: usize,
    local_time_rules: &'a LocalTimeRulesObservation,
}

fn group_header_from_shared(
    root: &ProcessItem,
    input: GroupHeaderInput<'_>,
) -> Option<ProjectedRow> {
    let GroupHeaderInput {
        row_key,
        expansion_key,
        name,
        expanded,
        member_count,
        aggregate,
        flat_index,
        local_time_rules,
    } = input;
    Some(ProjectedRow::GroupHeader {
        flat_index,
        main_pid: root.pid,
        row_key,
        name,
        expansion_key,
        member_count,
        expanded,
        metrics: Box::new(aggregate),
        user: root.current_user().unwrap_or_else(missing_value),
        status: root.status.clone(),
        nice: root.current_nice(),
        start_time_secs: root.current_start_time_secs(),
        start_clock: taskmanager_shell::presentation::start_clock_local(
            root.current_start_time_secs(),
            local_time_rules,
        ),
    })
}

/// Build the renderer's cell strings from typed observations and injected
/// local-time rules. An unavailable or stale scalar renders an honest dash
/// instead of the legacy field's zero sentinel.
pub(crate) fn build_row_cells_with_rules(
    process: &ProcessItem,
    local_time_rules: &LocalTimeRulesObservation,
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
        user: process.current_user().unwrap_or_else(missing_value),
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
        expanded_tree: &HashSet<ProcessLiveKey>,
        local_time_rules: &LocalTimeRulesObservation,
        observed_at_ms: u64,
    ) -> Self {
        let process_facts = flat
            .iter()
            .map(|process| ProcessRowFacts::from_process(process))
            .collect();
        let shared_rows =
            project_process_tree_rows(flat, expanded_groups, expanded_tree, sort, observed_at_ms);
        let mut rows: Vec<ProjectedRow> = shared_rows
            .into_iter()
            .filter_map(|row| projected_row_from_shared(row, flat, local_time_rules))
            .collect();
        for row in &mut rows {
            match row {
                ProjectedRow::Tree { .. } => {}
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
    expanded_tree: HashSet<ProcessLiveKey>,
    local_time_rules: Option<taskmanager_core::core::time::LocalTimeRulesCacheKey>,
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
        expanded_tree: &HashSet<ProcessLiveKey>,
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
    pub(crate) fn with_local_time_rules(mut self, rules: &LocalTimeRulesObservation) -> Self {
        self.local_time_rules = Some(rules.cache_key());
        self
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/process_projection/tests.rs"]
mod tests;

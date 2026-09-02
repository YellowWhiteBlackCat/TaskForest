//! Canonical category-tree row projection and cache shared by rendering and
//! keyboard paging.

use std::collections::HashSet;
use std::rc::Rc;

use crate::gpui_app::graph::GraphCacheHandle;
use crate::gpui_app::root::RootView;
use gpui::Entity;
use taskmanager_application::i18n;
use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_core::core::process::{
    ProcessApplicationIdentity, ProcessCategory, ProcessItem, ProcessLiveKey,
};
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_shell::SortCol;
use taskmanager_shell::matches_process_query;
use taskmanager_shell::presentation::missing_value;
use taskmanager_shell::{
    ProcessRowAggregate, ProcessRowId, ProcessTreeRow, SortDir, project_process_tree_rows,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::{RowDensity, UiSize};

/// Stable expansion-set key for one category bucket (the shared neutral
/// implementation): the `category:`-prefixed [`ProcessCategory::stable_key`],
/// which can never collide with a normalized app-group name.
use taskmanager_application::process_category_projection::{
    category_expansion_key, process_memory_observation_for_display,
};

/// Seed the canonical category tree expanded on first use. The set is also
/// used by capture/bootstrap paths, so every surface starts with the same
/// visible hierarchy while retaining per-category collapse controls.
pub fn default_category_expansions() -> HashSet<String> {
    ProcessCategory::ALL
        .iter()
        .copied()
        .map(category_expansion_key)
        .collect()
}

pub fn sort_id(col: SortCol) -> &'static str {
    match col {
        SortCol::Name => "sort-name",
        SortCol::User => "sort-user",
        SortCol::Pid => "sort-pid",
        SortCol::Threads => "sort-threads",
        SortCol::StartTime => "sort-start",
        SortCol::State => "sort-status",
        SortCol::Cpu => "sort-cpu",
        SortCol::Memory => "sort-mem",
        // Not a renderable GPUI column (see `rows`'s module docs); the id
        // keeps the arm exhaustive. It can never be mounted on a header cell.
        SortCol::Pss => "sort-pss",
        SortCol::Swap => "sort-swap",
        SortCol::DiskRead => "sort-disk-r",
        SortCol::DiskWrite => "sort-disk-w",
        SortCol::CpuTime => "sort-cputime",
        SortCol::Fds => "sort-fds",
        SortCol::Nice => "sort-nice",
    }
}

/// Apps memory policy: use current hybrid PSS when the platform provider has
/// it; if that enrichment is unavailable, retain the already-typed RSS value
/// as an explicit resident fallback. This keeps the row useful while the
/// measurement kind remains isolated in the core observations.
pub fn memory_for_display(process: &ProcessItem) -> Option<u64> {
    process_memory_observation_for_display(process)
        .current_value()
        .copied()
}

/// Canonical column order minus the hidden set — the columns currently
/// reachable in the header row. `Name` is the identity column (never toggleable
/// in the "Choose columns" picker — see `super::is_hideable`), so it
/// survives any hidden set. Header arrow-key navigation consumes this exact
/// projection so keyboard movement can never land on a column that is not
/// rendered (and body rows stay pixel-aligned with the header by construction).
pub fn visible_sort_cols(hidden_cols: &HashSet<SortCol>) -> Vec<SortCol> {
    super::columns()
        .iter()
        .copied()
        .filter(|col| *col == SortCol::Name || !hidden_cols.contains(col))
        .collect()
}

/// Apply host-fact-driven column visibility without mutating the user's saved
/// preference. `Some(0)` is a confirmed no-swap state; `None` means the host
/// did not provide a current swap-total fact and therefore must not trigger an
/// automatic hide.
pub fn effective_process_hidden_cols(
    hidden_cols: &HashSet<SortCol>,
    swap_total_bytes: Option<u64>,
) -> HashSet<SortCol> {
    let mut effective = hidden_cols.clone();
    if swap_total_bytes == Some(0) {
        effective.insert(SortCol::Swap);
    }
    effective
}

/// Keep the cached row order and the rendered header on the same visible sort
/// column when a host-fact policy hides the persisted active column.
#[must_use]
pub fn effective_process_sort_col(sort_col: SortCol, hidden_cols: &HashSet<SortCol>) -> SortCol {
    if hidden_cols.contains(&sort_col) {
        visible_sort_cols(hidden_cols)
            .into_iter()
            .next()
            .unwrap_or(SortCol::Name)
    } else {
        sort_col
    }
}

/// The adjacent rendered column for header arrow-key navigation (ArrowRight →
/// next visible column, ArrowLeft → previous visible column). Walks `visible`
/// in canonical order and WRAPS at the ends (a single-row cycle: from the last
/// column Right returns to the first, and vice versa), so a user on the Name
/// edge is never stuck. Returns `cur` unchanged when it is not in `visible`
/// (e.g. the user hid the active sort column via the picker) or `visible`
/// holds a single column. The sort DIRECTION is deliberately not part of this
/// function: moving the sort column across the header PRESERVES `sort_asc`
/// (unlike a click, whose reducer flips the direction).
pub fn sort_col_step(cur: SortCol, right: bool, visible: &[SortCol]) -> SortCol {
    let Some(pos) = visible.iter().position(|col| *col == cur) else {
        return cur;
    };
    if visible.len() == 1 {
        return cur;
    }
    let next = if right {
        (pos + 1) % visible.len()
    } else {
        (pos + visible.len() - 1) % visible.len()
    };
    visible[next]
}

/// What a row's expand/collapse chevron toggles when clicked.
#[derive(Clone)]
pub enum Toggle {
    None,
    Tree(ProcessLiveKey),
    GroupApp(ProcessLiveKey),
    /// An application aggregate whose root currently lacks a live identity.
    /// The row remains visible and expandable, but it cannot become an
    /// actionable process target.
    GroupAppUnknown(String),
    /// The typed core category whose member rows are toggled, derived into the prefixed
    /// [`category_expansion_key`] entries of `RootView::processes.expanded_apps`
    /// (the shared hierarchy expansion set).
    GroupCategory(ProcessCategory),
}

/// Pre-formatted display text for one row's data cells (the columns whose
/// rendering is a pure function of the row's typed values). Filled once per
/// projection rebuild by [`visible_rows`] — the per-frame row closure only
/// consumes these strings, never reformats. gpui 0.2.2 rebuilds the visible
/// rows' elements on EVERY frame (including column-band navigation frames),
/// so the ~10 `format!` calls per row must not run per frame. The
/// formatters stay in `super::formatting` as the single source; this struct
/// only memoizes their output.
#[derive(Default)]
pub struct RowCellText {
    pub user: String,
    pub pid: String,
    pub status_label: String,
    pub cpu: String,
    pub memory: String,
    pub swap: String,
    pub disk_read: String,
    pub disk_write: String,
    pub cpu_time: String,
    pub threads: String,
    pub fds: String,
    pub nice: String,
    pub start_time: String,
    precomputed: bool,
}

impl RowCellText {
    /// Format every data cell of one visible row, mirroring the exact
    /// formatter/dash conventions the inline render path used (`"—"` for
    /// `None`, the typed formatter for `Some`).
    pub(super) fn build(row: &VisibleRow, units: UnitPreferences) -> Self {
        use super::formatting::{
            format_bytes_rate, format_cpu_percent, format_cpu_time, format_memory, format_nice,
            format_start_time, optional_f32_dash, optional_i32_dash, optional_u32_dash,
            optional_u64_dash,
        };
        Self {
            user: row.user.clone(),
            pid: row
                .process_identity
                .map_or_else(String::new, |identity| identity.pid().to_string()),
            status_label: super::cells::status_label(&row.status),
            cpu: optional_f32_dash(row.cpu, format_cpu_percent),
            memory: row.mem.map_or_else(
                || "\u{2014}".to_string(),
                |bytes| format_memory(units, bytes),
            ),
            swap: row.swap.map_or_else(
                || "\u{2014}".to_string(),
                |bytes| format_memory(units, bytes),
            ),
            disk_read: row.disk_read.map_or_else(
                || "\u{2014}".to_string(),
                |bytes| format_bytes_rate(units, bytes),
            ),
            disk_write: row.disk_write.map_or_else(
                || "\u{2014}".to_string(),
                |bytes| format_bytes_rate(units, bytes),
            ),
            cpu_time: optional_u64_dash(row.cpu_time_secs, format_cpu_time),
            threads: optional_u32_dash(row.threads),
            fds: optional_u32_dash(row.fds),
            nice: optional_i32_dash(row.nice, format_nice),
            start_time: format_start_time(
                row.start_time_secs,
                &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
            ),
            precomputed: true,
        }
    }

    fn apply_local_time(
        &mut self,
        start_time_secs: Option<u64>,
        rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
    ) {
        self.start_time = super::formatting::format_start_time(start_time_secs, rules);
    }
}

/// A single row the `uniform_list` renders. Carries the display data plus tree/group
/// affordance info (depth, has-children, collapse state, instance badge, toggle target).
pub struct VisibleRow {
    pub name: String,
    /// Semantic row identity used by selection/focus. Application aggregates
    /// carry their root key while remaining PID-less; category headers have no
    /// selectable key and real process rows carry `Process(live-key)`.
    pub selection_key: Option<ProcessRowId>,
    /// Exact live identity represented by an individual process row. Aggregate
    /// rows deliberately carry `None`: their numeric cells are totals, not a
    /// representative process, and must never participate in process
    /// selection or context actions.
    pub process_identity: Option<ProcessLiveKey>,
    /// Verified desktop-entry identity, when the provider found one. The row
    /// renders its validated asset and falls back to the generic glyph when
    /// icon-theme resolution is unavailable.
    pub application_identity: Option<ProcessApplicationIdentity>,
    pub user: String,
    pub status: String,
    /// Typed CPU percentage; `None` renders as "—" (unavailable/unknown).
    pub cpu: Option<f32>,
    /// Typed resident memory bytes; `None` renders as "—".
    pub mem: Option<u64>,
    /// Full typed aggregate CPU observation for an aggregate row. Leaf rows
    /// leave this absent; the scalar `cpu` field is its current-value view.
    pub cpu_aggregate: Option<AggregateMetric<f32>>,
    /// Full typed aggregate memory observation for an aggregate row. Leaf rows
    /// leave this absent; the scalar `mem` field is its current-value view.
    pub memory_aggregate: Option<AggregateMetric<u64>>,
    /// Typed per-process swap bytes; this is never included in `mem`.
    pub swap: Option<u64>,
    /// Typed disk read rate; `None` renders as "—".
    pub disk_read: Option<u64>,
    /// Typed disk write rate; `None` renders as "—".
    pub disk_write: Option<u64>,
    /// Thread count. On leaf/instance rows = `ProcessItem::current_threads()`;
    /// on aggregate rows (group/type) = the SUM across the group's available
    /// members. `None` renders as "—".
    pub threads: Option<u32>,
    /// Process start time as unix seconds. On leaf/instance rows =
    /// `ProcessItem::current_start_time_secs()`; on aggregate rows = the
    /// representative main_pid's. `None` renders as "—".
    pub start_time_secs: Option<u64>,
    /// Cumulative CPU time (user + system, seconds). On leaf/instance rows =
    /// `ProcessItem::current_cpu_time_secs()`; on aggregate rows (group/type) =
    /// the SUM across the group's available members. `None` renders as "—".
    pub cpu_time_secs: Option<u64>,
    /// Open file-descriptor count. On leaf/instance rows =
    /// `ProcessItem::current_fds()`; on aggregate rows = the SUM across the
    /// group's available members. `None` renders as "—".
    pub fds: Option<u32>,
    /// Scheduling nice value (-20..19). On leaf/instance rows =
    /// `ProcessItem::current_nice()`; on aggregate rows = the representative
    /// main_pid's. `None` renders as "—".
    pub nice: Option<i32>,
    /// Recent `cpu_usage` samples (newest-last) for the per-row sparkline. EMPTY
    /// only on aggregate rows (no single history); every real process row,
    /// including a tree parent, keeps its own trend. Shared `Rc`: the
    /// sparkline's paint closure clones the `Rc`, not the samples, on every
    /// repaint.
    pub cpu_history: Rc<[f32]>,
    /// Precomputed search-match ranges into `name`, filled by
    /// [`visible_rows`] when a query is active so the per-row render replays
    /// the cached ranges instead of re-running the shared match engine per
    /// row per frame (ADR-020: same single `match_ranges_ascii_ci` engine).
    pub name_highlights: Vec<std::ops::Range<usize>>,
    /// Pre-formatted data-cell texts (see [`RowCellText`]); filled by
    /// [`visible_rows`] after construction.
    pub cell_text: RowCellText,
    pub depth: usize,
    pub has_children: bool,
    pub collapsed: bool,
    /// Nearest visible selectable ancestor row (iced-parity tree keyboard
    /// navigation). An in-tree parent carries its [`ProcessRowId::Process`];
    /// a root process row under an application aggregate carries the
    /// aggregate's [`ProcessRowId::Application`]. Rows whose parent is a
    /// structural category header — or that have no visible parent — carry
    /// `None`, so a bare Left on them is a no-op instead of falling through
    /// to column stepping. Computed once per projection rebuild (a Copy
    /// field on the row the recursion already owns); the renderer never
    /// re-derives it.
    pub parent_key: Option<ProcessRowId>,
    /// Small inline annotation (e.g. "×3" instance count on group rows).
    pub badge: Option<String>,
    pub toggle: Toggle,
}

impl VisibleRow {
    /// Whether this row is an aggregate group header rather than an individual
    /// process. Tree parents remain selectable because they carry their own
    /// real PID; only category/app/type aggregate rows have no identity.
    #[must_use]
    pub fn is_aggregate(&self) -> bool {
        !matches!(self.selection_key, Some(ProcessRowId::Process(_)))
    }
}

/// One bare structural arrow's meaning on a row that owns a subtree — the
/// tree-keyboard-navigation matrix shared verbatim with the iced frontend's
/// `toggle_at_visual_cursor` (same semantics, per-frontend execution). Pure
/// so tests pin the matrix without driving a real window, and so the row key
/// handler stays a thin executor with no second copy of the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StructuralArrow {
    /// Left on an expanded row: collapse its subtree; the row itself stays
    /// selected and visible.
    Collapse,
    /// Right on a collapsed row: expand its subtree; the row stays selected.
    Expand,
    /// Left on an already-collapsed row: move the selection up to the nearest
    /// visible selectable ancestor ([`VisibleRow::parent_key`]).
    GotoParent(ProcessRowId),
}

/// Resolve one bare structural arrow (the caller has already gated on
/// `has_children` and the absence of Alt/Shift column modifiers). Right on an
/// expanded row is an honest no-op, as is Left with no selectable ancestor —
/// neither falls through to column stepping.
#[must_use]
pub(crate) fn structural_arrow_action(
    collapsed: bool,
    parent_key: Option<ProcessRowId>,
    right: bool,
) -> Option<StructuralArrow> {
    match (right, collapsed) {
        (false, false) => Some(StructuralArrow::Collapse),
        (true, true) => Some(StructuralArrow::Expand),
        (false, true) => parent_key.map(StructuralArrow::GotoParent),
        (true, false) => None,
    }
}

/// Localized aggregate-row label for one category bucket.
fn category_label(category: ProcessCategory) -> &'static str {
    match category {
        ProcessCategory::Application => i18n::t("proc.category_apps"),
        ProcessCategory::Background => i18n::t("proc.category_background"),
        ProcessCategory::Uncategorized => i18n::t("proc.category_uncategorized"),
    }
}

/// Adapt the shell-owned structural projection into GPUI's renderer row.
/// Filtering and formatting remain GPUI concerns; category order, hierarchy,
/// expansion, identity and typed aggregate facts do not.
pub fn category_tree_rows(
    processes: &[&ProcessItem],
    observed_at_ms: u64,
    column: SortCol,
    ascending: bool,
    expanded: &HashSet<String>,
    collapsed: &HashSet<ProcessLiveKey>,
    units: UnitPreferences,
) -> Vec<VisibleRow> {
    let direction = if ascending {
        taskmanager_shell::SortDir::Asc
    } else {
        taskmanager_shell::SortDir::Desc
    };
    project_process_tree_rows(
        processes,
        expanded,
        collapsed,
        (column, direction),
        observed_at_ms,
    )
    .into_iter()
    .filter_map(|row| visible_row_from_shared(row, processes, units))
    .collect()
}

fn visible_row_from_shared(
    row: ProcessTreeRow,
    processes: &[&ProcessItem],
    units: UnitPreferences,
) -> Option<VisibleRow> {
    match row {
        ProcessTreeRow::Category {
            category,
            representative_index,
            expanded,
            aggregate,
            ..
        } => {
            let representative = processes.get(representative_index)?;
            Some(group_row_from_shared(
                representative,
                GroupRowInput {
                    selection_key: None,
                    name: category_label(category).to_owned(),
                    expanded,
                    aggregate,
                    toggle: Toggle::GroupCategory(category),
                    depth: 0,
                },
                units,
            ))
        }
        ProcessTreeRow::Application {
            visible_index,
            row_key,
            expansion_key,
            expanded,
            aggregate,
            ..
        } => {
            let representative = processes.get(visible_index)?;
            let toggle = row_key.and_then(ProcessRowId::live_key).map_or_else(
                || Toggle::GroupAppUnknown(expansion_key.clone()),
                Toggle::GroupApp,
            );
            Some(group_row_from_shared(
                representative,
                GroupRowInput {
                    selection_key: row_key,
                    name: representative
                        .current_application_name()
                        .unwrap_or(representative.name.as_str())
                        .to_owned(),
                    expanded,
                    aggregate,
                    toggle,
                    depth: 1,
                },
                units,
            ))
        }
        ProcessTreeRow::Process {
            visible_index,
            row_key,
            parent_key,
            depth,
            has_children,
            collapsed,
        } => {
            let process = processes.get(visible_index)?;
            let parent_key = match parent_key {
                Some(ProcessRowId::Category(_)) => None,
                other => other,
            };
            let row = VisibleRow {
                name: process.name.clone(),
                selection_key: row_key,
                process_identity: row_key.and_then(ProcessRowId::live_key),
                application_identity: process.current_application_identity().cloned(),
                user: process.current_user().unwrap_or_else(missing_value),
                status: process.status.clone(),
                cpu: process.current_cpu_percentage(),
                mem: memory_for_display(process),
                cpu_aggregate: None,
                memory_aggregate: None,
                swap: process.current_swap_bytes(),
                disk_read: process.current_disk_read_bytes_per_sec(),
                disk_write: process.current_disk_write_bytes_per_sec(),
                threads: process.current_threads(),
                start_time_secs: process.current_start_time_secs(),
                cpu_time_secs: process.current_cpu_time_secs(),
                fds: process.current_fds(),
                nice: process.current_nice(),
                cpu_history: Rc::from(process.cpu_history.as_slice()),
                name_highlights: Vec::new(),
                cell_text: RowCellText::default(),
                depth,
                has_children,
                collapsed,
                parent_key,
                badge: None,
                toggle: if has_children {
                    row_key
                        .and_then(ProcessRowId::live_key)
                        .map_or(Toggle::None, Toggle::Tree)
                } else {
                    Toggle::None
                },
            };
            let mut row = row;
            row.cell_text = RowCellText::build(&row, units);
            Some(row)
        }
    }
}

struct GroupRowInput {
    selection_key: Option<ProcessRowId>,
    name: String,
    expanded: bool,
    aggregate: ProcessRowAggregate,
    toggle: Toggle,
    depth: usize,
}

fn group_row_from_shared(
    representative: &ProcessItem,
    input: GroupRowInput,
    units: UnitPreferences,
) -> VisibleRow {
    let GroupRowInput {
        selection_key,
        name,
        expanded,
        aggregate,
        toggle,
        depth,
    } = input;
    let mut row = VisibleRow {
        name,
        selection_key,
        process_identity: None,
        application_identity: representative.current_application_identity().cloned(),
        user: representative.current_user().unwrap_or_else(missing_value),
        status: representative.status.clone(),
        cpu: aggregate.cpu().current_value().copied(),
        mem: aggregate.memory().current_value().copied(),
        cpu_aggregate: Some(aggregate.cpu().clone()),
        memory_aggregate: Some(aggregate.memory().clone()),
        swap: aggregate.swap().current_value().copied(),
        disk_read: aggregate.disk_read().current_value().copied(),
        disk_write: aggregate.disk_write().current_value().copied(),
        threads: aggregate
            .threads()
            .current_value()
            .and_then(|value| u32::try_from(*value).ok()),
        start_time_secs: representative.current_start_time_secs(),
        cpu_time_secs: aggregate.cpu_time().current_value().copied(),
        fds: aggregate
            .fds()
            .current_value()
            .and_then(|value| u32::try_from(*value).ok()),
        nice: representative.current_nice(),
        cpu_history: Rc::from([]),
        name_highlights: Vec::new(),
        cell_text: RowCellText::default(),
        depth,
        has_children: true,
        collapsed: !expanded,
        parent_key: None,
        badge: None,
        toggle,
    };
    row.cell_text = RowCellText::build(&row, units);
    row
}

/// A body cell for a numeric column: RIGHT-aligned and rendered in the theme's
/// monospace stack ([`Theme::mono_font`]) so digits (and decimal points) line up
/// vertically across rows and across ticks — the Win11 Task Manager / Mission
/// Center numeric-column look. The caller pins the cell width with `.w(..)`
/// (or `.flex_1()` for a growable variant), matching the header cell produced by
/// `sort_cell` for the same [`SortCol`] so header and body stay pixel-aligned.
/// One process-table row's render inputs (design-debt #1 props
/// consolidation).
pub struct ProcRowProps<'a> {
    pub theme: &'a Theme,
    pub row: &'a VisibleRow,
    pub row_idx: usize,
    pub is_sel: bool,
    pub is_hov: bool,
    pub entity: &'a Entity<RootView>,
    pub process_identities: Rc<Vec<ProcessLiveKey>>,
    /// Ordered selectable semantic rows. Includes PID-less application roots
    /// and excludes structural category headers.
    pub row_keys: Rc<Vec<ProcessRowId>>,
    /// The full visible-row projection this row belongs to, shared with the
    /// key handlers. Bare Left/Right resolve the LIVE selected row through it
    /// (focus and selection diverge after Home/End/PageUp) so the structural
    /// action never consults a stale focused row; the Rc clone per rendered
    /// row is one non-atomic refcount bump, the same cost as `pids`/`row_keys`.
    pub rows: Rc<Vec<VisibleRow>>,
    /// Apps-page preference: dim current zero-valued resource cells while
    /// leaving missing values rendered as the unavailable-value dash.
    pub gray_zero_values: bool,
    /// Table row density: the row's vertical padding + line-height come from
    /// the density axis, mirroring the header so both stay pixel-aligned.
    pub density: RowDensity,
    /// Product-wide readable type/icon metrics, independent from row density.
    pub ui_size: UiSize,
    pub(crate) graph_cache: GraphCacheHandle,
}

/// All inputs to the visible-row projection (design-debt #1 props
/// consolidation). The projection is shared by rendering and keyboard paging
/// so both consume one ordering.
pub struct VisibleRowsProps<'a> {
    pub processes: &'a [&'a ProcessItem],
    /// Timestamp of the accepted process snapshot that supplied `processes`.
    /// Aggregate observations retain this timestamp instead of inferring it
    /// from member fields.
    pub observed_at_ms: u64,
    pub query: &'a str,
    pub sort_col: SortCol,
    pub sort_asc: bool,
    pub filter: ProcessStatusFilter,
    pub collapsed: &'a HashSet<ProcessLiveKey>,
    pub expanded_apps: &'a HashSet<String>,
    /// Presentation unit preferences for the precomputed data-cell texts.
    pub units: UnitPreferences,
}

pub fn visible_rows(props: VisibleRowsProps<'_>) -> Vec<VisibleRow> {
    visible_rows_with_local_time(props, &LocalTimeRulesObservation::unsupported(0))
}

/// Build visible rows against composition-injected local-time rules. The
/// compatibility-shaped [`visible_rows`] keeps pure projection fixtures host-
/// independent by supplying an explicit unsupported observation.
pub fn visible_rows_with_local_time(
    props: VisibleRowsProps<'_>,
    local_time_rules: &LocalTimeRulesObservation,
) -> Vec<VisibleRow> {
    let VisibleRowsProps {
        processes,
        observed_at_ms,
        query,
        sort_col,
        sort_asc,
        filter,
        collapsed,
        expanded_apps,
        units,
    } = props;
    // ONE filter pass through the shell grammar (the same
    // `matches_process_query` the iced/TUI frontends and the shell track's
    // `visible_processes` consume): structured `pid:`/`user:`/`status:`/
    // `cmd:`/`name:` selectors plus the name-or-pid-or-user-or-cmdline
    // fallback, intersected with the status bucket. Borrow-based like the
    // historic pass — no per-item allocation.
    let filtered: Vec<&ProcessItem> = processes
        .iter()
        .copied()
        .filter(|process| filter.matches(&process.status) && matches_process_query(process, query))
        .collect();
    project_visible_rows_from_shell(ShellVisibleRowsProps {
        processes: &filtered,
        observed_at_ms,
        query,
        sort_col,
        sort_asc,
        collapsed,
        expanded_apps,
        units,
        local_time_rules,
    })
}

/// Adapt the already filtered/sorted shell process list into GPUI rows.
/// Production GPUI calls this after [`RootView`]'s direct-track shell has
/// applied the query and status filter. The compatibility-shaped
/// [`visible_rows_with_local_time`] remains only for isolated projection tests.
pub struct ShellVisibleRowsProps<'a> {
    pub processes: &'a [&'a ProcessItem],
    pub observed_at_ms: u64,
    pub query: &'a str,
    pub sort_col: SortCol,
    pub sort_asc: bool,
    pub collapsed: &'a HashSet<ProcessLiveKey>,
    pub expanded_apps: &'a HashSet<String>,
    pub units: UnitPreferences,
    pub local_time_rules: &'a LocalTimeRulesObservation,
}

pub fn project_visible_rows_from_shell(props: ShellVisibleRowsProps<'_>) -> Vec<VisibleRow> {
    let ShellVisibleRowsProps {
        processes,
        observed_at_ms,
        query,
        sort_col,
        sort_asc,
        collapsed,
        expanded_apps,
        units,
        local_time_rules,
    } = props;
    let mut rows = category_tree_rows(
        processes,
        observed_at_ms,
        sort_col,
        sort_asc,
        expanded_apps,
        collapsed,
        units,
    );
    // Precompute the per-row derived text ONCE per projection rebuild: the
    // search-highlight ranges and the data-cell formats. A repaint (hover or
    // column-band navigation) replays them instead of re-running the match
    // engine and ~10 formatters per visible row per frame.
    let trimmed = query.trim();
    for row in &mut rows {
        if !trimmed.is_empty() {
            row.name_highlights =
                taskmanager_core::core::text::match_ranges_ascii_ci(&row.name, trimmed);
        }
        if !row.cell_text.precomputed {
            row.cell_text = RowCellText::build(row, units);
        }
        row.cell_text
            .apply_local_time(row.start_time_secs, local_time_rules);
    }
    rows
}

/// Count the honest application-tree roots in a process snapshot. This is the
/// Applications page summary authority: it is independent from expansion,
/// sorting and filtering, so collapsing the Applications category cannot make
/// the page title claim that zero apps are running.
#[must_use]
pub fn application_root_count(processes: &[&ProcessItem]) -> usize {
    let expanded = HashSet::from([category_expansion_key(ProcessCategory::Application)]);
    project_process_tree_rows(
        processes,
        &expanded,
        &HashSet::new(),
        (SortCol::Pid, SortDir::Asc),
        0,
    )
    .iter()
    .filter(|row| matches!(row, ProcessTreeRow::Application { .. }))
    .count()
}

/// Cached visible-row projection for the processes table. Rebuilt only when
/// the process data (`processes_generation`), the search query, the sort
/// state, or an expansion set changes; hover-driven re-renders and keyboard
/// paging reuse the same `Rc` payloads, so a 10k-row model is built once per
/// tick instead of once per frame and once per keypress.
pub struct ProjectionCache {
    pub processes_generation: u64,
    pub query: String,
    pub sort_col: SortCol,
    pub sort_asc: bool,
    pub filter: ProcessStatusFilter,
    pub collapsed: HashSet<ProcessLiveKey>,
    pub expanded_apps: HashSet<String>,
    pub local_time_rules: taskmanager_core::core::time::LocalTimeRulesCacheKey,
    pub rows: Rc<Vec<VisibleRow>>,
    pub process_identities: Rc<Vec<ProcessLiveKey>>,
    pub application_count: usize,
    /// Unit preferences the cached cell texts were formatted with.
    pub units: UnitPreferences,
}

/// Adapter parity: GPUI's renderer row is derived from the shell-owned
/// `ProcessTreeRow` structure. The shared fixture pins bucket order, expansion
/// keys, typed aggregates, member counts, and expanded member order; GPUI owns
/// only labels, cells, and toolkit row shape.
#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_processes_view_rows_projection_category_projection_parity.rs"]
mod category_projection_parity;

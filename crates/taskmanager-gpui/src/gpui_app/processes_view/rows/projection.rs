//! Canonical category-tree row projection and cache shared by rendering and
//! keyboard paging.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::core::process::{
    AppGroup, ProcessApplicationIdentity, ProcessCategory, ProcessItem, ProcessNode,
    application_group_name, build_process_tree, process_category,
};
use crate::gpui_app::processes_view::rows::SortCol;
use crate::gpui_app::processes_view::rows::groups::aggregate_row;
use crate::gpui_app::processes_view::sort_key::sort_axis;
use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::Theme;
use crate::gpui_app::theme::tokens::{RowDensity, UiSize};
use crate::i18n;
use gpui::Entity;
use taskmanager_application::process_category_projection::category_buckets;
use taskmanager_application::process_sort::compare_processes;
use taskmanager_shell::ProcessRowKey;
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_shell::matches_process_query;

/// Stable expansion-set key for one category bucket (the shared neutral
/// implementation): the `category:`-prefixed [`ProcessCategory::stable_key`],
/// which can never collide with a normalized app-group name. Re-exported so
/// the `rows` facade and the toggle paths keep one import site.
pub use taskmanager_application::process_category_projection::category_expansion_key;

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
    process
        .current_memory_pss_bytes()
        .or_else(|| process.current_memory_bytes())
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
    TreePid(u32),
    GroupApp(String),
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
    pub(super) fn build(row: &VisibleRow) -> Self {
        use super::formatting::{
            format_bytes_rate, format_cpu_percent, format_cpu_time, format_memory, format_nice,
            format_start_time, optional_f32_dash, optional_i32_dash, optional_u32_dash,
            optional_u64_dash,
        };
        Self {
            user: row.user.clone(),
            pid: row
                .process_pid
                .map_or_else(String::new, |pid| pid.to_string()),
            status_label: super::cells::status_label(&row.status),
            cpu: optional_f32_dash(row.cpu, format_cpu_percent),
            memory: row
                .mem
                .map_or_else(|| "\u{2014}".to_string(), format_memory),
            swap: row
                .swap
                .map_or_else(|| "\u{2014}".to_string(), format_memory),
            disk_read: row
                .disk_read
                .map_or_else(|| "\u{2014}".to_string(), format_bytes_rate),
            disk_write: row
                .disk_write
                .map_or_else(|| "\u{2014}".to_string(), format_bytes_rate),
            cpu_time: optional_u64_dash(row.cpu_time_secs, format_cpu_time),
            threads: optional_u32_dash(row.threads),
            fds: optional_u32_dash(row.fds),
            nice: optional_i32_dash(row.nice, format_nice),
            start_time: format_start_time(
                row.start_time_secs,
                &taskmanager_application::LocalTimeRulesObservation::unsupported(0),
            ),
            precomputed: true,
        }
    }

    fn apply_local_time(
        &mut self,
        start_time_secs: Option<u64>,
        rules: &taskmanager_application::LocalTimeRulesObservation,
    ) {
        self.start_time = super::formatting::format_start_time(start_time_secs, rules);
    }

    /// Build a group/category header's metric text from the rounded member
    /// readouts. Raw numeric fields remain available on [`VisibleRow`] for
    /// sorting and diagnostics; this presentation layer makes the displayed
    /// root exactly reconcilable with the displayed children.
    pub(super) fn build_additive(row: &VisibleRow, members: &[&ProcessItem]) -> Self {
        let mut text = Self::build(row);
        text.cpu = super::formatting::format_additive_cpu(
            members
                .iter()
                .map(|process| process.current_cpu_percentage()),
        );
        text.memory = super::formatting::format_additive_memory(
            members.iter().map(|process| memory_for_display(process)),
        );
        text.swap = super::formatting::format_additive_memory(
            members.iter().map(|process| process.current_swap_bytes()),
        );
        text.disk_read = super::formatting::format_additive_rate(
            members
                .iter()
                .map(|process| process.current_disk_read_bytes_per_sec()),
        );
        text.disk_write = super::formatting::format_additive_rate(
            members
                .iter()
                .map(|process| process.current_disk_write_bytes_per_sec()),
        );
        text
    }
}

/// A single row the `uniform_list` renders. Carries the display data plus tree/group
/// affordance info (depth, has-children, collapse state, instance badge, toggle target).
pub struct VisibleRow {
    pub name: String,
    /// Semantic row identity used by selection/focus. Application aggregates
    /// carry their root key while remaining PID-less; category headers have no
    /// selectable key and real process rows carry `Process(pid)`.
    pub selection_key: Option<ProcessRowKey>,
    /// The process identity represented by this row. Aggregate rows are
    /// deliberately `None`: their numeric cells are totals, not a
    /// representative process, and they must never participate in process
    /// selection, context actions, or PID navigation.
    pub process_pid: Option<u32>,
    /// Internal ordering key retained for compatibility with the existing
    /// row projection API. It is zero for aggregate rows and must not be
    /// rendered or used as a process identity; use [`VisibleRow::process_pid`]
    /// instead.
    pub pid: u32,
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
        self.process_pid.is_none()
    }
}

/// Recursive tree sort by a caller-supplied DIRECTED item comparator — the
/// view-layer counterpart of `sort_nodes`. The comparator already carries the
/// direction (the neutral `compare_processes` applies it plus the
/// direction-independent pid tie-break); this helper only recurses the same
/// ordering into every child level.
fn sort_nodes_by<'a, F>(nodes: &mut [ProcessNode<'a>], cmp: &F)
where
    F: Fn(&ProcessItem, &ProcessItem) -> std::cmp::Ordering,
{
    nodes.sort_by(|a, b| cmp(a.item, b.item));
    for n in nodes.iter_mut() {
        sort_nodes_by(&mut n.children, cmp);
    }
}

fn tree_row_from_node(
    node: &ProcessNode<'_>,
    depth_offset: usize,
    collapsed: &HashSet<u32>,
) -> VisibleRow {
    let has_children = !node.children.is_empty();
    let mut row = VisibleRow {
        name: node.item.name.clone(),
        selection_key: Some(ProcessRowKey::Process(node.item.pid)),
        process_pid: Some(node.item.pid),
        pid: node.item.pid,
        application_identity: node.item.current_application_identity().cloned(),
        user: node.item.current_user().unwrap_or_default(),
        status: node.item.status.clone(),
        cpu: node.item.current_cpu_percentage(),
        mem: memory_for_display(node.item),
        swap: node.item.current_swap_bytes(),
        disk_read: node.item.current_disk_read_bytes_per_sec(),
        disk_write: node.item.current_disk_write_bytes_per_sec(),
        threads: node.item.current_threads(),
        start_time_secs: node.item.current_start_time_secs(),
        cpu_time_secs: node.item.current_cpu_time_secs(),
        fds: node.item.current_fds(),
        nice: node.item.current_nice(),
        cpu_history: Rc::from(node.item.cpu_history.as_slice()),
        name_highlights: Vec::new(),
        cell_text: RowCellText::default(),
        depth: node.depth.saturating_add(depth_offset),
        has_children,
        collapsed: collapsed.contains(&node.item.pid),
        badge: None,
        toggle: if has_children {
            Toggle::TreePid(node.item.pid)
        } else {
            Toggle::None
        },
    };
    row.cell_text = RowCellText::build(&row);
    row
}

fn push_tree_rows<'a>(
    node: &ProcessNode<'a>,
    depth_offset: usize,
    collapsed: &HashSet<u32>,
    rows: &mut Vec<VisibleRow>,
) {
    rows.push(tree_row_from_node(node, depth_offset, collapsed));
    if !node.children.is_empty() && !collapsed.contains(&node.item.pid) {
        for child in &node.children {
            push_tree_rows(child, depth_offset, collapsed, rows);
        }
    }
}

/// Flatten one process-tree root into the members owned by its application
/// aggregate. The root itself is included: unlike the old tree-parent
/// projection, the aggregate row owns the sum while every process row below it
/// keeps its own sample and PID.
fn collect_tree_members<'a>(node: &ProcessNode<'a>, members: &mut Vec<&'a ProcessItem>) {
    members.push(node.item);
    for child in &node.children {
        collect_tree_members(child, members);
    }
}

fn app_group_from_tree_root<'a>(root: &ProcessNode<'a>) -> AppGroup {
    let mut members = Vec::new();
    collect_tree_members(root, &mut members);
    let pids = members.iter().map(|process| process.pid).collect();
    AppGroup {
        name: root
            .item
            .current_application_name()
            .map(str::to_owned)
            .unwrap_or_else(|| application_group_name(root.item)),
        main_pid: root.item.pid,
        application_identity: root.item.current_application_identity().cloned(),
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
}

/// Localized aggregate-row label for one category bucket.
fn category_label(category: ProcessCategory) -> &'static str {
    match category {
        ProcessCategory::Application => i18n::t("proc.category_apps"),
        ProcessCategory::Background => i18n::t("proc.category_background"),
        ProcessCategory::Uncategorized => i18n::t("proc.category_uncategorized"),
    }
}

/// Canonical process projection: split the filtered processes into the honest core
/// [`ProcessCategory`] buckets — via the neutral
/// [`category_buckets`](taskmanager_application::process_category_projection)
/// projection, the shared skeleton of every frontend's category mode (fixed
/// `ALL` order, empty buckets omitted, members in input order) — and emit one
/// aggregate header row per NON-EMPTY bucket; an empty bucket never renders a
/// fabricated header. Each header has no PID and totals its members. The
/// Applications bucket expands to application-root aggregate rows (also with
/// no PID), followed by the real process tree whose every row keeps its own
/// PID and own observations. Background and Uncategorized retain their direct
/// process tree because they do not have an application-root projection.
pub fn category_tree_rows(
    processes: &[&ProcessItem],
    column: SortCol,
    ascending: bool,
    expanded: &HashSet<String>,
    collapsed: &HashSet<u32>,
) -> Vec<VisibleRow> {
    let by_pid: HashMap<u32, &ProcessItem> = processes
        .iter()
        .map(|process| (process.pid, *process))
        .collect();
    let mut rows = Vec::new();
    for bucket in category_buckets(processes, |process| process_category(process)) {
        let category = bucket.category();
        let bucket_members: Vec<&ProcessItem> =
            bucket.members().iter().map(|member| **member).collect();
        let key = category_expansion_key(category);
        let is_expanded = expanded.contains(&key);
        let mut pids: Vec<u32> = bucket_members.iter().map(|process| process.pid).collect();
        pids.sort_unstable();
        let group = AppGroup {
            name: key,
            main_pid: pids.first().copied().unwrap_or(0),
            application_identity: None,
            total_cpu_usage: bucket.sum_f32(|process| process.current_cpu_percentage()),
            total_memory_bytes: bucket.sum_u64(|process| process.current_memory_bytes()),
            process_count: bucket.member_count(),
            pids,
        };
        let mut aggregate = aggregate_row(&group, &by_pid, Toggle::GroupCategory(category));
        aggregate.name = category_label(category).to_owned();
        aggregate.badge = None;
        aggregate.collapsed = !is_expanded;
        rows.push(aggregate);
        if is_expanded {
            // Each category owns the same core parent/child tree as the
            // standalone Tree view. Sorting is recursive and uses the neutral
            // comparator, so the visible hierarchy cannot diverge by frontend.
            let mut tree = build_process_tree(&bucket_members);
            let axis = sort_axis(column);
            sort_nodes_by(&mut tree, &|a, b| compare_processes(a, b, axis, ascending));
            if category == ProcessCategory::Application {
                // A process-tree root is the application aggregate boundary.
                // Grouping by each process' desktop identity would split
                // launchers/sandboxes/helpers such as bwrap and glycin-svg;
                // grouping by the root keeps the complete app tree together,
                // exactly like the reference process monitors.
                let mut app_groups: Vec<AppGroup> =
                    tree.iter().map(app_group_from_tree_root).collect();
                super::groups::sort_groups(&mut app_groups, &bucket_members, column, ascending);
                for group in app_groups {
                    let Some(root) = tree.iter().find(|node| node.item.pid == group.main_pid)
                    else {
                        continue;
                    };
                    let expansion_key = format!("app-tree:{}", group.main_pid);
                    let mut app_row =
                        aggregate_row(&group, &by_pid, Toggle::GroupApp(expansion_key.clone()));
                    app_row.selection_key = Some(ProcessRowKey::Application(group.main_pid));
                    app_row.depth = 1;
                    app_row.badge = None;
                    app_row.collapsed = !expanded.contains(&expansion_key);
                    rows.push(app_row);
                    if expanded.contains(&expansion_key) {
                        push_tree_rows(root, 2, collapsed, &mut rows);
                    }
                }
            } else {
                for node in &tree {
                    push_tree_rows(node, 1, collapsed, &mut rows);
                }
            }
        }
    }
    rows
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
    pub pids: Rc<Vec<u32>>,
    /// Ordered selectable semantic rows. Includes PID-less application roots
    /// and excludes structural category headers.
    pub row_keys: Rc<Vec<ProcessRowKey>>,
    /// Apps-page preference: dim current zero-valued resource cells while
    /// leaving missing values rendered as the unavailable-value dash.
    pub gray_zero_values: bool,
    /// Table row density: the row's vertical padding + line-height come from
    /// the density axis, mirroring the header so both stay pixel-aligned.
    pub density: RowDensity,
    /// Product-wide readable type/icon metrics, independent from row density.
    pub ui_size: UiSize,
}

/// All inputs to the visible-row projection (design-debt #1 props
/// consolidation). The projection is shared by rendering and keyboard paging
/// so both consume one ordering.
pub struct VisibleRowsProps<'a> {
    pub processes: &'a [&'a ProcessItem],
    pub query: &'a str,
    pub sort_col: SortCol,
    pub sort_asc: bool,
    pub filter: ProcessStatusFilter,
    pub collapsed: &'a HashSet<u32>,
    pub expanded_apps: &'a HashSet<String>,
}

pub fn visible_rows(props: VisibleRowsProps<'_>) -> Vec<VisibleRow> {
    visible_rows_with_local_time(
        props,
        &taskmanager_application::LocalTimeRulesObservation::unsupported(0),
    )
}

/// Build visible rows against composition-injected local-time rules. The
/// compatibility-shaped [`visible_rows`] keeps pure projection fixtures host-
/// independent by supplying an explicit unsupported observation.
pub fn visible_rows_with_local_time(
    props: VisibleRowsProps<'_>,
    local_time_rules: &taskmanager_application::LocalTimeRulesObservation,
) -> Vec<VisibleRow> {
    let VisibleRowsProps {
        processes,
        query,
        sort_col,
        sort_asc,
        filter,
        collapsed,
        expanded_apps,
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
    let mut rows = category_tree_rows(&filtered, sort_col, sort_asc, expanded_apps, collapsed);
    // Precompute the per-row derived text ONCE per projection rebuild: the
    // search-highlight ranges and the data-cell formats. A repaint (hover or
    // column-band navigation) replays them instead of re-running the match
    // engine and ~10 formatters per visible row per frame.
    let trimmed = query.trim();
    for row in &mut rows {
        if !trimmed.is_empty() {
            row.name_highlights =
                taskmanager_application::text::match_ranges_ascii_ci(&row.name, trimmed);
        }
        if !row.cell_text.precomputed {
            row.cell_text = RowCellText::build(row);
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
    let applications: Vec<&ProcessItem> = processes
        .iter()
        .copied()
        .filter(|process| process_category(process) == ProcessCategory::Application)
        .collect();
    build_process_tree(&applications).len()
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
    pub collapsed: HashSet<u32>,
    pub expanded_apps: HashSet<String>,
    pub local_time_rules: taskmanager_application::LocalTimeRulesCacheKey,
    pub rows: Rc<Vec<VisibleRow>>,
    pub pids: Rc<Vec<u32>>,
    pub application_count: usize,
}

/// Pairwise parity: the gpui `category_rows` projection vs the neutral
/// `category_buckets` skeleton from `taskmanager-application`, on the shared
/// fixture. Bucket order, expansion keys (recovered from the typed toggle),
/// per-bucket aggregates, member counts, and expanded member order must agree
/// — the frontend owns only labels, row shape, and the PSS-preferred memory
/// metric it folds itself.
#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_processes_view_rows_projection_category_projection_parity.rs"]
mod category_projection_parity;

//! Applications page canonical category-tree row model.
//!
//! Builds the flat visual row lists consumed by both the Applications
//! renderer (`ui::render_processes`) and the TUI key handler
//! (`TuiApp::move_nonflat_selection_oneshot` and the typed group/tree
//! transition helpers).
//!
//! The product has one row hierarchy: category headers, application totals and
//! recursive process nodes. Historical presentation tokens are normalized by
//! `taskmanager-core::Config` and never enter this renderer.
//!
//! The shell's [`taskmanager_shell::ProcessTreeRow`] projection owns the O(N)
//! category/tree walk and its cacheable, borrowed-free row structure.
//! [`materialize_row`] / [`materialize_rows`] adapt one shared row into the
//! TUI-only text model. A visible window materializes only its own rows; the
//! lazy [`VisibleProcesses`] accessor resolves process facts on demand.
//!
//! The test-side `build_process_rows` helper materializes the shared rows and
//! remains the fresh-rebuild reference for cache-invalidation tests.

use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_core::core::process::{ProcessCategory, ProcessItem, ProcessLiveKey};
use taskmanager_shell::{ProcessRowId, ProcessTreeRow};

#[cfg(test)]
#[path = "../tests/headless/process_view_support.rs"]
pub(crate) mod process_view_support;

/// One row of the Applications category tree.
///
/// `Group` is emitted for every non-empty bucket or application subtree: it
/// carries availability-preserving typed aggregates for display plus the group
/// `name` that keys [`TuiApp::expanded_groups`] (for the category buckets this
/// is the locale-neutral `category:<stable_key>` token) and the English `label`
/// the renderer draws. `TreeNode` is a process row in the recursive hierarchy,
/// carrying its render metadata (depth for the
/// indentation and the expansion chevron, `collapsed` mirroring the node's
/// membership in [`TuiApp::collapsed_tree`] so the renderer never re-queries
/// the set).
#[derive(Debug)]
pub(crate) enum ProcessRow<'a> {
    /// A toggleable group header. `expanded` mirrors whether the group `name`
    /// is currently in [`TuiApp::expanded_groups`] so the renderer can draw the
    /// correct chevron without re-querying the set. `name` is the
    /// expansion-set key; `label` is the display text (identical for the
    /// app/type groups, the English category label for the category buckets).
    Group {
        name: String,
        label: String,
        depth: usize,
        count: usize,
        cpu: AggregateMetric<f32>,
        memory: AggregateMetric<u64>,
        expanded: bool,
        row_key: Option<ProcessRowId>,
    },
    /// A process-tree node with its hierarchy metadata.
    TreeNode {
        process: &'a ProcessItem,
        depth: usize,
        has_children: bool,
        collapsed: bool,
    },
}

/// Lazy by-index accessor over the shell's visible process projection
/// (the TUI-006 follow-up slice). The shell already memoizes the visible
/// ordering as raw indices (`ShellApp::visible_process_indices`); this type
/// pairs one memoized index vector with the authoritative process slice it
/// indexes so a consumer can resolve `&ProcessItem` values ON DEMAND instead
/// of materializing the whole per-frame `Vec<&ProcessItem>` pointer vector.
///
/// The lifetime `'a` is the borrow of the authoritative process slice, so a
/// resolved reference outlives the accessor itself — exactly the guarantee
/// the borrowed-slice helpers (`&[&ProcessItem]`) give their callers.
///
/// The memo key (process revision + query + filter + sort) pins the exact
/// visible ordering, and the TUI's own presentation cache key contains the
/// same revision, so an index can never silently address a different process
/// while a shared row slice is being resolved through this accessor.
#[derive(Clone)]
pub(crate) struct VisibleProcesses<'a> {
    indices: std::rc::Rc<Vec<usize>>,
    processes: &'a [ProcessItem],
}

impl<'a> VisibleProcesses<'a> {
    /// Pair the shell's memoized visible-row indices with the authoritative
    /// process slice they index into. Both must come from the same shell
    /// state (no projection mutation in between).
    #[must_use]
    pub(crate) fn new(indices: std::rc::Rc<Vec<usize>>, processes: &'a [ProcessItem]) -> Self {
        Self { indices, processes }
    }

    /// The number of visible rows — the length every shared row indexes
    /// against, identical to `ShellApp::visible_process_count`.
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.indices.len()
    }

    /// Whether no process is visible.
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// The visible process at `index`, resolved on demand (O(1) per lookup,
    /// zero per-frame O(N) allocation). Out-of-range returns `None`.
    #[must_use]
    pub(crate) fn get(&self, index: usize) -> Option<&'a ProcessItem> {
        self.processes.get(*self.indices.get(index)?)
    }

    /// The process a shared structural row addresses, resolved through this
    /// accessor (the owned-row counterpart used by the render path).
    #[must_use]
    pub(crate) fn process_of(&self, row: &ProcessTreeRow) -> Option<&'a ProcessItem> {
        process_id_on(row, self)
    }

    /// The actionable semantic key a shared structural row carries.
    #[must_use]
    pub(crate) fn row_key_of(&self, row: &ProcessTreeRow) -> Option<ProcessRowId> {
        row_key_on(row)
    }

    /// The expansion-set key a toggleable group-header row carries.
    #[must_use]
    pub(crate) fn expansion_key_of(&self, row: &ProcessTreeRow) -> Option<String> {
        expansion_key_on(row)
    }

    /// The process addressed by the shared row at `index` (the accessor
    /// counterpart of [`id_process`]).
    #[must_use]
    pub(crate) fn id_process(
        &self,
        ids: &[ProcessTreeRow],
        index: usize,
    ) -> Option<&'a ProcessItem> {
        ids.get(index).and_then(|id| self.process_of(id))
    }

    /// The semantic key at the shared row `index` (the accessor
    /// counterpart of [`id_row_key`]).
    #[must_use]
    pub(crate) fn id_row_key(&self, ids: &[ProcessTreeRow], index: usize) -> Option<ProcessRowId> {
        ids.get(index).and_then(|id| self.row_key_of(id))
    }

    /// Materialize the borrowed [`ProcessRow`] view of the shared row at
    /// `index` (the accessor counterpart of [`materialize_row`]). O(1) per
    /// row.
    #[must_use]
    pub(crate) fn materialize_row(
        &self,
        ids: &[ProcessTreeRow],
        index: usize,
    ) -> Option<ProcessRow<'a>> {
        materialize_row_on(ids, self, index)
    }
}

/// The private index-lookup seam shared by the borrowed-slice helpers and the
/// lazy [`VisibleProcesses`] accessor.
trait VisibleLookup<'a> {
    fn lookup(&self, index: usize) -> Option<&'a ProcessItem>;
}

impl<'a> VisibleLookup<'a> for [&'a ProcessItem] {
    fn lookup(&self, index: usize) -> Option<&'a ProcessItem> {
        self.get(index).copied()
    }
}

impl<'a> VisibleLookup<'a> for VisibleProcesses<'a> {
    fn lookup(&self, index: usize) -> Option<&'a ProcessItem> {
        self.get(index)
    }
}

/// The expansion key already computed by the shell structural projection.
fn expansion_key_on(row: &ProcessTreeRow) -> Option<String> {
    row.expansion_key().map(str::to_owned)
}

/// The typed actionable key already carried by the shell row.
fn row_key_on(row: &ProcessTreeRow) -> Option<ProcessRowId> {
    row.row_key()
}

/// Resolve a process-backed shared row through the current visible list.
fn process_id_on<'a, L: ?Sized + VisibleLookup<'a>>(
    row: &ProcessTreeRow,
    visible: &L,
) -> Option<&'a ProcessItem> {
    match row {
        ProcessTreeRow::Process { visible_index, .. } => visible.lookup(*visible_index),
        _ => None,
    }
}

/// The TUI renderer adapter for one shared row at `index`, re-fetching
/// display-specific facts through
/// the visible list behind `L` (a borrowed `&[&ProcessItem]` slice or the
/// lazy [`VisibleProcesses`] accessor — both resolve one index the same way).
fn materialize_row_on<'a, L: ?Sized + VisibleLookup<'a>>(
    ids: &[ProcessTreeRow],
    visible: &L,
    index: usize,
) -> Option<ProcessRow<'a>> {
    match ids.get(index)? {
        ProcessTreeRow::Category {
            category,
            expansion_key: name,
            expanded,
            member_count: count,
            aggregate,
            ..
        } => Some(ProcessRow::Group {
            name: name.clone(),
            label: category_label(*category).to_owned(),
            depth: 0,
            count: *count,
            cpu: aggregate.cpu().clone(),
            memory: aggregate.memory().clone(),
            expanded: *expanded,
            row_key: None,
        }),
        ProcessTreeRow::Application {
            visible_index,
            row_key,
            expansion_key,
            expanded,
            member_count: count,
            aggregate,
            ..
        } => {
            let root = visible.lookup(*visible_index)?;
            Some(ProcessRow::Group {
                name: expansion_key.clone(),
                label: root
                    .current_application_name()
                    .unwrap_or(&root.name)
                    .to_owned(),
                depth: 1,
                count: *count,
                cpu: aggregate.cpu().clone(),
                memory: aggregate.memory().clone(),
                expanded: *expanded,
                row_key: *row_key,
            })
        }
        ProcessTreeRow::Process {
            visible_index,
            depth,
            has_children,
            collapsed,
            ..
        } => Some(ProcessRow::TreeNode {
            process: visible.lookup(*visible_index)?,
            depth: *depth,
            has_children: *has_children,
            collapsed: *collapsed,
        }),
    }
}

/// English display label for one category bucket.
const fn category_label(category: ProcessCategory) -> &'static str {
    match category {
        ProcessCategory::Application => "Applications",
        ProcessCategory::Background => "Background processes",
        ProcessCategory::Uncategorized => "Uncategorized",
    }
}

/// Renderer layer: convert the shared row at `index`
/// back into the borrowed [`ProcessRow`] view by re-fetching the process
/// through the id's visible-list index. O(1) per row. A row whose id cannot
/// resolve against the visible list (impossible while the cache key matches,
/// since the key pins the exact visible list the ids were built from) is
/// skipped by the slice-level helpers instead of being fabricated.
pub(crate) fn materialize_row<'a>(
    ids: &[ProcessTreeRow],
    visible: &[&'a ProcessItem],
    index: usize,
) -> Option<ProcessRow<'a>> {
    materialize_row_on(ids, visible, index)
}

/// Materialize the id slice range `[start, end)` into borrowed rows. Rows
/// whose id cannot resolve (see [`materialize_row`]) are skipped rather than
/// fabricated, so the result can only be shorter than the requested window
/// in a cache-consistency bug — never wrong-content.
pub(crate) fn materialize_window<'a>(
    ids: &[ProcessTreeRow],
    visible: &[&'a ProcessItem],
    start: usize,
    end: usize,
) -> Vec<ProcessRow<'a>> {
    (start..end)
        .filter_map(|index| materialize_row(ids, visible, index))
        .collect()
}

/// Materialize every id in the slice (the whole-chain convenience used by
/// callers that still want the complete borrowed row list).
#[must_use]
pub(crate) fn materialize_rows<'a>(
    ids: &[ProcessTreeRow],
    visible: &[&'a ProcessItem],
) -> Vec<ProcessRow<'a>> {
    materialize_window(ids, visible, 0, ids.len())
}

/// Materialize the product's shared row hierarchy. This is the fresh-rebuild
/// reference chain: the cached shared rows must produce item-for-item
/// identical renderer rows under the same key (test-enforced).
/// The group NAME at `index` when that visual row is a toggleable group
/// header, else `None` (a process row, a tree row, or out of bounds). The
/// returned slice borrows from `rows`, not from the original process data, so
/// callers can clone it out of the row-scope borrow.
#[must_use]
pub(crate) fn group_name_at<'r>(rows: &'r [ProcessRow<'_>], index: usize) -> Option<&'r str> {
    match rows.get(index)? {
        ProcessRow::Group { name, .. } => Some(name.as_str()),
        ProcessRow::TreeNode { .. } => None,
    }
}

/// Semantic identity at one visual row. Category/type headers are structural;
/// application aggregates and real process rows are actionable selections.
#[must_use]
pub(crate) fn row_key_at(rows: &[ProcessRow<'_>], index: usize) -> Option<ProcessRowId> {
    match rows.get(index)? {
        ProcessRow::Group { row_key, .. } => *row_key,
        ProcessRow::TreeNode { process, .. } => ProcessRowId::from_process(process),
    }
}

/// The process at `index` when that shared row is a recursive process node,
/// else `None`. The owned-id counterpart of [`process_at`]; the reference
/// borrows from the visible list the ids index into, not from the ids.
/// Test-side reference resolver: production consumers resolve through the
/// [`VisibleProcesses`] accessor.
/// The process at `index` when that visual row is a recursive process node,
/// else `None` (a
/// group header or out of bounds). The returned reference borrows from the
/// original process data (lifetime `'a`), not the transient `rows` borrow, so
/// it can outlive the row-scope borrow that produced it.
#[must_use]
pub(crate) fn process_at<'a>(rows: &[ProcessRow<'a>], index: usize) -> Option<&'a ProcessItem> {
    match rows.get(index)? {
        ProcessRow::TreeNode { process, .. } => Some(*process),
        ProcessRow::Group { .. } => None,
    }
}

/// The category-first projection carries tree nodes in the same visual row
/// list as its headers. This resolver lets the keyboard path reuse the same
/// row model for Left/Right without rebuilding a second tree projection.
#[must_use]
pub(crate) fn category_tree_children_at(
    rows: &[ProcessRow<'_>],
    index: usize,
) -> Option<ProcessLiveKey> {
    match rows.get(index) {
        Some(ProcessRow::TreeNode {
            process,
            has_children: true,
            ..
        }) => ProcessLiveKey::from_process(process),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/gui/process_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/gui/perf_budget_tests.rs"]
mod perf_budget_tests;

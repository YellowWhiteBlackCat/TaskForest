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
//! The projection is split into two pure layers (TUI-006):
//!
//! 1. [`build_canonical_row_ids`] — the O(N) walk over the visible processes
//!    that emits a fully OWNED [`CanonicalRowId`] slice. The slice carries no
//!    borrowed process facts (only stable indices into the visible list plus
//!    precomputed header aggregates), so it can be cached across frames keyed
//!    by the same presentation inputs as the visual-row-count cache.
//! 2. [`materialize_row`] / [`materialize_rows`] — the O(1)-per-row conversion
//!    of one owned id back into the borrowed [`ProcessRow`] view the renderer
//!    consumes, re-fetching the process through the id's visible-list index.
//!    A visible window materializes only its own rows. Both the borrowed
//!    slice entry (`&[&ProcessItem]`) and the lazy [`VisibleProcesses`]
//!    accessor share one generic core, so the two entry shapes cannot drift;
//!    the accessor lets a per-frame consumer resolve `&ProcessItem` values on
//!    demand instead of materializing the whole O(N) visible pointer vector.
//!
//! `build_process_rows` is the whole-chain convenience (ids + full
//! materialize) and stays the fresh-rebuild reference the cache-invalidation
//! tests compare against.

use std::collections::{HashMap, HashSet};

use taskmanager_application::process_category_projection::{
    category_buckets, category_expansion_key,
};
use taskmanager_core::core::process::{
    ProcessCategory, ProcessItem, ProcessNode, build_process_tree, flatten_tree_visible,
    process_category,
};
use taskmanager_shell::{ProcessRowId, SortCol, SortDir};

/// One row of the Applications category tree.
///
/// `Group` is only ever emitted for a group with more than one member: it
/// carries the aggregated metrics for display plus the group `name` that keys
/// [`TuiApp::expanded_groups`] (for the category buckets this is the
/// locale-neutral `category:<stable_key>` token) and the English `label` the
/// renderer draws. `TreeNode` is a process row in the recursive hierarchy,
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
        cpu: f32,
        memory: u64,
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
/// while a canonical id slice is being resolved through this accessor.
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

    /// The number of visible rows — the length every canonical id indexes
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

    /// The process a canonical id addresses, resolved through this accessor
    /// (the owned-id counterpart used by the render path).
    #[must_use]
    pub(crate) fn process_of(&self, id: &CanonicalRowId) -> Option<&'a ProcessItem> {
        process_id_on(id, self)
    }

    /// The actionable semantic key a canonical id carries, resolved through
    /// this accessor.
    #[must_use]
    pub(crate) fn row_key_of(&self, id: &CanonicalRowId) -> Option<ProcessRowId> {
        row_key_on(id, self)
    }

    /// The expansion-set key a toggleable group-header id carries, resolved
    /// through this accessor.
    #[must_use]
    pub(crate) fn expansion_key_of(&self, id: &CanonicalRowId) -> Option<String> {
        expansion_key_on(id, self)
    }

    /// The process addressed by the canonical id at `index` (the accessor
    /// counterpart of [`id_process`]).
    #[must_use]
    pub(crate) fn id_process(
        &self,
        ids: &[CanonicalRowId],
        index: usize,
    ) -> Option<&'a ProcessItem> {
        ids.get(index).and_then(|id| self.process_of(id))
    }

    /// The semantic key at the canonical id `index` (the accessor
    /// counterpart of [`id_row_key`]).
    #[must_use]
    pub(crate) fn id_row_key(&self, ids: &[CanonicalRowId], index: usize) -> Option<ProcessRowId> {
        ids.get(index).and_then(|id| self.row_key_of(id))
    }

    /// Materialize the borrowed [`ProcessRow`] view of the canonical id at
    /// `index` (the accessor counterpart of [`materialize_row`]). O(1) per
    /// row.
    #[must_use]
    pub(crate) fn materialize_row(
        &self,
        ids: &[CanonicalRowId],
        index: usize,
    ) -> Option<ProcessRow<'a>> {
        materialize_row_on(ids, self, index)
    }
}

/// The private index-lookup seam shared by the borrowed-slice helpers and the
/// lazy [`VisibleProcesses`] accessor, so the row-materialization rules
/// (aggregate fields, visible-index re-fetch, fail-closed `None`) exist
/// exactly once and the two entry shapes cannot drift.
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

/// The shared core of [`CanonicalRowId::expansion_key`].
fn expansion_key_on<'a, L: ?Sized + VisibleLookup<'a>>(
    id: &CanonicalRowId,
    visible: &L,
) -> Option<String> {
    match id {
        CanonicalRowId::Category { category, .. } => Some(category_expansion_key(*category)),
        CanonicalRowId::AppRoot { visible_index, .. } => {
            let root = visible.lookup(*visible_index)?;
            Some(format!("{}{}", APP_TREE_EXPANSION_KEY_PREFIX, root.pid))
        }
        CanonicalRowId::Process { .. } => None,
    }
}

/// The shared core of [`CanonicalRowId::row_key`].
fn row_key_on<'a, L: ?Sized + VisibleLookup<'a>>(
    id: &CanonicalRowId,
    visible: &L,
) -> Option<ProcessRowId> {
    match id {
        CanonicalRowId::Category { .. } => None,
        CanonicalRowId::AppRoot { visible_index, .. } => visible
            .lookup(*visible_index)
            .and_then(ProcessRowId::application_of),
        CanonicalRowId::Process { visible_index, .. } => visible
            .lookup(*visible_index)
            .and_then(ProcessRowId::from_process),
    }
}

/// The shared core of [`CanonicalRowId::process`].
fn process_id_on<'a, L: ?Sized + VisibleLookup<'a>>(
    id: &CanonicalRowId,
    visible: &L,
) -> Option<&'a ProcessItem> {
    match id {
        CanonicalRowId::Process { visible_index, .. } => visible.lookup(*visible_index),
        _ => None,
    }
}

/// The shared core of [`materialize_row`]: one borrowed `ProcessRow` view of
/// the canonical id at `index`, re-fetching display-specific facts through
/// the visible list behind `L` (a borrowed `&[&ProcessItem]` slice or the
/// lazy [`VisibleProcesses`] accessor — both resolve one index the same way).
fn materialize_row_on<'a, L: ?Sized + VisibleLookup<'a>>(
    ids: &[CanonicalRowId],
    visible: &L,
    index: usize,
) -> Option<ProcessRow<'a>> {
    match ids.get(index)? {
        CanonicalRowId::Category {
            category,
            expanded,
            count,
            cpu,
            memory,
        } => Some(ProcessRow::Group {
            name: category_expansion_key(*category),
            label: category_label(*category).to_owned(),
            depth: 0,
            count: *count,
            cpu: *cpu,
            memory: *memory,
            expanded: *expanded,
            row_key: None,
        }),
        CanonicalRowId::AppRoot {
            visible_index,
            expanded,
            count,
            cpu,
            memory,
        } => {
            let root = visible.lookup(*visible_index)?;
            Some(ProcessRow::Group {
                name: format!("{}{}", APP_TREE_EXPANSION_KEY_PREFIX, root.pid),
                label: root
                    .current_application_name()
                    .unwrap_or(&root.name)
                    .to_owned(),
                depth: 1,
                count: *count,
                cpu: *cpu,
                memory: *memory,
                expanded: *expanded,
                row_key: ProcessRowId::application_of(root),
            })
        }
        CanonicalRowId::Process {
            visible_index,
            depth,
            has_children,
            collapsed,
        } => Some(ProcessRow::TreeNode {
            process: visible.lookup(*visible_index)?,
            depth: *depth,
            has_children: *has_children,
            collapsed: *collapsed,
        }),
    }
}

/// Fully-owned identity of one canonical Applications row (TUI-006).
///
/// The id slice is the cacheable half of the row projection: it borrows
/// nothing from the process data, so it can outlive any single frame inside
/// the TUI's presentation cache. Everything display-specific is either
/// precomputed at build time (the header aggregates, whose floating-point
/// sums are computed in exactly the traversal order the fresh rebuild uses)
/// or re-fetched at materialize time through `visible_index` — the row's
/// position in the visible process list the cache key pins.
///
/// The visible-list index (not a pointer) is the only linkage to process
/// facts: the cache key contains the process revision, so the visible list
/// the ids were built from is byte-identical to the one every materialize
/// under the same key sees, and an index can never silently address a
/// different process.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CanonicalRowId {
    /// A category bucket header (depth 0, structural — never actionable).
    Category {
        category: ProcessCategory,
        expanded: bool,
        count: usize,
        cpu: f32,
        memory: u64,
    },
    /// One application aggregate header (depth 1). `visible_index` addresses
    /// the aggregate's root process in the visible list, so the label, the
    /// pid-keyed expansion key and the actionable row key are re-fetched from
    /// the single authoritative process at materialize time.
    AppRoot {
        visible_index: usize,
        expanded: bool,
        count: usize,
        cpu: f32,
        memory: u64,
    },
    /// One recursive process node.
    Process {
        visible_index: usize,
        depth: usize,
        has_children: bool,
        collapsed: bool,
    },
}

impl CanonicalRowId {
    /// The process this row addresses when it is a recursive process node,
    /// else `None` (a group header aggregates but never resolves to one
    /// process — same rule as [`process_at`]). Test-side reference resolver:
    /// production consumers resolve through the [`VisibleProcesses`] accessor.
    #[cfg(test)]
    pub(crate) fn process<'a>(&self, visible: &[&'a ProcessItem]) -> Option<&'a ProcessItem> {
        process_id_on(self, visible)
    }
}

/// Expansion-set key prefix for one application aggregate header
/// (`app-tree:<root pid>`). Declared here so the generator and the stale-key
/// pruner share one spelling and can never drift.
pub(crate) const APP_TREE_EXPANSION_KEY_PREFIX: &str = "app-tree:";

/// English display label for one category bucket.
const fn category_label(category: ProcessCategory) -> &'static str {
    match category {
        ProcessCategory::Application => "Applications",
        ProcessCategory::Background => "Background processes",
        ProcessCategory::Uncategorized => "Uncategorized",
    }
}

/// Layer 1 of the canonical row projection: the pure O(N) walk over the
/// visible processes that emits the fully-owned [`CanonicalRowId`] slice.
/// Pure and side-effect-free, with the exact aggregate computations (and
/// floating-point summation order) of the row projection, so materializing
/// the ids under one cache key is byte-identical to a fresh rebuild.
///
/// Header aggregates are computed here (once per key change) instead of at
/// materialize time: summing a bucket or an application subtree costs
/// O(members), and the point of the id slice is a per-frame cost that only
/// follows the visible window.
///
/// The ids address processes by their visible-list index through a pid →
/// index map. That reuses the feature's existing pid identity contract (the
/// tree builder, the `app-tree:<pid>` expansion keys and the collapse set are
/// all pid-keyed); the map is last-wins for a pid, matching the tree
/// builder's own pid map.
#[must_use]
pub(crate) fn build_canonical_row_ids(
    processes: &[&ProcessItem],
    expanded: &HashSet<String>,
    collapsed: &HashSet<u32>,
    sort: (SortCol, SortDir),
) -> Vec<CanonicalRowId> {
    let visible_index_by_pid: HashMap<u32, usize> = processes
        .iter()
        .enumerate()
        .map(|(index, process)| (process.pid, index))
        .collect();
    let mut ids: Vec<CanonicalRowId> = Vec::new();
    for bucket in category_buckets(processes, |process| process_category(process)) {
        let members: Vec<&ProcessItem> = bucket.members().iter().map(|member| **member).collect();
        let key = category_expansion_key(bucket.category());
        let is_expanded = expanded.contains(&key);
        ids.push(CanonicalRowId::Category {
            category: bucket.category(),
            expanded: is_expanded,
            count: bucket.member_count(),
            cpu: bucket.sum_f32(|process| process.current_cpu_percentage()),
            memory: bucket.sum_u64(|process| process.current_memory_bytes()),
        });
        if !is_expanded {
            continue;
        }
        let mut tree = build_process_tree(&members);
        sort_tree_nodes(&mut tree, sort);
        if bucket.category() == ProcessCategory::Application {
            for root in &tree {
                let (count, cpu, memory) = tree_totals(root);
                let app_key = format!("{}{}", APP_TREE_EXPANSION_KEY_PREFIX, root.item.pid);
                let app_expanded = expanded.contains(&app_key);
                ids.push(CanonicalRowId::AppRoot {
                    visible_index: visible_index_by_pid
                        .get(&root.item.pid)
                        .copied()
                        .unwrap_or(usize::MAX),
                    expanded: app_expanded,
                    count,
                    cpu,
                    memory,
                });
                if app_expanded {
                    push_tree_ids(
                        &mut ids,
                        std::slice::from_ref(root),
                        collapsed,
                        &visible_index_by_pid,
                        2,
                    );
                }
            }
        } else {
            push_tree_ids(&mut ids, &tree, collapsed, &visible_index_by_pid, 1);
        }
    }
    ids
}

/// Append the visible flattened nodes of one tree level to the id slice. The
/// depth offset (2 under an application aggregate, 1 directly under a
/// category) and the collapsed-set reads mirror the row projection exactly.
fn push_tree_ids(
    ids: &mut Vec<CanonicalRowId>,
    tree: &[ProcessNode<'_>],
    collapsed: &HashSet<u32>,
    visible_index_by_pid: &HashMap<u32, usize>,
    depth_offset: usize,
) {
    for node in flatten_tree_visible(tree, collapsed) {
        ids.push(CanonicalRowId::Process {
            visible_index: visible_index_by_pid
                .get(&node.item.pid)
                .copied()
                .unwrap_or(usize::MAX),
            depth: node.depth.saturating_add(depth_offset),
            has_children: node.has_children,
            collapsed: collapsed.contains(&node.item.pid),
        });
    }
}

/// Layer 2 of the canonical row projection: convert the owned id at `index`
/// back into the borrowed [`ProcessRow`] view by re-fetching the process
/// through the id's visible-list index. O(1) per row. A row whose id cannot
/// resolve against the visible list (impossible while the cache key matches,
/// since the key pins the exact visible list the ids were built from) is
/// skipped by the slice-level helpers instead of being fabricated.
pub(crate) fn materialize_row<'a>(
    ids: &[CanonicalRowId],
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
    ids: &[CanonicalRowId],
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
    ids: &[CanonicalRowId],
    visible: &[&'a ProcessItem],
) -> Vec<ProcessRow<'a>> {
    materialize_window(ids, visible, 0, ids.len())
}

/// Project visible processes into the product's only row hierarchy: the
/// owned canonical id slice followed by a full materialize. This is the
/// fresh-rebuild REFERENCE chain: the cached id slice must produce
/// item-for-item identical rows under the same key (test-enforced), so the
/// invalidation tests rebuild through here and compare against the cache.
#[cfg(test)]
#[must_use]
pub(crate) fn build_process_rows<'a>(
    processes: &[&'a ProcessItem],
    expanded: &HashSet<String>,
    collapsed: &HashSet<u32>,
    sort: (SortCol, SortDir),
) -> Vec<ProcessRow<'a>> {
    let ids = build_canonical_row_ids(processes, expanded, collapsed, sort);
    materialize_rows(&ids, processes)
}

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

/// The process at `index` when that canonical id is a recursive process node,
/// else `None`. The owned-id counterpart of [`process_at`]; the reference
/// borrows from the visible list the ids index into, not from the ids.
/// Test-side reference resolver: production consumers resolve through the
/// [`VisibleProcesses`] accessor.
#[cfg(test)]
pub(crate) fn id_process<'a>(
    ids: &[CanonicalRowId],
    visible: &[&'a ProcessItem],
    index: usize,
) -> Option<&'a ProcessItem> {
    ids.get(index)?.process(visible)
}

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

/// Recursively sort a process tree by the shared process-table sort. Every
/// column projects through the shell's `sort_axis` translation onto the
/// neutral comparator — the same single ordering the iced/gpui tree
/// projections and the shell `visible_processes` path apply — so the tree
/// mode cannot drift from the flat order or the other frontends. The
/// comparator carries the direction (plus the direction-independent pid
/// tie-break); this helper only recurses it into every child level.
fn sort_tree_nodes<'a>(nodes: &mut [ProcessNode<'a>], sort: (SortCol, SortDir)) {
    let (column, direction) = sort;
    let ascending = matches!(direction, SortDir::Asc);
    let axis = taskmanager_shell::sort_axis(column);
    sort_tree_nodes_by(nodes, &|left, right| {
        taskmanager_application::process_sort::compare_processes(left, right, axis, ascending)
    });
}

fn sort_tree_nodes_by<'a, F>(nodes: &mut [ProcessNode<'a>], cmp: &F)
where
    F: Fn(&ProcessItem, &ProcessItem) -> std::cmp::Ordering,
{
    nodes.sort_by(|left, right| cmp(left.item, right.item));
    for node in nodes.iter_mut() {
        sort_tree_nodes_by(&mut node.children, cmp);
    }
}

fn tree_totals(root: &ProcessNode<'_>) -> (usize, f32, u64) {
    fn fold(node: &ProcessNode<'_>, count: &mut usize, cpu: &mut f32, memory: &mut u64) {
        *count = count.saturating_add(1);
        *cpu += node.item.current_cpu_percentage().unwrap_or(0.0);
        *memory = memory.saturating_add(node.item.current_memory_bytes().unwrap_or(0));
        for child in &node.children {
            fold(child, count, cpu, memory);
        }
    }

    let (mut count, mut cpu, mut memory) = (0, 0.0, 0);
    fold(root, &mut count, &mut cpu, &mut memory);
    (count, cpu, memory)
}

/// The category-first projection carries tree nodes in the same visual row
/// list as its headers. This resolver lets the keyboard path reuse the same
/// row model for Left/Right without rebuilding a second tree projection.
#[must_use]
pub(crate) fn category_tree_children_at(rows: &[ProcessRow<'_>], index: usize) -> Option<u32> {
    match rows.get(index) {
        Some(ProcessRow::TreeNode {
            process,
            has_children: true,
            ..
        }) => Some(process.pid),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/gui/process_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/gui/perf_budget_tests.rs"]
mod perf_budget_tests;

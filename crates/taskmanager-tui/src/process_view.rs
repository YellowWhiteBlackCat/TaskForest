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

use std::collections::HashSet;

use taskmanager_application::process_category_projection::{
    category_buckets, category_expansion_key,
};
use taskmanager_application::{
    ProcessCategory, ProcessItem, ProcessNode, build_process_tree, flatten_tree_visible,
    process_category,
};
use taskmanager_shell::{ProcessRowKey, SortCol, SortDir};

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
        row_key: Option<ProcessRowKey>,
    },
    /// A process-tree node with its hierarchy metadata.
    TreeNode {
        process: &'a ProcessItem,
        depth: usize,
        has_children: bool,
        collapsed: bool,
    },
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

/// Project the visible processes into the category-first tree rows — via the
/// neutral `category_buckets` projection shared with the other frontends
/// (fixed `ALL` order, empty buckets never fabricate a header, members in
/// the shared visible-list order which IS the active sort), expansion keyed
/// by the shared [`category_expansion_key`]. The classification itself is
/// the single core `process_category` source — never re-derived here.
fn category_rows<'a>(
    processes: &[&'a ProcessItem],
    expanded: &HashSet<String>,
    collapsed: &HashSet<u32>,
    sort: (SortCol, SortDir),
) -> Vec<ProcessRow<'a>> {
    let mut rows = Vec::new();
    for bucket in category_buckets(processes, |process| process_category(process)) {
        let members: Vec<&'a ProcessItem> =
            bucket.members().iter().map(|member| **member).collect();
        let key = category_expansion_key(bucket.category());
        let is_expanded = expanded.contains(&key);
        rows.push(ProcessRow::Group {
            label: category_label(bucket.category()).to_owned(),
            depth: 0,
            count: bucket.member_count(),
            cpu: bucket.sum_f32(|process| process.current_cpu_percentage()),
            memory: bucket.sum_u64(|process| process.current_memory_bytes()),
            expanded: is_expanded,
            name: key,
            row_key: None,
        });
        if is_expanded {
            let mut tree = build_process_tree(&members);
            sort_tree_nodes(&mut tree, sort);
            if bucket.category() == ProcessCategory::Application {
                for root in &tree {
                    let (count, cpu, memory) = tree_totals(root);
                    let app_key = format!("{}{}", APP_TREE_EXPANSION_KEY_PREFIX, root.item.pid);
                    let app_expanded = expanded.contains(&app_key);
                    rows.push(ProcessRow::Group {
                        name: app_key,
                        label: root
                            .item
                            .current_application_name()
                            .unwrap_or(&root.item.name)
                            .to_owned(),
                        depth: 1,
                        count,
                        cpu,
                        memory,
                        expanded: app_expanded,
                        row_key: Some(ProcessRowKey::Application(root.item.pid)),
                    });
                    if app_expanded {
                        rows.extend(
                            flatten_tree_visible(std::slice::from_ref(root), collapsed)
                                .into_iter()
                                .map(|node| ProcessRow::TreeNode {
                                    process: node.item,
                                    depth: node.depth.saturating_add(2),
                                    has_children: node.has_children,
                                    collapsed: collapsed.contains(&node.item.pid),
                                }),
                        );
                    }
                }
            } else {
                rows.extend(
                    flatten_tree_visible(&tree, collapsed)
                        .into_iter()
                        .map(|node| ProcessRow::TreeNode {
                            process: node.item,
                            depth: node.depth.saturating_add(1),
                            has_children: node.has_children,
                            collapsed: collapsed.contains(&node.item.pid),
                        }),
                );
            }
        }
    }
    rows
}

/// Project visible processes into the product's only row hierarchy.
#[must_use]
pub(crate) fn build_process_rows<'a>(
    processes: &[&'a ProcessItem],
    expanded: &HashSet<String>,
    collapsed: &HashSet<u32>,
    sort: (SortCol, SortDir),
) -> Vec<ProcessRow<'a>> {
    category_rows(processes, expanded, collapsed, sort)
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
pub(crate) fn row_key_at(rows: &[ProcessRow<'_>], index: usize) -> Option<ProcessRowKey> {
    match rows.get(index)? {
        ProcessRow::Group { row_key, .. } => *row_key,
        ProcessRow::TreeNode { process, .. } => Some(ProcessRowKey::Process(process.pid)),
    }
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

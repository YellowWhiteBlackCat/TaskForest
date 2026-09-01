//! Test-side reference resolvers for the shared category-tree row model:
//! the fresh-rebuild chain and the row-to-process lookup the cache
//! invalidation and selection tests compare against. Production consumers
//! resolve through [`super::VisibleProcesses`] and the memoized caches.

use std::collections::HashSet;

use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::{ProcessTreeRow, SortCol, SortDir, project_process_tree_rows};

use super::{ProcessRow, materialize_rows};

/// The whole-chain convenience: shared rows followed by a full materialize.
/// The fresh-rebuild REFERENCE the cache-invalidation tests compare against.
#[must_use]
pub(crate) fn build_process_rows<'a>(
    processes: &[&'a ProcessItem],
    expanded: &HashSet<String>,
    collapsed: &HashSet<ProcessLiveKey>,
    sort: (SortCol, SortDir),
    observed_at_ms: u64,
) -> Vec<ProcessRow<'a>> {
    let rows = project_process_tree_rows(processes, expanded, collapsed, sort, observed_at_ms);
    materialize_rows(&rows, processes)
}

/// The process at `index` when that visual row is a recursive process node,
/// else `None` (a group header or out of bounds).
#[must_use]
pub(crate) fn id_process<'a>(
    ids: &[ProcessTreeRow],
    visible: &[&'a ProcessItem],
    index: usize,
) -> Option<&'a ProcessItem> {
    match ids.get(index)? {
        ProcessTreeRow::Process { visible_index, .. } => visible.get(*visible_index).copied(),
        ProcessTreeRow::Category { .. } | ProcessTreeRow::Application { .. } => None,
    }
}

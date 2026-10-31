//! Test-side fresh-rebuild reference for the shared category-tree row model,
//! used by cache-invalidation and selection tests. Production consumers
//! resolve through [`super::VisibleProcesses`] and the memoized caches.

use std::collections::HashSet;

use taskforest_core::core::process::ProcessItem;
use taskforest_core::core::process::ProcessLiveKey;
use taskmanager_shell::{SortCol, SortDir, project_process_tree_rows};

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

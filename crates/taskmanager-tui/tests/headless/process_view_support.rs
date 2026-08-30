//! Test-side reference resolvers for the canonical category-tree row model:
//! the fresh-rebuild chain and the row-to-process lookup the cache
//! invalidation and selection tests compare against. Production consumers
//! resolve through [`super::VisibleProcesses`] and the memoized caches.

use std::collections::HashSet;

use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::{SortCol, SortDir};

use super::{CanonicalRowId, ProcessRow, build_canonical_row_ids, materialize_rows, process_id_on};

/// The whole-chain convenience: canonical ids followed by a full materialize.
/// The fresh-rebuild REFERENCE the cache-invalidation tests compare against.
#[must_use]
pub(crate) fn build_process_rows<'a>(
    processes: &[&'a ProcessItem],
    expanded: &HashSet<String>,
    collapsed: &HashSet<ProcessLiveKey>,
    sort: (SortCol, SortDir),
    observed_at_ms: u64,
) -> Vec<ProcessRow<'a>> {
    let ids = build_canonical_row_ids(processes, expanded, collapsed, sort, observed_at_ms);
    materialize_rows(&ids, processes)
}

/// The process at `index` when that visual row is a recursive process node,
/// else `None` (a group header or out of bounds).
#[must_use]
pub(crate) fn id_process<'a>(
    ids: &[CanonicalRowId],
    visible: &[&'a ProcessItem],
    index: usize,
) -> Option<&'a ProcessItem> {
    ids.get(index)?.process(visible)
}

impl CanonicalRowId {
    /// The process this row addresses when it is a recursive process node,
    /// else `None` (a group header aggregates but never resolves to one
    /// process — same rule as [`super::process_at`]). Test-side reference
    /// resolver: production consumers resolve through the [`super::VisibleProcesses`]
    /// accessor.
    #[must_use]
    pub(crate) fn process<'a>(&self, visible: &[&'a ProcessItem]) -> Option<&'a ProcessItem> {
        process_id_on(self, visible)
    }
}

//! Multi-select range resolution and set maintenance shared by grouped/tree
//! frontends (ADR-027). Selection state is identity-authoritative (CORE-01):
//! the batch set and the semantic row carry validated live identities
//! (pid + provider start token), so refresh, reorder, disappearance, and pid
//! reuse each have one deterministic outcome. The positional cursor
//! ([`ShellApp::selected`]) is derived state that follows the identity when
//! it resolves.

use std::collections::HashSet;

use taskmanager_application::AppPage;
use taskmanager_core::core::process::ProcessItem;

use super::ShellApp;
use super::process_rows::{ProcessRowId, ProcessRowIdentity};
use super::sorting::{SortCol, SortDir};
use crate::ProcessStatusFilter;

/// Memo entry for the visible-row projection consumed by
/// `ShellApp::visible_processes_indices`; kept here with the other
/// selection/visibility projection state so the parent module stays under
/// its source-line ceiling.
#[derive(Clone, Debug)]
pub(crate) struct VisibleProcessesMemo {
    pub(crate) process_revision: u64,
    pub(crate) query: String,
    pub(crate) status_filter: ProcessStatusFilter,
    pub(crate) sort: (SortCol, SortDir),
    pub(crate) source_len: usize,
    pub(crate) indices: std::rc::Rc<Vec<usize>>,
}

impl ShellApp {
    /// Clear the multi-select set. The keyboard batch menu uses this for its
    /// "clear selection" action; the set is otherwise emptied implicitly by
    /// selection collapse / list refresh.
    pub fn clear_selected_rows(&mut self) {
        self.selected_rows.clear();
    }

    /// The memoized raw indices behind [`Self::visible_processes`]. Frontends
    /// that already own a projection cache can pair these indices with the
    /// current process slice and avoid allocating a fresh `Vec<&ProcessItem>`
    /// on every view rebuild. The indices are valid only for the current
    /// process snapshot and the current query/filter/sort state.
    #[must_use]
    pub fn visible_process_indices(&self) -> std::rc::Rc<Vec<usize>> {
        self.visible_processes_indices()
    }

    /// Count the current visible process projection without materializing a
    /// second vector of references.
    #[must_use]
    pub fn visible_process_count(&self) -> usize {
        self.visible_processes_indices().len()
    }

    /// Resolve one row in the current visible process projection directly
    /// against the authoritative process snapshot.
    #[must_use]
    pub fn visible_process_at(&self, index: usize) -> Option<&ProcessItem> {
        let raw_index = *self.visible_processes_indices().get(index)?;
        self.data.processes.as_deref()?.get(raw_index)
    }

    /// Find a visible process by PID without allocating the full borrowed-row
    /// vector. The scan is reserved for menu/detail actions; renderers should
    /// use [`Self::visible_process_indices`] for their row projection.
    #[must_use]
    pub fn visible_process_by_pid(&self, pid: u32) -> Option<&ProcessItem> {
        let processes = self.data.processes.as_deref()?;
        self.visible_processes_indices()
            .iter()
            .find_map(|&raw_index| {
                processes
                    .get(raw_index)
                    .filter(|process| process.pid == pid)
            })
    }

    /// Find the visible-row position for a PID without allocating borrowed
    /// row references. Used by renderer-local menu actions before they route
    /// back through the shared selection reducer.
    #[must_use]
    pub fn visible_process_index_of_pid(&self, pid: u32) -> Option<usize> {
        let processes = self.data.processes.as_deref()?;
        self.visible_processes_indices()
            .iter()
            .position(|&raw_index| processes.get(raw_index).is_some_and(|p| p.pid == pid))
    }

    /// The validated live identity at one visible-row position, when the row
    /// carries a current provider start token.
    #[must_use]
    pub fn row_identity_at(&self, index: usize) -> Option<ProcessRowIdentity> {
        self.visible_process_at(index)
            .and_then(ProcessRowIdentity::from_process)
    }

    /// The visible-row position of one exact live identity (pid AND start
    /// token); a pid-reuse impostor never matches.
    #[must_use]
    pub fn visible_position_of_identity(&self, identity: ProcessRowIdentity) -> Option<usize> {
        let processes = self.data.processes.as_deref()?;
        self.visible_processes_indices()
            .iter()
            .position(|&raw_index| {
                processes.get(raw_index).is_some_and(|process| {
                    process.pid == identity.pid()
                        && process.current_start_token() == Some(identity.start_token())
                })
            })
    }

    /// Whether one process row is part of the batch set, by exact live
    /// identity.
    #[must_use]
    pub fn is_process_selected(&self, process: &ProcessItem) -> bool {
        ProcessRowIdentity::from_process(process)
            .is_some_and(|identity| self.selected_rows.contains(&identity))
    }

    /// The authoritative multi-select identity set (renderers highlight
    /// members; batch intents freeze from this).
    #[must_use]
    pub fn selected_identities(&self) -> &HashSet<ProcessRowIdentity> {
        &self.selected_rows
    }
}

impl ShellApp {
    pub fn move_selection(&mut self, delta: isize) {
        let length = self.active_row_count();
        if length == 0 {
            self.clear_empty_selection();
            return;
        }
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(length.saturating_sub(1));
        self.sync_application_selection();
        self.collapse_selection_to_anchor();
    }

    pub fn move_selection_to_first(&mut self) {
        if self.active_row_count() == 0 {
            self.clear_empty_selection();
            return;
        }
        self.selected = 0;
        self.sync_application_selection();
        self.collapse_selection_to_anchor();
    }

    pub fn move_selection_to_last(&mut self) {
        let length = self.active_row_count();
        if length == 0 {
            self.clear_empty_selection();
            return;
        }
        self.selected = length - 1;
        self.sync_application_selection();
        self.collapse_selection_to_anchor();
    }

    #[must_use]
    pub fn select_row(&mut self, index: usize) -> bool {
        if index >= self.active_row_count() {
            return false;
        }
        self.selected = index;
        self.sync_application_selection();
        self.collapse_selection_to_anchor();
        true
    }

    #[must_use]
    pub fn select_application_row(&mut self, root_pid: u32, flat_index: usize) -> bool {
        let Some(root) = (self.page() == AppPage::Applications)
            .then(|| self.visible_process_at(flat_index))
            .flatten()
            .filter(|process| process.pid == root_pid)
            .and_then(ProcessRowIdentity::from_process)
        else {
            return false;
        };
        self.selected = flat_index;
        self.selected_rows.clear();
        self.selected_row = Some(ProcessRowId::Application(root));
        self.application.selected_process = None;
        true
    }

    #[must_use]
    pub fn toggle_row_selection(&mut self, index: usize) -> bool {
        let Some(identity) = self.row_identity_at(index) else {
            return false;
        };
        self.selected = index;
        self.selected_row = Some(ProcessRowId::Process(identity));
        self.toggle_selected_identity(identity);
        self.sync_application_selection();
        true
    }

    pub fn toggle_selected_identity(&mut self, identity: ProcessRowIdentity) {
        if !self.selected_rows.insert(identity) {
            self.selected_rows.remove(&identity);
        }
        self.selected_row = Some(ProcessRowId::Process(identity));
    }

    #[must_use]
    pub fn extend_row_selection(&mut self, index: usize) -> bool {
        let rows = self.visible_processes();
        let Some(end) = rows
            .get(index)
            .and_then(|row| ProcessRowIdentity::from_process(row))
        else {
            return false;
        };
        let anchor = rows
            .get(self.selected)
            .and_then(|row| ProcessRowIdentity::from_process(row))
            .unwrap_or(end);
        let range = selected_rows_range(&rows, anchor, end);
        drop(rows);
        self.selected_rows.extend(range);
        self.selected = index;
        self.selected_row = Some(ProcessRowId::Process(end));
        self.sync_application_selection();
        true
    }

    /// Reconcile selection against the accepted process snapshot (CORE-01):
    /// identities survive only when their full live key (pid + start token)
    /// resolves, so a pid reused by a new process is dropped instead of
    /// silently retargeting. The cursor follows a resolvable identity to its
    /// new position (reorder-stability); otherwise it keeps its position for
    /// the nearest-survivor clamp path.
    pub fn prune_stale_selection(&mut self) {
        let processes = self.data.processes.as_deref().unwrap_or_default();
        let live: HashSet<ProcessRowIdentity> = processes
            .iter()
            .filter_map(ProcessRowIdentity::from_process)
            .collect();
        self.selected_rows
            .retain(|identity| live.contains(identity));
        match self.selected_row {
            Some(ProcessRowId::Application(root) | ProcessRowId::Process(root))
                if live.contains(&root) =>
            {
                if let Some(position) = self.visible_position_of_identity(root) {
                    self.selected = position;
                }
            }
            Some(ProcessRowId::Application(_) | ProcessRowId::Process(_)) => {
                self.selected_row = None;
            }
            Some(ProcessRowId::Category(_)) | None => {}
        }
        if let Some(frozen) = self.selected_process_identity() {
            self.application.selected_process = Some(frozen);
        } else {
            self.application.selected_process = None;
        }
    }

    pub fn clear_process_selection(&mut self) {
        self.selected_rows.clear();
        self.selected_row = None;
        self.application.selected_process = None;
    }

    pub fn push_search_char(&mut self, character: char) {
        if !character.is_control() {
            self.query.push(character);
            self.reset_selection_after_query_change();
        }
    }

    pub fn pop_search_char(&mut self) {
        self.query.pop();
        self.reset_selection_after_query_change();
    }

    pub(super) fn collapse_selection_to_anchor(&mut self) {
        self.selected_rows.clear();
        if let Some(identity) = self.row_identity_at(self.selected) {
            self.selected_rows.insert(identity);
            self.selected_row = Some(ProcessRowId::Process(identity));
        } else {
            self.selected_row = None;
        }
    }

    fn clear_empty_selection(&mut self) {
        self.selected = 0;
        self.selected_rows.clear();
        self.selected_row = None;
    }

    fn reset_selection_after_query_change(&mut self) {
        self.selected = 0;
        self.sync_application_selection();
        self.collapse_selection_to_anchor();
    }
}

/// The identity range spanning `anchor` → `end` (inclusive, in the caller's
/// display order). Pure so grouped/tree frontends can extend their
/// multi-select ranges against their OWN visual-row projection without
/// borrowing `&mut self` while the row slice is alive: the caller collects
/// the range, drops the borrow, then inserts into
/// [`super::ShellApp::selected_rows`]. A missing end (stale identity) yields
/// just `end`; a missing anchor degenerates to the single identity.
#[must_use]
pub fn selected_rows_range(
    rows: &[&ProcessItem],
    anchor: ProcessRowIdentity,
    end: ProcessRowIdentity,
) -> Vec<ProcessRowIdentity> {
    let identities: Vec<ProcessRowIdentity> = rows
        .iter()
        .filter_map(|row| ProcessRowIdentity::from_process(row))
        .collect();
    let start = identities.iter().position(|identity| *identity == anchor);
    let end_pos = identities.iter().position(|identity| *identity == end);
    match (start, end_pos) {
        (Some(start), Some(end_pos)) => {
            identities[start.min(end_pos)..=start.max(end_pos)].to_vec()
        }
        _ => vec![end],
    }
}

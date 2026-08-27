//! Multi-select range resolution and set maintenance shared by grouped/tree
//! frontends (ADR-027).

use taskmanager_application::{AppPage, ProcessItem};

use super::sorting::{SortCol, SortDir};
use super::{ProcessRowKey, ShellApp};
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
    pub fn clear_selected_pids(&mut self) {
        self.selected_pids.clear();
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
        if self.page() != AppPage::Applications
            || self
                .visible_process_at(flat_index)
                .map(|process| process.pid)
                != Some(root_pid)
        {
            return false;
        }
        self.selected = flat_index;
        self.selected_pids.clear();
        self.selected_process_row = Some(ProcessRowKey::Application(root_pid));
        self.application.selected_process = None;
        true
    }

    pub fn toggle_row_selection(&mut self, index: usize) -> bool {
        let Some(pid) = self.row_pid(index) else {
            return false;
        };
        self.selected = index;
        self.selected_process_row = Some(ProcessRowKey::Process(pid));
        self.toggle_selected_pid(pid);
        self.sync_application_selection();
        true
    }

    pub fn toggle_selected_pid(&mut self, pid: u32) {
        if !self.selected_pids.insert(pid) {
            self.selected_pids.remove(&pid);
        }
        self.selected_process_row = Some(ProcessRowKey::Process(pid));
    }

    pub fn extend_row_selection(&mut self, index: usize) -> bool {
        let rows = self.visible_processes();
        let Some(end) = rows.get(index) else {
            return false;
        };
        let end_pid = end.pid;
        let anchor = rows
            .get(self.selected)
            .map_or(end_pid, |process| process.pid);
        let range = selected_pids_range(&rows, anchor, end_pid);
        drop(rows);
        self.selected_pids.extend(range);
        self.selected = index;
        self.selected_process_row = Some(ProcessRowKey::Process(end_pid));
        self.sync_application_selection();
        true
    }

    pub fn prune_stale_selection(&mut self) {
        let live: std::collections::HashSet<u32> = self
            .data
            .processes
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|process| process.pid)
            .collect();
        self.selected_pids.retain(|pid| live.contains(pid));
        if self.selected_process_row.is_some_and(|row| match row {
            ProcessRowKey::Application(root) | ProcessRowKey::Process(root) => {
                !live.contains(&root)
            }
            ProcessRowKey::Category(_) => false,
        }) {
            self.selected_process_row = None;
        }
    }

    #[must_use]
    pub fn selected_pids(&self) -> &std::collections::HashSet<u32> {
        &self.selected_pids
    }

    pub fn clear_process_selection(&mut self) {
        self.selected_pids.clear();
        self.selected_process_row = None;
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

    fn row_pid(&self, index: usize) -> Option<u32> {
        (self.page() == AppPage::Applications)
            .then(|| self.visible_process_at(index).map(|process| process.pid))
            .flatten()
    }

    pub(super) fn collapse_selection_to_anchor(&mut self) {
        self.selected_pids.clear();
        if let Some(pid) = self.row_pid(self.selected) {
            self.selected_pids.insert(pid);
            self.selected_process_row = Some(ProcessRowKey::Process(pid));
        } else {
            self.selected_process_row = None;
        }
    }

    fn clear_empty_selection(&mut self) {
        self.selected = 0;
        self.selected_pids.clear();
        self.selected_process_row = None;
    }

    fn reset_selection_after_query_change(&mut self) {
        self.selected = 0;
        self.sync_application_selection();
        self.collapse_selection_to_anchor();
    }
}

/// The pid range spanning `anchor` → `end` (inclusive, in the caller's display
/// order). Pure so grouped/tree frontends can extend their multi-select ranges
/// against their OWN visual-row projection without borrowing `&mut self` while
/// the row slice is alive: the caller collects the range, drops the borrow,
/// then inserts into [`super::ShellApp::selected_pids`]. A missing end (stale
/// pid) yields just `end`; a missing anchor degenerates to the single pid.
#[must_use]
pub fn selected_pids_range(rows: &[&ProcessItem], anchor: u32, end: u32) -> Vec<u32> {
    let start = rows.iter().position(|process| process.pid == anchor);
    let end_pos = rows.iter().position(|process| process.pid == end);
    match (start, end_pos) {
        (Some(start), Some(end_pos)) => rows[start.min(end_pos)..=start.max(end_pos)]
            .iter()
            .map(|process| process.pid)
            .collect(),
        _ => vec![end],
    }
}

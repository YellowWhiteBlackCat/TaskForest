//! Multi-select range resolution and set maintenance shared by grouped/tree
//! frontends (ADR-027). Selection state is identity-authoritative (CORE-01):
//! the batch set and the semantic row carry validated live identities
//! (pid + provider start token), so refresh, reorder, disappearance, and pid
//! reuse each have one deterministic outcome. The positional cursor
//! ([`ShellApp::selected`]) is derived state that follows the identity when
//! it resolves.

use std::collections::HashSet;

use taskmanager_application::AppPage;
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessItem, ProcessLiveKey};

use super::ShellApp;
use super::process_rows::{ProcessRowAnchor, ProcessRowId};
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

    /// Resolve one exact live identity against the accepted process snapshot.
    #[must_use]
    pub fn process_by_identity(&self, identity: ProcessLiveKey) -> Option<&ProcessItem> {
        self.data
            .processes_slice()
            .iter()
            .find(|process| ProcessLiveKey::from_process(process) == Some(identity))
    }

    /// Resolve one exact live identity against the visible projection.
    #[must_use]
    pub fn visible_process_by_identity(&self, identity: ProcessLiveKey) -> Option<&ProcessItem> {
        let processes = self.data.processes.as_deref()?;
        self.visible_processes_indices()
            .iter()
            .filter_map(|&raw_index| processes.get(raw_index))
            .find(|process| ProcessLiveKey::from_process(process) == Some(identity))
    }

    /// Find the visible-row position for a stable identity. A PID-reuse
    /// impostor never matches.
    #[must_use]
    pub fn visible_process_index_of_identity(&self, identity: ProcessLiveKey) -> Option<usize> {
        let processes = self.data.processes.as_deref()?;
        self.visible_processes_indices()
            .iter()
            .position(|&raw_index| {
                processes
                    .get(raw_index)
                    .is_some_and(|process| ProcessLiveKey::from_process(process) == Some(identity))
            })
    }

    /// The validated live identity at one visible-row position, when the row
    /// carries a current provider start token.
    #[must_use]
    pub fn row_identity_at(&self, index: usize) -> Option<ProcessLiveKey> {
        self.visible_process_at(index)
            .and_then(ProcessLiveKey::from_process)
    }

    /// The stable semantic row id at one flat visible-row position.
    #[must_use]
    pub fn row_id_at(&self, index: usize) -> Option<ProcessRowId> {
        self.row_identity_at(index).map(ProcessRowId::Process)
    }

    /// The stable row id plus the accepted process projection generation.
    /// Delayed renderer events should carry this value rather than a bare
    /// numeric cursor or PID.
    #[must_use]
    pub fn row_anchor_at(&self, index: usize) -> Option<ProcessRowAnchor> {
        self.row_id_at(index)
            .map(|id| ProcessRowAnchor::new(id, self.data.process_projection_generation()))
    }

    /// The visible-row position of one exact live identity (pid AND start
    /// token); a pid-reuse impostor never matches.
    #[must_use]
    pub fn visible_position_of_identity(&self, identity: ProcessLiveKey) -> Option<usize> {
        self.visible_process_index_of_identity(identity)
    }

    /// Whether one process row is part of the batch set, by exact live
    /// identity.
    #[must_use]
    pub fn is_process_selected(&self, process: &ProcessItem) -> bool {
        ProcessLiveKey::from_process(process)
            .is_some_and(|identity| self.selected_rows.contains(&identity))
    }

    /// The authoritative multi-select identity set (renderers highlight
    /// members; batch intents freeze from this).
    #[must_use]
    pub fn selected_identities(&self) -> &HashSet<ProcessLiveKey> {
        &self.selected_rows
    }

    /// The selected semantic row paired with the current projection
    /// generation. `None` means there is no actionable row selection.
    #[must_use]
    pub fn selected_row_anchor(&self) -> Option<ProcessRowAnchor> {
        self.selected_row
            .map(|id| ProcessRowAnchor::new(id, self.data.process_projection_generation()))
    }

    /// Write a selection a grouped/tree renderer already resolved against its
    /// own visual projection: the semantic primary row and the typed detail
    /// identity move together, and both stay empty outside the Applications
    /// page. This is the AUTHORIZED writer for that pair (ADR-027: the fields
    /// are shell-owned, the projection that produced them is not) — a frontend
    /// must not assign [`ShellApp::selected_row`] or the application state's
    /// `selected_process` directly.
    ///
    /// Unlike [`Self::select_row_id`] this does not resolve the row again and
    /// does not touch the shell's fail-closed `process_selection_invalidated`
    /// flag: the caller is reporting a row it is looking at right now, and an
    /// explicit row outranks that flag in
    /// [`ShellApp::selected_process_identity`]. It also leaves the multi-select
    /// set alone — a grouped arrow step clears it with
    /// [`Self::clear_selected_rows`] and then re-adds the anchor through
    /// [`Self::add_selected_identity`], while a grouped re-sync keeps a live
    /// multi-select.
    pub fn set_row_selection(&mut self, row: Option<ProcessRowId>, process: Option<&ProcessItem>) {
        let applications = self.page() == AppPage::Applications;
        self.selected_row = if applications { row } else { None };
        self.application.selected_process = if applications {
            process.and_then(FrozenProcessIdentity::from_process)
        } else {
            None
        };
    }

    /// Add one validated live identity to the multi-select set without
    /// disturbing its other members, and without re-resolving it against the
    /// projection. The AUTHORIZED writer for a grouped frontend's "this arrow
    /// step landed on exactly this process row" collapse — pair it with
    /// [`Self::clear_selected_rows`] to reproduce a single-select.
    pub fn add_selected_identity(&mut self, identity: ProcessLiveKey) {
        self.selected_rows.insert(identity);
    }

    /// Repoint the positional keyboard anchor without re-deriving the semantic
    /// row from the flat projection. The AUTHORIZED writer for
    /// [`ShellApp::selected`]: grouped and tree frontends index an interleaved
    /// visual list, so their cursor write must not run the flat
    /// [`Self::select_row`] reconciliation behind it.
    pub fn move_selection_to(&mut self, index: usize) {
        self.selected = index;
    }

    /// Select a semantic row that was resolved from the current projection.
    /// Process and application rows must still resolve by their full live
    /// identity; category rows remain structural and never create a process
    /// target.
    #[must_use]
    pub fn select_row_id(&mut self, row: ProcessRowId) -> bool {
        match row {
            ProcessRowId::Category(category) => {
                self.selected_rows.clear();
                self.selected_row = Some(ProcessRowId::Category(category));
                self.application.selected_process = None;
                self.process_selection_invalidated = false;
                true
            }
            ProcessRowId::Application(identity) => {
                if self.page() != AppPage::Applications {
                    return false;
                }
                let Some(index) = self.visible_process_index_of_identity(identity) else {
                    return false;
                };
                self.selected = index;
                self.selected_rows.clear();
                self.selected_row = Some(ProcessRowId::Application(identity));
                self.process_selection_invalidated = false;
                self.application.selected_process = None;
                true
            }
            ProcessRowId::Process(identity) => {
                if self.page() != AppPage::Applications {
                    return false;
                }
                let Some(index) = self.visible_process_index_of_identity(identity) else {
                    return false;
                };
                self.selected = index;
                self.selected_rows.clear();
                self.selected_row = Some(ProcessRowId::Process(identity));
                self.process_selection_invalidated = false;
                self.selected_rows.insert(identity);
                self.application.selected_process = self
                    .process_by_identity(identity)
                    .and_then(FrozenProcessIdentity::from_process);
                true
            }
        }
    }

    /// Select a row anchor emitted by the current projection. A delayed
    /// anchor from an older process revision is rejected before its identity
    /// can affect selection.
    #[must_use]
    pub fn select_row_anchor(&mut self, anchor: ProcessRowAnchor) -> bool {
        if !anchor.belongs_to(self.data.process_projection_generation()) {
            return false;
        }
        self.select_row_id(anchor.id())
    }

    /// Promote the current positional compatibility cursor to a stable row
    /// identity once. This is used only by legacy callers that still assign
    /// `selected`; all new selection paths enter through row ids/anchors.
    pub(super) fn ensure_process_row_at_cursor(&mut self) {
        if self.page() != AppPage::Applications
            || self.selected_row.is_some()
            || self.process_selection_invalidated
        {
            return;
        }
        let Some(identity) = self.row_identity_at(self.selected) else {
            return;
        };
        self.selected_row = Some(ProcessRowId::Process(identity));
        self.selected_rows.clear();
        self.selected_rows.insert(identity);
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
        if self.page() == AppPage::Applications {
            let _ = self.select_row(self.selected);
        } else {
            self.clear_non_process_selection();
        }
    }

    pub fn move_selection_to_first(&mut self) {
        if self.active_row_count() == 0 {
            self.clear_empty_selection();
            return;
        }
        self.selected = 0;
        if self.page() == AppPage::Applications {
            let _ = self.select_row(self.selected);
        } else {
            self.clear_non_process_selection();
        }
    }

    pub fn move_selection_to_last(&mut self) {
        let length = self.active_row_count();
        if length == 0 {
            self.clear_empty_selection();
            return;
        }
        self.selected = length - 1;
        if self.page() == AppPage::Applications {
            let _ = self.select_row(self.selected);
        } else {
            self.clear_non_process_selection();
        }
    }

    #[must_use]
    pub fn select_row(&mut self, index: usize) -> bool {
        if index >= self.active_row_count() {
            return false;
        }
        if self.page() != AppPage::Applications {
            self.selected = index;
            self.clear_non_process_selection();
            return true;
        }
        let Some(anchor) = self.row_anchor_at(index) else {
            return false;
        };
        self.select_row_anchor(anchor)
    }

    #[must_use]
    pub fn toggle_row_selection(&mut self, index: usize) -> bool {
        if self.page() != AppPage::Applications {
            return false;
        }
        let Some(identity) = self.row_identity_at(index) else {
            return false;
        };
        self.selected = index;
        self.selected_row = Some(ProcessRowId::Process(identity));
        self.process_selection_invalidated = false;
        self.toggle_selected_identity(identity);
        self.sync_application_selection();
        true
    }

    pub fn toggle_selected_identity(&mut self, identity: ProcessLiveKey) {
        if !self.selected_rows.insert(identity) {
            self.selected_rows.remove(&identity);
        }
        self.selected_row = Some(ProcessRowId::Process(identity));
        self.process_selection_invalidated = false;
    }

    #[must_use]
    pub fn extend_row_selection(&mut self, index: usize) -> bool {
        if self.page() != AppPage::Applications {
            return false;
        }
        let rows = self.visible_processes();
        let Some(end) = rows
            .get(index)
            .and_then(|row| ProcessLiveKey::from_process(row))
        else {
            return false;
        };
        let anchor = rows
            .get(self.selected)
            .and_then(|row| ProcessLiveKey::from_process(row))
            .unwrap_or(end);
        let range = selected_rows_range(&rows, anchor, end);
        drop(rows);
        self.selected_rows.extend(range);
        self.selected = index;
        self.selected_row = Some(ProcessRowId::Process(end));
        self.process_selection_invalidated = false;
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
        let processes = self.data.processes_slice();
        let live: HashSet<ProcessLiveKey> = processes
            .iter()
            .filter_map(ProcessLiveKey::from_process)
            .collect();
        self.selected_rows
            .retain(|identity| live.contains(identity));
        match self.selected_row {
            Some(ProcessRowId::Application(root) | ProcessRowId::Process(root))
                if live.contains(&root) =>
            {
                self.process_selection_invalidated = false;
                if let Some(position) = self.visible_position_of_identity(root) {
                    self.selected = position;
                }
            }
            Some(ProcessRowId::Application(_) | ProcessRowId::Process(_)) => {
                self.selected_row = None;
                self.process_selection_invalidated = true;
            }
            Some(ProcessRowId::Category(_)) => {
                self.process_selection_invalidated = false;
            }
            None => self.ensure_process_row_at_cursor(),
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
        self.process_selection_invalidated = true;
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
        if self.page() != AppPage::Applications {
            self.clear_non_process_selection();
            return;
        }
        self.selected_rows.clear();
        if let Some(identity) = self.row_identity_at(self.selected) {
            self.selected_rows.insert(identity);
            self.selected_row = Some(ProcessRowId::Process(identity));
            self.process_selection_invalidated = false;
        } else {
            self.selected_row = None;
            self.process_selection_invalidated = true;
        }
    }

    fn clear_empty_selection(&mut self) {
        self.selected = 0;
        self.selected_rows.clear();
        self.selected_row = None;
        self.process_selection_invalidated = true;
    }

    fn clear_non_process_selection(&mut self) {
        self.selected_rows.clear();
        self.selected_row = None;
        self.application.selected_process = None;
        self.process_selection_invalidated = false;
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
    anchor: ProcessLiveKey,
    end: ProcessLiveKey,
) -> Vec<ProcessLiveKey> {
    let identities: Vec<ProcessLiveKey> = rows
        .iter()
        .filter_map(|row| ProcessLiveKey::from_process(row))
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

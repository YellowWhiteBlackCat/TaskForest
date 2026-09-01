//! Shell-owned interactive-state glue on `RootView` (ADR-027 direct track).
//!
//! The authoritative process selection, inventory-table sorts, and typed
//! action feedback all live in [`DirectTrackState`] — reducers owned by the
//! `taskmanager-shell` crate. This module is the ONLY place the GPUI window
//! touches them: typed write methods for the interaction handlers, read
//! accessors the render paths use, and the single outcome→localized-copy fold
//! for the three inventory action bars (`services_feedback` /
//! `startup_feedback` / `session_feedback`). No view field may cache an
//! authority that `shell_state` already owns.

use super::RootView;
use crate::gpui_app::list_view::ActionFeedback;
use taskmanager_core::core::process::{ProcessItem, ProcessLiveKey};
use taskmanager_shell::{
    InfoSortCol, InfoTable, ProcessRowId, ProcessStatusFilter, SortCol, SortDir,
};
use taskmanager_ui::data::table::SortState;

impl RootView {
    // ── process viewing (authority: `self.shell.processes`) ─────────────────
    //
    // The Apps-page sort, status filter, and search query are the same
    // fields `ShellApp` owns on the shell track; the reducers below are the
    // ONLY write path (header clicks, arrow-key column moves, pill clicks,
    // the search box, and the persistence / saved-view restore edge).

    /// The active process-table (column, direction) sort.
    #[must_use]
    pub fn process_sort(&self) -> (SortCol, SortDir) {
        self.shell.processes.sort()
    }

    /// The active Apps status bucket.
    #[must_use]
    pub fn process_status_filter(&self) -> ProcessStatusFilter {
        self.shell.processes.status_filter()
    }

    /// The live process search query (shell-owned; filtered with the shared
    /// `matches_process_query` grammar).
    #[must_use]
    pub fn process_query(&self) -> &str {
        self.shell.processes.query()
    }

    /// Header-click sort change: same-column flip, new column with the
    /// conventional initial direction (see `ProcessViewing::click_sort_column`).
    pub fn click_process_sort(&mut self, column: SortCol) {
        self.shell.processes.click_sort_column(column);
    }

    /// Header ArrowLeft/ArrowRight: move the active column, keep direction.
    pub fn move_process_sort_column(&mut self, column: SortCol) {
        self.shell.processes.move_sort_column(column);
    }

    /// ABSOLUTE (column, direction) restore — the persistence and saved-view
    /// apply edge. Interactive paths use the click/move reducers above.
    pub fn set_process_sort(&mut self, column: SortCol, direction: SortDir) {
        self.shell.processes.set_sort(column, direction);
    }

    /// Select the Apps status bucket (segmented pill click).
    pub fn set_process_status_filter(&mut self, filter: ProcessStatusFilter) {
        self.shell.processes.set_status_filter(filter);
    }

    /// Replace the whole process search query (the search box reports
    /// absolute text).
    pub fn set_process_query(&mut self, query: &str) {
        self.shell.processes.set_query(query);
    }

    // ── process selection (authority: `self.shell.selection`) ───────────────

    #[must_use]
    pub fn selected_process_identity(&self) -> Option<ProcessLiveKey> {
        self.shell.selection.anchor()
    }

    /// Semantic active row. Application aggregates are represented by their
    /// own root identity and therefore never masquerade as a selected PID.
    #[must_use]
    pub fn selected_process_row(&self) -> Option<ProcessRowId> {
        self.shell.selection.active_row()
    }

    #[must_use]
    pub fn selected_application_root(&self) -> Option<ProcessLiveKey> {
        self.shell.selection.application_root()
    }

    /// The authoritative multi-select identity set (row highlighting).
    #[must_use]
    pub fn selected_process_identities(&self) -> &std::collections::HashSet<ProcessLiveKey> {
        self.shell.selection.rows()
    }

    /// How many rows the multi-select set holds (batch-verb enablement).
    #[must_use]
    pub fn selected_process_count(&self) -> usize {
        self.shell.selection.rows().len()
    }

    /// Shared target/availability projection for every process action surface.
    /// GPUI only renders this result; it never reconstructs a control scope
    /// from a PID, label, or local menu state.
    #[must_use]
    pub fn process_control_availability(&self) -> taskmanager_shell::ProcessControlAvailability {
        self.shell.process_control_availability()
    }

    /// Whether one process row is part of the multi-select set, by exact
    /// live identity.
    #[must_use]
    pub fn is_process_selected(&self, process: &ProcessItem) -> bool {
        ProcessLiveKey::from_process(process)
            .is_some_and(|identity| self.shell.selection.contains(identity))
    }

    /// Plain click / context-menu focus: collapse to exactly one exact live
    /// row identity. Callers must resolve the identity from the rendered row.
    pub fn select_process_single(&mut self, identity: ProcessLiveKey) {
        if self
            .processes()
            .iter()
            .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
        {
            self.shell.selection.select_single(identity);
        }
    }

    pub fn select_application_root(&mut self, identity: ProcessLiveKey) {
        if self
            .processes()
            .iter()
            .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
        {
            self.shell.selection.select_application(identity);
        }
    }

    /// Ctrl-click toggle of one exact live row (anchor follows membership).
    pub fn toggle_process_selection(&mut self, identity: ProcessLiveKey) {
        if self
            .processes()
            .iter()
            .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
        {
            self.shell.selection.toggle(identity);
        }
    }

    /// Shift-click range grow against the live display-order projection.
    pub fn extend_process_selection(&mut self, display: &[ProcessLiveKey], end: ProcessLiveKey) {
        if self
            .processes()
            .iter()
            .any(|process| ProcessLiveKey::from_process(process) == Some(end))
        {
            self.shell.selection.extend_range(display, end);
        }
    }

    /// Keyboard navigation move (collapse unless `preserve_set`).
    pub fn move_process_selection(&mut self, identity: Option<ProcessLiveKey>, preserve_set: bool) {
        self.shell.selection.move_to(identity, preserve_set);
    }

    pub fn move_process_row_selection(&mut self, row: Option<ProcessRowId>, preserve_set: bool) {
        self.shell.selection.move_to_row(row, preserve_set);
    }

    /// Replace the whole selection in one step (capture fixtures and
    /// batch-intent reconciliation). The caller supplies exact live keys.
    pub fn replace_process_selection(
        &mut self,
        identities: impl IntoIterator<Item = ProcessLiveKey>,
        anchor: Option<ProcessLiveKey>,
    ) {
        self.shell.selection.replace(identities, anchor);
    }

    /// Clear the multi-select set and the anchor.
    pub fn clear_process_selection(&mut self) {
        self.shell.selection.clear();
    }

    // ── inventory sorts (authority: `self.shell.sorts`) ─────────────────────

    /// The active Services/Startup/Users sort (`None` = provider order).
    #[must_use]
    pub fn inventory_sort(&self, table: InfoTable) -> Option<(InfoSortCol, SortDir)> {
        self.shell.sorts.active(table)
    }

    /// Apply one table-header sort change from the shared table widget: the
    /// widget's three-state cycle reports the ABSOLUTE post-click state, which
    /// is stored verbatim so the painted indicator and this authority can
    /// never disagree. `None` (unsortable column) is ignored.
    pub fn apply_table_sort(
        &mut self,
        table: InfoTable,
        column: Option<InfoSortCol>,
        sort: SortState,
    ) {
        let Some(column) = column else {
            return;
        };
        let applied = match sort {
            SortState::Unsorted => None,
            SortState::Ascending => Some((column, SortDir::Asc)),
            SortState::Descending => Some((column, SortDir::Desc)),
        };
        self.shell.sorts.set(table, applied);
    }

    // ── typed feedback → action-bar copy (authority: `self.shell.feedback`) ─

    /// Fold the latest typed Services control outcome into the action-bar
    /// status line. Pure read: the typed outcome stays authoritative in the
    /// shell slot; only the localized copy is derived per render.
    #[must_use]
    pub fn services_feedback(&self) -> Option<ActionFeedback> {
        let outcome = self.shell.feedback.service()?;
        let display_name = self
            .services()
            .iter()
            .find(|service| service.id == outcome.service_id)
            .map_or_else(
                || outcome.service_id.as_str().to_string(),
                |service| service.name.clone(),
            );
        Some(super::services::service_action_feedback(
            outcome.result,
            outcome.action,
            &display_name,
        ))
    }

    /// Fold the latest typed Startup control outcome into the action-bar
    /// status line (see [`Self::services_feedback`]).
    #[must_use]
    pub fn startup_feedback(&self) -> Option<ActionFeedback> {
        let outcome = self.shell.feedback.startup()?;
        Some(super::platform_lists::startup_outcome_feedback(
            outcome.result,
            outcome.enabled,
            &outcome.target_name,
        ))
    }

    /// Fold the latest typed login-session control outcome into the action-bar
    /// status line (see [`Self::services_feedback`]).
    #[must_use]
    pub fn session_feedback(&self) -> Option<ActionFeedback> {
        let outcome = self.shell.feedback.session()?;
        Some(super::platform_lists::session_outcome_feedback(
            outcome.result,
            outcome.action,
            outcome.session_id.as_str(),
        ))
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_shell_state_tests.rs"]
mod tests;

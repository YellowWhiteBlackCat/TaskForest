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

use std::collections::HashSet;

use super::RootView;
use crate::gpui_app::list_view::ActionFeedback;
use taskmanager_shell::{
    InfoSortCol, InfoTable, ProcessRowKey, ProcessStatusFilter, SortCol, SortDir,
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

    /// The anchor pid (keyboard / plain-click focus; the single-select
    /// identity). Batch intents fall back to it when the set is empty.
    #[must_use]
    pub fn selected_pid(&self) -> Option<u32> {
        self.shell.selection.anchor()
    }

    /// Semantic active row. Application aggregates are represented by their
    /// own root key and therefore never masquerade as a selected PID.
    #[must_use]
    pub fn selected_process_row(&self) -> Option<ProcessRowKey> {
        self.shell.selection.active_row()
    }

    #[must_use]
    pub fn selected_application_root(&self) -> Option<u32> {
        self.shell.selection.application_root()
    }

    /// The authoritative multi-select set (renderers highlight members).
    #[must_use]
    pub fn selected_pids(&self) -> &HashSet<u32> {
        self.shell.selection.pids()
    }

    /// Plain click / context-menu focus: collapse to exactly one pid.
    pub fn select_process_single(&mut self, pid: u32) {
        self.shell.selection.select_single(pid);
    }

    pub fn select_application_root(&mut self, root_pid: u32) {
        self.shell.selection.select_application(root_pid);
    }

    /// Ctrl-click toggle of one pid (anchor follows membership).
    pub fn toggle_process_selection(&mut self, pid: u32) {
        self.shell.selection.toggle(pid);
    }

    /// Shift-click range grow against the live display-order pid projection.
    pub fn extend_process_selection(&mut self, display_pids: &[u32], end: u32) {
        self.shell.selection.extend_range(display_pids, end);
    }

    /// Keyboard navigation move (collapse unless `preserve_set`).
    pub fn move_process_selection(&mut self, pid: Option<u32>, preserve_set: bool) {
        self.shell.selection.move_to(pid, preserve_set);
    }

    pub fn move_process_row_selection(&mut self, row: Option<ProcessRowKey>, preserve_set: bool) {
        self.shell.selection.move_to_row(row, preserve_set);
    }

    /// Replace the whole selection in one step (capture fixtures, batch-intent
    /// reconciliation).
    pub fn replace_process_selection(
        &mut self,
        pids: impl IntoIterator<Item = u32>,
        anchor: Option<u32>,
    ) {
        self.shell.selection.replace(pids, anchor);
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

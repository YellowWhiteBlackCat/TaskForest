//! The bulk search-input vocabulary for bracketed paste (2026-08-17 input
//! line). Single-character typing lives on `ShellApp::push_search_char` in
//! `app.rs`; the PASTE path is bounded and sanitized here so every frontend
//! shares one semantics: a terminal paste can carry newlines, tabs, and
//! control bytes, but the search box is a single-line field — line breaks
//! collapse to single spaces, other control bytes drop, and the query stays
//! bounded so one paste can never flood the filter.

use super::ShellApp;
use super::input_mode::SearchSlot;

/// The bounded search-query length a bracketed paste may grow the query to
/// (keyboard typing is naturally bounded; paste is the first bulk path).
pub const SEARCH_QUERY_MAX: usize = 256;

impl ShellApp {
    /// Bulk search-input path for bracketed paste. The cursor/multi-set
    /// reset mirrors `Self::push_search_char`; returns false when nothing
    /// changed (empty or fully-sanitized paste, or the query already at the
    /// cap).
    pub fn push_search_text(&mut self, text: &str) -> bool {
        let current = match self.search_slot() {
            SearchSlot::Applications => &self.query,
            SearchSlot::Services => &self.services_query,
        };
        let room = SEARCH_QUERY_MAX.saturating_sub(current.chars().count());
        if room == 0 {
            return false;
        }
        let budgeted: String = text.chars().take(room).collect();
        // The shared single-line paste path collapses line breaks/tabs into
        // one space, drops other control bytes, and replaces any selection.
        let changed = {
            let (query, edit) = self.active_search_mut();
            edit.insert_str(query, &budgeted, true)
        };
        if changed {
            self.reset_selection_after_query_change();
        }
        changed
    }

    /// Replace the Applications query from a frontend's own text widget.
    /// The widget owns its `TextInput` buffer; this is the shell-side fact.
    /// A real change applies the same cursor hygiene as a typed edit.
    pub fn set_query(&mut self, query: impl Into<String>) {
        let query = bounded_query(query);
        if self.query == query {
            return;
        }
        self.query = query;
        self.query_edit.set_caret_at_end(&self.query);
        self.reset_selection_for_search_slot(SearchSlot::Applications);
    }

    /// Replace the Services query from a frontend's own text widget.
    pub fn set_services_query(&mut self, query: impl Into<String>) {
        let query = bounded_query(query);
        if self.services_query == query {
            return;
        }
        self.services_query = query;
        self.services_query_edit
            .set_caret_at_end(&self.services_query);
        self.reset_selection_for_search_slot(SearchSlot::Services);
    }
}

fn bounded_query(query: impl Into<String>) -> String {
    query.into().chars().take(SEARCH_QUERY_MAX).collect()
}

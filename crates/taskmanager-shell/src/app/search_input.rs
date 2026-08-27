//! The bulk search-input vocabulary for bracketed paste (2026-08-17 input
//! line). Single-character typing lives on `ShellApp::push_search_char` in
//! `app.rs`; the PASTE path is bounded and sanitized here so every frontend
//! shares one semantics: a terminal paste can carry newlines, tabs, and
//! control bytes, but the search box is a single-line field — line breaks
//! collapse to single spaces, other control bytes drop, and the query stays
//! bounded so one paste can never flood the filter.

use super::ShellApp;

/// The bounded search-query length a bracketed paste may grow the query to
/// (keyboard typing is naturally bounded; paste is the first bulk path).
pub const SEARCH_QUERY_MAX: usize = 256;

impl ShellApp {
    /// Bulk search-input path for bracketed paste. The cursor/multi-set
    /// reset mirrors `Self::push_search_char`; returns false when nothing
    /// changed (empty or fully-sanitized paste, or the query already at the
    /// cap).
    pub fn push_search_text(&mut self, text: &str) -> bool {
        let before = self.query.clone();
        let mut changed = false;
        for character in text.chars() {
            if self.query.chars().count() >= SEARCH_QUERY_MAX {
                break;
            }
            match character {
                '\n' | '\r' | '\t' => {
                    // A pasted block flattened into one line; collapse runs
                    // of line breaks into a single space, never doubles.
                    if !self.query.ends_with(' ') {
                        self.query.push(' ');
                    }
                    changed = true;
                }
                character if character.is_control() => {}
                character => {
                    self.query.push(character);
                    changed = true;
                }
            }
        }
        if changed {
            self.selected = 0;
            self.sync_application_selection();
            self.collapse_selection_to_anchor();
        }
        self.query != before
    }
}

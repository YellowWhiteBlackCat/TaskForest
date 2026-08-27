//! Search-match highlighter (absorption §6.3-F; gc's tree-sitter syntax
//! highlighter is out of scope — this is the single-pattern, character-level
//! match engine TaskManager needs for search highlighting).
//!
//! Pure functions + a small stateful wrapper. Matches are byte ranges into
//! the searched text (gpui offset convention), always on UTF-8 character
//! boundaries; empty queries match nothing.

use std::ops::Range;

/// One match of the query inside the searched text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    /// The byte range of the match (UTF-8 character boundaries).
    pub range: Range<usize>,
}

/// Find all **non-overlapping** matches of `query` in `text` (byte ranges).
///
/// Character-level comparison: multi-byte characters compare as whole
/// characters and ranges never split a character. With `case_sensitive =
/// false` each character compares by lowercase value (full Unicode
/// lowercase, so e.g. `Ä` matches `ä`).
#[must_use]
pub fn find_matches(text: &str, query: &str, case_sensitive: bool) -> Vec<SearchMatch> {
    if query.is_empty() || text.is_empty() {
        return Vec::new();
    }
    let query_chars: Vec<char> = query.chars().collect();
    let mut matches = Vec::new();
    let mut cursor = 0usize;
    while cursor < text.len() {
        let start = cursor;
        let mut end = cursor;
        let mut matched = true;
        for &query_char in &query_chars {
            let Some(ch) = text[end..].chars().next() else {
                matched = false;
                break;
            };
            let equal = if case_sensitive {
                ch == query_char
            } else {
                ch.to_lowercase().eq(query_char.to_lowercase())
            };
            if !equal {
                matched = false;
                break;
            }
            end += ch.len_utf8();
        }
        if matched {
            matches.push(SearchMatch { range: start..end });
            // Non-overlapping: continue after this match.
            cursor = end;
        } else {
            cursor = start + text[start..].chars().next().map_or(1, char::len_utf8);
        }
    }
    matches
}

/// The first match at or after `offset` (bytes; used to scroll a match into
/// view when cycling search results).
#[must_use]
pub fn first_match_at_or_after(matches: &[SearchMatch], offset: usize) -> Option<SearchMatch> {
    matches
        .iter()
        .find(|m| m.range.end > offset)
        .or_else(|| matches.first())
        .cloned()
}

/// A cached search highlighter: re-computes matches only when the query or
/// the text changed.
#[derive(Clone, Debug, Default)]
pub struct Highlighter {
    query: String,
    case_sensitive: bool,
    last_text: String,
    matches: Vec<SearchMatch>,
}

impl Highlighter {
    /// Create a highlighter for `query` (case-sensitive matching).
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            case_sensitive: true,
            last_text: String::new(),
            matches: Vec::new(),
        }
    }

    /// Compare case-insensitively instead (default: case-sensitive).
    #[must_use]
    pub fn case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }

    /// Rebuild matches for `text` (no-op when neither query nor text
    /// changed).
    pub fn update(&mut self, text: &str) {
        if self.last_text == text {
            return;
        }
        self.last_text = text.to_owned();
        self.matches = find_matches(text, &self.query, self.case_sensitive);
    }

    /// Replace the query and rebuild matches for the last text.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        let text = std::mem::take(&mut self.last_text);
        self.last_text = text.clone();
        self.matches = find_matches(&text, &self.query, self.case_sensitive);
    }

    /// The current query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// All matches (byte ranges).
    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    /// The first match, if any (首匹配定位: scroll targets use this).
    pub fn first_match(&self) -> Option<SearchMatch> {
        self.matches.first().cloned()
    }

    /// The number of matches.
    pub fn count(&self) -> usize {
        self.matches.len()
    }

    /// Whether there are no matches.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// The next match at or after `offset`, wrapping (cycling search).
    pub fn match_at_or_after(&self, offset: usize) -> Option<SearchMatch> {
        first_match_at_or_after(&self.matches, offset)
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_data_highlighter_tests.rs"]
mod tests;

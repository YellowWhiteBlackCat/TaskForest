//! Search-match highlighting for the process table.
//!
//! Match geometry comes exclusively from
//! `taskmanager_application::text::match_ranges_ascii_ci` (ADR-020) — this
//! module only splits a name into plain/highlighted segments around those
//! byte ranges. The matcher guarantees every range falls on a UTF-8
//! character boundary, so byte slicing (`&text[range]`) is safe for any
//! text, including non-ASCII names.

use taskmanager_application::text::match_ranges_ascii_ci;

/// Split `text` into display segments, marking which ones matched `query`.
///
/// The query is trimmed before matching; an empty or blank query (or a query
/// that matches nothing) yields the whole text as a single non-matching
/// segment. The returned segments concatenate back to exactly `text`.
pub fn highlight_segments(text: &str, query: &str) -> Vec<(String, bool)> {
    let needle = query.trim();
    if needle.is_empty() {
        return vec![(text.to_string(), false)];
    }
    let ranges = match_ranges_ascii_ci(text, needle);
    if ranges.is_empty() {
        return vec![(text.to_string(), false)];
    }
    let mut segments = Vec::with_capacity(ranges.len() * 2 + 1);
    let mut cursor = 0;
    for range in ranges {
        if cursor < range.start {
            segments.push((text[cursor..range.start].to_string(), false));
        }
        segments.push((text[range.start..range.end].to_string(), true));
        cursor = range.end;
    }
    if cursor < text.len() {
        segments.push((text[cursor..].to_string(), false));
    }
    segments
}

#[cfg(test)]
#[path = "../../tests/gui/ui/highlight_tests.rs"]
mod tests;

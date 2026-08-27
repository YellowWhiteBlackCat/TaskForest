//! Shared text matching utilities for the display layer.
//!
//! Process/service/startup search all match by case-insensitive substring;
//! before this module each call site lowered the whole haystack per item
//! (`to_ascii_lowercase` allocates once per process per frame). These helpers
//! fold ASCII case on the fly over bytes, which is behavior-identical for
//! ASCII needles (Unicode lowercasing never rewrites ASCII bytes except
//! exotic special-case folds like U+FB00), and they never allocate.

/// ASCII case-insensitive substring match over UTF-8 bytes. A valid UTF-8
/// needle's lead bytes never appear inside another code point's continuation
/// sequence, so window alignment over the haystack cannot split a character.
#[must_use]
pub fn contains_ascii_ci(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.as_bytes().windows(needle.len()).any(|window| {
        window
            .iter()
            .map(|byte| byte.to_ascii_lowercase())
            .eq(needle.bytes().map(|byte| byte.to_ascii_lowercase()))
    })
}

/// Every non-overlapping ASCII case-insensitive match of `needle` in
/// `haystack`, as byte ranges into the haystack. This is the SINGLE source of
/// search-match geometry: each frontend (GPUI, TUI, iced) renders these
/// ranges with its own text primitives and never recomputes matches itself
/// (ADR-020). Empty needles yield no ranges (an empty search highlights
/// nothing).
#[must_use]
pub fn match_ranges_ascii_ci(haystack: &str, needle: &str) -> Vec<std::ops::Range<usize>> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    let hay = haystack.as_bytes();
    let needle_len = needle.len();
    let mut ranges = Vec::new();
    let mut cursor = 0;
    while cursor + needle_len <= hay.len() {
        let window = &hay[cursor..cursor + needle_len];
        let matches = window
            .iter()
            .map(|byte| byte.to_ascii_lowercase())
            .eq(needle.bytes().map(|byte| byte.to_ascii_lowercase()));
        if matches {
            ranges.push(cursor..cursor + needle_len);
            cursor += needle_len;
        } else {
            cursor += 1;
        }
    }
    ranges
}

/// ASCII case-insensitive byte ordering for sort comparators. Equivalent to
/// `a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase())` without allocating
/// either lowercase copy.
#[must_use]
pub fn cmp_ascii_ci(a: &str, b: &str) -> std::cmp::Ordering {
    a.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_text_tests.rs"]
mod tests;

use std::ops::Range;

use super::{Highlighter, SearchMatch, find_matches, first_match_at_or_after};

fn ranges(matches: &[SearchMatch]) -> Vec<Range<usize>> {
    matches.iter().map(|m| m.range.clone()).collect()
}

#[test]
fn case_sensitive_matches_exact_only() {
    assert_eq!(
        ranges(&find_matches("Hello hello HELLO", "hello", true)),
        vec![6..11]
    );
    assert_eq!(
        ranges(&find_matches("hello hello", "hello", true)),
        vec![0..5, 6..11]
    );
}

#[test]
fn case_insensitive_matches_all_variants() {
    assert_eq!(
        ranges(&find_matches("Hello hello HELLO", "hello", false)),
        vec![0..5, 6..11, 12..17]
    );
    // Full Unicode lowercase: Ä matches ä. Ranges are byte-based, so
    // each two-byte "Äpfel" word spans 6 bytes (chars would be 5).
    assert_eq!(
        ranges(&find_matches("Äpfel äpfel", "äpfel", false)),
        vec![0..6, 7..13]
    );
    assert_eq!(
        ranges(&find_matches("Äpfel äpfel", "äpfel", true)),
        vec![7..13]
    );
}

#[test]
fn matches_are_non_overlapping() {
    // Leftmost-longest non-overlapping: "aa" matched once.
    assert_eq!(ranges(&find_matches("aaa", "aa", true)), vec![0..2]);
    assert_eq!(ranges(&find_matches("aaaa", "aa", true)), vec![0..2, 2..4]);
}

#[test]
fn multibyte_ranges_never_split_characters() {
    // "中文搜索中文" — bytes per char are 3.
    let text = "中文搜索中文";
    let matches = find_matches(text, "中文", true);
    assert_eq!(ranges(&matches), vec![0..6, 12..18]);
    // The match boundaries are valid char boundaries.
    for m in &matches {
        assert!(text.is_char_boundary(m.range.start));
        assert!(text.is_char_boundary(m.range.end));
    }
}

#[test]
fn empty_query_or_text_matches_nothing() {
    assert!(find_matches("abc", "", true).is_empty());
    assert!(find_matches("", "a", true).is_empty());
    assert!(find_matches("", "", true).is_empty());
}

#[test]
fn no_match_returns_empty() {
    assert!(find_matches("abcdef", "xyz", true).is_empty());
    assert!(find_matches("abc", "abcd", true).is_empty());
}

#[test]
fn first_match_at_or_after_wraps() {
    let matches = find_matches("a b a", "a", true);
    assert_eq!(matches.len(), 2);
    assert_eq!(
        first_match_at_or_after(&matches, 0),
        Some(matches[0].clone())
    );
    // Offset inside the first match: next match.
    assert_eq!(
        first_match_at_or_after(&matches, 1),
        Some(matches[1].clone())
    );
    // Offset past everything: wraps to the first match.
    assert_eq!(
        first_match_at_or_after(&matches, 10),
        Some(matches[0].clone())
    );
    assert_eq!(first_match_at_or_after(&[], 0), None);
}

#[test]
fn highlighter_caches_and_updates() {
    let mut h = Highlighter::new("foo").case_sensitive(false);
    h.update("FOO foo");
    assert_eq!(h.count(), 2);
    assert_eq!(h.first_match().unwrap().range, 0..3);
    assert_eq!(h.query(), "foo");
    assert!(!h.is_empty());

    // Same text again: matches stay (cache).
    h.update("FOO foo");
    assert_eq!(h.count(), 2);

    // Text change rebuilds.
    h.update("nothing here");
    assert_eq!(h.count(), 0);
    assert!(h.is_empty());
    assert_eq!(h.first_match(), None);

    // Query change rebuilds against the last text.
    h.set_query("here");
    assert_eq!(h.count(), 1);
    assert_eq!(h.match_at_or_after(0).unwrap().range, 8..12);
}

use super::{cmp_ascii_ci, contains_ascii_ci, match_ranges_ascii_ci};

#[test]
fn contains_matches_case_insensitively() {
    assert!(contains_ascii_ci("Firefox", "fire"));
    assert!(contains_ascii_ci("firefox", "FIRE"));
    assert!(contains_ascii_ci("systemd-resolved", "SYSTEMD"));
    assert!(!contains_ascii_ci("systemd", "systemd-resolved"));
}

#[test]
fn contains_empty_needle_matches_everything() {
    assert!(contains_ascii_ci("anything", ""));
}

#[test]
fn contains_multibyte_haystack_keeps_ascii_semantics() {
    // Unicode bytes must not false-match across code-point boundaries.
    assert!(contains_ascii_ci("Réseau", "seau"));
    assert!(!contains_ascii_ci("Réseau", "seau!"));
}

#[test]
fn cmp_orders_ascii_case_insensitively_without_allocation() {
    assert_eq!(cmp_ascii_ci("Alpha", "beta"), std::cmp::Ordering::Less);
    assert_eq!(cmp_ascii_ci("ALPHA", "alpha"), std::cmp::Ordering::Equal);
    assert_eq!(cmp_ascii_ci("beta", "Alpha"), std::cmp::Ordering::Greater);
}

#[test]
fn match_ranges_covers_every_case_insensitive_occurrence() {
    assert_eq!(match_ranges_ascii_ci("Firefox", "fire"), vec![0..4]);
    assert_eq!(match_ranges_ascii_ci("firefox", "FIRE"), vec![0..4]);
    // Non-overlapping occurrences, case-insensitively.
    assert_eq!(
        match_ranges_ascii_ci("abcABCabc", "abc"),
        vec![0..3, 3..6, 6..9]
    );
    // Overlapping occurrences consume the matched window (no overlap).
    assert_eq!(match_ranges_ascii_ci("aaaa", "aa"), vec![0..2, 2..4]);
    // A missing needle leaves the haystack untouched.
    assert_eq!(
        match_ranges_ascii_ci("systemd-resolved", "plasma"),
        Vec::<std::ops::Range<usize>>::new()
    );
    // Unicode bytes never false-match across code-point boundaries and
    // ranges stay byte-aligned with the haystack (é occupies two bytes,
    // so "seau" starts at byte 3).
    assert_eq!(match_ranges_ascii_ci("Réseau", "seau"), vec![3..7]);
}

#[test]
fn match_ranges_empty_or_too_long_needle_highlights_nothing() {
    assert_eq!(
        match_ranges_ascii_ci("anything", ""),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert_eq!(
        match_ranges_ascii_ci("short", "much longer needle"),
        Vec::<std::ops::Range<usize>>::new()
    );
}

#[test]
fn match_ranges_agree_with_contains() {
    for (haystack, needle) in [
        ("Firefox", "fire"),
        ("systemd-resolved", "SYSTEMD"),
        ("Réseau", "seau"),
        ("alpha", "beta"),
    ] {
        assert_eq!(
            contains_ascii_ci(haystack, needle),
            !match_ranges_ascii_ci(haystack, needle).is_empty(),
            "contains and match_ranges must agree for ({haystack:?}, {needle:?})"
        );
    }
}

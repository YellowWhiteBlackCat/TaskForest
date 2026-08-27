use super::highlight_segments;

#[test]
fn empty_and_blank_queries_yield_one_unmatched_segment() {
    assert_eq!(
        highlight_segments("Firefox", ""),
        vec![("Firefox".to_string(), false)]
    );
    assert_eq!(
        highlight_segments("Firefox", "   "),
        vec![("Firefox".to_string(), false)]
    );
}

#[test]
fn query_is_trimmed_before_matching() {
    assert_eq!(
        highlight_segments("Firefox", "  fire  "),
        vec![("Fire".to_string(), true), ("fox".to_string(), false)]
    );
}

#[test]
fn matching_is_ascii_case_insensitive() {
    assert_eq!(
        highlight_segments("Firefox", "FIRE"),
        vec![("Fire".to_string(), true), ("fox".to_string(), false)]
    );
    assert_eq!(
        highlight_segments("gnome-shell", "SHELL"),
        vec![("gnome-".to_string(), false), ("shell".to_string(), true),]
    );
}

#[test]
fn multiple_non_overlapping_matches_split_into_alternating_segments() {
    assert_eq!(
        highlight_segments("abXabcYZabc", "abc"),
        vec![
            ("abX".to_string(), false),
            ("abc".to_string(), true),
            ("YZ".to_string(), false),
            ("abc".to_string(), true),
        ]
    );
    assert_eq!(
        highlight_segments("aaaa", "aa"),
        vec![("aa".to_string(), true), ("aa".to_string(), true)]
    );
}

#[test]
fn no_match_yields_the_whole_text_unhighlighted() {
    assert_eq!(
        highlight_segments("systemd-resolved", "plasma"),
        vec![("systemd-resolved".to_string(), false)]
    );
    assert_eq!(
        highlight_segments("short", "much longer needle"),
        vec![("short".to_string(), false)]
    );
}

#[test]
fn leading_and_trailing_text_keeps_boundaries_exact() {
    assert_eq!(
        highlight_segments("abcFirefox", "firefox"),
        vec![("abc".to_string(), false), ("Firefox".to_string(), true)]
    );
    assert_eq!(
        highlight_segments("FirefoxEnd", "firefox"),
        vec![("Firefox".to_string(), true), ("End".to_string(), false)]
    );
}

#[test]
fn non_ascii_text_slices_on_utf8_boundaries_without_panicking() {
    assert_eq!(
        highlight_segments("中国系统进程", "系统"),
        vec![
            ("中国".to_string(), false),
            ("系统".to_string(), true),
            ("进程".to_string(), false),
        ]
    );
    assert_eq!(
        highlight_segments("你好世界", "xyz"),
        vec![("你好世界".to_string(), false)]
    );
}

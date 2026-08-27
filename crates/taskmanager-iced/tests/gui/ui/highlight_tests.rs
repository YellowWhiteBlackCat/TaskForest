use super::highlight_segments;

#[test]
fn empty_or_whitespace_query_yields_one_plain_segment() {
    assert_eq!(
        highlight_segments("Firefox", ""),
        vec![("Firefox".to_string(), false)]
    );
    assert_eq!(
        highlight_segments("Firefox", "   "),
        vec![("Firefox".to_string(), false)]
    );
    assert_eq!(
        highlight_segments("", "fire"),
        vec![("".to_string(), false)]
    );
}

#[test]
fn matching_is_ascii_case_insensitive() {
    assert_eq!(
        highlight_segments("Firefox", "fire"),
        vec![("Fire".to_string(), true), ("fox".to_string(), false)]
    );
    assert_eq!(
        highlight_segments("firefox", "FIRE"),
        vec![("fire".to_string(), true), ("fox".to_string(), false)]
    );
    assert_eq!(
        highlight_segments("systemd-resolved", "SYSTEMD"),
        vec![
            ("systemd".to_string(), true),
            ("-resolved".to_string(), false)
        ]
    );
}

#[test]
fn multiple_non_overlapping_matches_split_into_alternating_segments() {
    assert_eq!(
        highlight_segments("abcABCabc", "abc"),
        vec![
            ("abc".to_string(), true),
            ("ABC".to_string(), true),
            ("abc".to_string(), true),
        ]
    );
    assert_eq!(
        highlight_segments("aaaa", "aa"),
        vec![("aa".to_string(), true), ("aa".to_string(), true)]
    );
    assert_eq!(
        highlight_segments("rust-analyzer", "a"),
        vec![
            ("rust-".to_string(), false),
            ("a".to_string(), true),
            ("n".to_string(), false),
            ("a".to_string(), true),
            ("lyzer".to_string(), false),
        ]
    );
}

#[test]
fn no_match_yields_the_whole_text_as_one_plain_segment() {
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
fn multibyte_text_slices_only_on_code_point_boundaries() {
    // é occupies two bytes, so "seau" matches at byte range 3..7; slicing
    // must keep the code point intact and never panic.
    assert_eq!(
        highlight_segments("Réseau", "seau"),
        vec![("Ré".to_string(), false), ("seau".to_string(), true)]
    );
    // A non-ASCII needle matches byte-exactly (UTF-8 lead bytes never
    // align with continuation bytes), and ranges stay on code points.
    assert_eq!(
        highlight_segments("进程管理器", "进程"),
        vec![("进程".to_string(), true), ("管理器".to_string(), false)]
    );
    assert_eq!(
        highlight_segments("进程管理器", "管理"),
        vec![
            ("进程".to_string(), false),
            ("管理".to_string(), true),
            ("器".to_string(), false),
        ]
    );
}

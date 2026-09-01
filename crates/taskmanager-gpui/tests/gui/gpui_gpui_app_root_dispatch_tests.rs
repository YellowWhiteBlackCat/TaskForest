use taskmanager_shell::presentation::{search_url_for, url_encode_query};

/// GPUI owns no percent-encoding of its own: the "Search online" URL is the
/// shell's single-source builder. These receipts pin the bytes this surface
/// hands to the platform's URL-open port.
#[test]
fn unreserved_characters_pass_through_unchanged() {
    assert_eq!(
        url_encode_query("AZaz09-._~"),
        "AZaz09-._~",
        "RFC 3986 unreserved set must not be escaped"
    );
}

#[test]
fn spaces_and_query_separators_are_escaped() {
    assert_eq!(url_encode_query("a b"), "a%20b");
    assert_eq!(url_encode_query("q=a&b"), "q%3Da%26b");
    assert_eq!(url_encode_query("?"), "%3F");
    assert_eq!(url_encode_query("#"), "%23");
    assert_eq!(url_encode_query("%"), "%25");
}

#[test]
fn utf8_bytes_are_escaped_individually_with_uppercase_hex() {
    // "进程" in UTF-8: E8 BF 9B E7 A8 8B
    assert_eq!(url_encode_query("进程"), "%E8%BF%9B%E7%A8%8B");
}

#[test]
fn empty_and_ascii_control_inputs_are_harmless() {
    assert_eq!(url_encode_query(""), "");
    assert_eq!(url_encode_query("\n"), "%0A");
    assert_eq!(url_encode_query("\t"), "%09");
}

#[test]
fn the_search_online_url_is_the_shared_single_source_url() {
    assert_eq!(
        search_url_for("Media Player"),
        "https://www.google.com/search?q=Media%20Player",
        "the GPUI search action must open exactly the shared search URL"
    );
}

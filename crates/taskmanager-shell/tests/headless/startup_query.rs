use super::*;

#[test]
fn startup_query_matches_name_and_command_with_ascii_case_folding() {
    assert!(matches_startup_text(
        "Clipboard Sync",
        "wl-paste --watch",
        ""
    ));
    assert!(matches_startup_text(
        "Clipboard Sync",
        "wl-paste --watch",
        "   "
    ));
    assert!(matches_startup_text(
        "Clipboard Sync",
        "wl-paste --watch",
        "clipboard"
    ));
    assert!(matches_startup_text(
        "Clipboard Sync",
        "wl-paste --watch",
        "  WL-PASTE  "
    ));
    assert!(!matches_startup_text(
        "Clipboard Sync",
        "wl-paste --watch",
        "docker"
    ));
}

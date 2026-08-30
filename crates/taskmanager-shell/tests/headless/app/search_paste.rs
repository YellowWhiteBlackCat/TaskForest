//! The bulk search-input vocabulary for bracketed paste (`push_search_text`):
//! sanitization (line breaks collapse, control bytes drop), the bounded
//! query cap, and the cursor/multi-set reset mirroring `push_search_char`.

use super::*;
use taskmanager_core::core::process::ProcessLiveKey;

#[allow(dead_code)]
fn identity_of(app: &crate::ShellApp, pid: u32) -> ProcessLiveKey {
    app.projection()
        .processes_slice()
        .iter()
        .find(|process| process.pid == pid)
        .and_then(ProcessLiveKey::from_process)
        .expect("demo process carries a current start token")
}

#[test]
fn paste_appends_printable_text_and_resets_the_cursor() {
    let mut shell = crate::demo_app();
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell.open_search();
    shell.selected = 5;
    assert!(shell.push_search_text("postgres"));
    assert_eq!(shell.query, "postgres");
    assert_eq!(
        shell.selected, 0,
        "a changed query restarts at the first row"
    );
}

#[test]
fn paste_collapses_line_breaks_to_single_spaces_and_drops_control_bytes() {
    let mut shell = crate::demo_app();
    shell.open_search();
    assert!(shell.push_search_text("sys\r\nlog\td\x00\r\nd"));
    // \r\n run collapses to ONE space; the tab becomes a space; NUL drops;
    // the lone \r before the final `d` collapses too.
    assert_eq!(shell.query, "sys log d d");
}

#[test]
fn paste_into_a_query_ending_in_a_space_does_not_double_the_space() {
    let mut shell = crate::demo_app();
    shell.open_search();
    assert!(shell.push_search_text("sys "));
    assert!(shell.push_search_text("log"));
    assert_eq!(shell.query, "sys log");
    assert!(shell.push_search_text("\n\nx"));
    assert_eq!(
        shell.query, "sys log x",
        "a break run adds one space, not many"
    );
}

#[test]
fn paste_is_bounded_by_the_query_cap() {
    let mut shell = crate::demo_app();
    shell.open_search();
    let flood = "x".repeat(SEARCH_QUERY_MAX + 500);
    assert!(shell.push_search_text(&flood));
    assert_eq!(
        shell.query.chars().count(),
        SEARCH_QUERY_MAX,
        "one paste can never grow the query past the cap"
    );
    // A second paste finds the query already full and changes nothing.
    assert!(!shell.push_search_text("more"));
    assert_eq!(shell.query.chars().count(), SEARCH_QUERY_MAX);
}

#[test]
fn an_empty_or_fully_control_paste_changes_nothing() {
    let mut shell = crate::demo_app();
    shell.open_search();
    shell.selected = 3;
    assert!(!shell.push_search_text(""));
    assert!(!shell.push_search_text("\x00\x01\x02"));
    assert_eq!(shell.query, "");
    assert_eq!(shell.selected, 3, "a no-op paste must not move the cursor");
}

#[test]
fn paste_resets_the_multi_select_anchor_like_typing_does() {
    let mut shell = crate::demo_app();
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    // Grow a multi set past the anchor via the shell's own pid-level API:
    // the anchor row's pid plus the next row's pid.
    let pids: Option<(u32, u32)> = {
        let rows = shell.visible_processes();
        match (rows.get(shell.selected), rows.get(shell.selected + 1)) {
            (Some(anchor), Some(second)) => Some((anchor.pid, second.pid)),
            _ => None,
        }
    };
    if let Some((anchor_pid, second_pid)) = pids {
        shell.toggle_selected_identity(identity_of(&shell, anchor_pid));
        shell.toggle_selected_identity(identity_of(&shell, second_pid));
    }
    assert!(shell.selected_identities().len() >= 2);
    shell.open_search();
    assert!(shell.push_search_text("z"));
    assert_eq!(
        shell.selected_identities().len(),
        1,
        "a changed query collapses the multi set back to the new anchor"
    );
}

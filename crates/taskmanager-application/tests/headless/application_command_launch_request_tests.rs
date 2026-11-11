//! test-intent: behavior
//! One command-launch owner canonicalizes every frontend command draft.

use taskmanager_application::CommandLaunchRequest;

#[test]
fn a_single_line_draft_is_trimmed_before_it_leaves_the_frontend() {
    assert_eq!(
        CommandLaunchRequest::from_draft("  /usr/bin/echo hi  "),
        Some(CommandLaunchRequest {
            command: "/usr/bin/echo hi".to_owned()
        })
    );
}

#[test]
fn a_multiline_draft_keeps_real_newlines_and_normalizes_crlf_and_cr() {
    assert_eq!(
        CommandLaunchRequest::from_draft("  echo one\r\n\recho two  "),
        Some(CommandLaunchRequest {
            command: "echo one\n\necho two".to_owned()
        })
    );
}

#[test]
fn a_blank_draft_is_absent_instead_of_an_empty_launch_request() {
    assert_eq!(CommandLaunchRequest::from_draft(" \r\n\t "), None);
}

//! The session-family fact: one classification rule and three availability states.
//!
//! A frontend must wear the family the contract reports, so these tests pin the
//! rule that decides it (and the absence of one) rather than any renderer.

use taskforest_core::core::session::{SessionFamily, SessionFamilyFact, SessionItem};

#[test]
fn the_transport_outranks_the_terminal_that_carries_it() {
    assert_eq!(
        SessionFamilyFact::classify(Some("seat0"), Some("tty1"), true),
        SessionFamilyFact::Observed(SessionFamily::Remote),
        "an SSH session on a console line is still a remote session"
    );
    assert_eq!(
        SessionFamilyFact::classify(None, Some("wayland-0"), true),
        SessionFamilyFact::Observed(SessionFamily::Remote)
    );
}

#[test]
fn a_display_owned_terminal_is_a_graphical_session() {
    for tty in [":0", ":1", "wayland-0", "wayland-1"] {
        assert_eq!(
            SessionFamilyFact::classify(None, Some(tty), false),
            SessionFamilyFact::Observed(SessionFamily::Graphical),
            "{tty} is owned by a display server"
        );
    }
    assert_eq!(
        SessionFamilyFact::classify(Some("seat0"), None, false),
        SessionFamilyFact::Observed(SessionFamily::Graphical),
        "a seat with no terminal belongs to a display session"
    );
}

#[test]
fn a_terminal_owned_line_is_a_console_session() {
    for tty in ["tty1", "tty2", "/dev/tty7", "pts/3", "/dev/pts/4"] {
        assert_eq!(
            SessionFamilyFact::classify(None, Some(tty), false),
            SessionFamilyFact::Observed(SessionFamily::Console),
            "{tty} is a terminal-owned line"
        );
    }
    assert_eq!(
        SessionFamilyFact::classify(Some("seat0"), Some("tty2"), false),
        SessionFamilyFact::Observed(SessionFamily::Console),
        "the terminal names the family even when the session also has a seat"
    );
}

#[test]
fn observed_facts_that_name_no_family_are_unknown_not_a_guess() {
    assert_eq!(
        SessionFamilyFact::classify(None, Some("console"), false),
        SessionFamilyFact::Observed(SessionFamily::Console)
    );
    assert_eq!(
        SessionFamilyFact::classify(None, Some("hvc0"), false),
        SessionFamilyFact::Observed(SessionFamily::Unknown),
        "an unrecognized line is reported as an observed unknown"
    );
    assert_eq!(
        SessionFamilyFact::classify(Some("seat0"), Some("hvc0"), false),
        SessionFamilyFact::Observed(SessionFamily::Unknown),
        "a present but unrecognized tty is not overwritten by the seat"
    );
}

#[test]
fn nothing_observed_stays_unobserved_and_unavailable_is_typed() {
    assert_eq!(
        SessionFamilyFact::classify(None, None, false),
        SessionFamilyFact::Unobserved,
        "facts that identify nothing must not be guessed into a family"
    );
    assert_eq!(
        SessionFamilyFact::classify(Some(""), Some(" "), false),
        SessionFamilyFact::Unobserved,
        "blank tokens do not fabricate an observed family"
    );
    assert_eq!(SessionFamilyFact::default(), SessionFamilyFact::Unobserved);

    assert_eq!(SessionFamilyFact::Unobserved.family(), None);
    assert_eq!(SessionFamilyFact::Unavailable.family(), None);
    assert_eq!(
        SessionFamilyFact::Observed(SessionFamily::Remote).family(),
        Some(SessionFamily::Remote)
    );
    assert!(!SessionFamilyFact::Unobserved.is_observed());
    assert!(!SessionFamilyFact::Unavailable.is_observed());
    assert!(SessionFamilyFact::Observed(SessionFamily::Console).is_observed());
}

#[test]
fn a_payload_without_the_family_key_decodes_as_unobserved() {
    // Rows written before the fact existed must read back as "never observed",
    // never as a real family and never as an error.
    let payload = r#"{
        "id": "6",
        "incarnation": null,
        "uid": 1000,
        "user": "ada",
        "seat": null,
        "tty": null,
        "remote": false,
        "timestamp": null
    }"#;
    let item: SessionItem = serde_json::from_str(payload).expect("older payload still decodes");
    assert_eq!(item.family, SessionFamilyFact::Unobserved);
}

//! The shared session-control confirmation gate (`pending_session`): arming
//! from an exact renderer-local row, the frozen identity surviving a list
//! refresh, confirmation emitting the correlated platform effect, and
//! dismissal clearing the gate without submitting.

use super::*;
use taskmanager_core::core::session::SessionIncarnation;

fn demo_session(id: &str) -> SessionItem {
    SessionItem {
        id: id.into(),
        incarnation: Some(SessionIncarnation::WindowsWts {
            logon_time_100ns: 42,
        }),
        uid: 501,
        user: "test-user".into(),
        ..SessionItem::default()
    }
}

#[test]
fn select_session_control_arms_the_gate_with_the_frozen_target() {
    let mut shell = ShellApp::new();
    let session = demo_session("9");
    shell.data.sessions = Some(vec![session.clone()]);
    assert!(shell.select_session_control(&session, SessionControlAction::Lock));
    let pending = shell.pending_session().expect("gate must be armed");
    assert_eq!(pending.session.id, session.id);
    assert_eq!(pending.action, SessionControlAction::Lock);
    assert_ne!(
        pending.request_id.get(),
        0,
        "a latest-wins correlation id must be allocated at arm time"
    );
}

#[test]
fn a_row_without_a_provider_identity_cannot_arm_the_gate() {
    let mut shell = ShellApp::new();
    let mut session = demo_session("2");
    session.incarnation = None;
    shell.data.sessions = Some(vec![session.clone()]);
    session.id = "".into();
    assert!(!shell.select_session_control(&session, SessionControlAction::Disconnect));
    assert!(shell.pending_session().is_none());
}

#[test]
fn a_duplicate_incarnation_has_no_confirmation_or_control_target() {
    let mut shell = ShellApp::new();
    let session = demo_session("9");
    shell.data.sessions = Some(vec![session.clone(), session.clone()]);

    assert!(!shell.select_session_control(&session, SessionControlAction::Disconnect));
    assert!(shell.pending_session().is_none());
    assert!(shell.confirm_session_control().is_none());
}

#[test]
fn confirm_emits_the_frozen_identity_and_clears_the_gate() {
    let mut shell = ShellApp::new();
    let session = demo_session("9");
    let other = demo_session("3");
    shell.data.sessions = Some(vec![session.clone(), other.clone()]);
    assert!(shell.select_session_control(&session, SessionControlAction::Disconnect));
    let armed_id = shell.pending_session().expect("gate armed").request_id;

    // Reordering the list between arm and confirm preserves the exact target.
    shell.data.sessions = Some(vec![other, session.clone()]);
    let Some(PlatformEffect::SessionControl(target)) = shell.confirm_session_control() else {
        panic!("confirm must emit a SessionControl effect");
    };
    assert_eq!(
        target.identity,
        session.identity().expect("trusted session")
    );
    assert_eq!(target.action, SessionControlAction::Disconnect);
    assert_eq!(target.request_id, armed_id);
    assert!(shell.pending_session().is_none());
    assert!(
        shell.confirm_session_control().is_none(),
        "confirming again without a gate must not emit"
    );
}

#[test]
fn a_replaced_session_invalidates_the_armed_confirmation() {
    let mut shell = ShellApp::new();
    let session = demo_session("9");
    shell.data.sessions = Some(vec![session.clone()]);
    assert!(shell.select_session_control(&session, SessionControlAction::Disconnect));

    let mut replacement = session.clone();
    replacement.incarnation = Some(SessionIncarnation::WindowsWts {
        logon_time_100ns: 43,
    });
    shell.data.sessions = Some(vec![replacement]);

    assert!(shell.confirm_session_control().is_none());
    assert!(shell.pending_session().is_none());
}

#[test]
fn dismiss_clears_the_gate_without_submitting() {
    let mut shell = ShellApp::new();
    let session = demo_session("2");
    shell.data.sessions = Some(vec![session.clone()]);
    assert!(shell.select_session_control(&session, SessionControlAction::Disconnect));
    shell.dismiss_overlay();
    assert!(shell.pending_session().is_none());
    assert!(shell.confirm_session_control().is_none());
}

#[test]
fn page_change_clears_the_gate() {
    let mut shell = ShellApp::new();
    let session = demo_session("2");
    shell.data.sessions = Some(vec![session.clone()]);
    assert!(shell.select_session_control(&session, SessionControlAction::Disconnect));
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Performance));
    assert!(
        shell.pending_session().is_none(),
        "leaving the Users page must drop the armed session gate"
    );
}

//! The shared session-control confirmation gate (`pending_session`): arming
//! from an exact renderer-local row, the frozen identity surviving a list
//! refresh, confirmation emitting the correlated platform effect, and
//! dismissal clearing the gate without submitting.

use super::*;

fn demo_session(id: &str) -> SessionItem {
    crate::fixture::demo_app()
        .data
        .sessions
        .and_then(|sessions| {
            sessions
                .iter()
                .find(|session| session.id.as_str() == id)
                .cloned()
        })
        .unwrap_or_else(|| panic!("demo session {id}"))
}

#[test]
fn select_session_control_arms_the_gate_with_the_frozen_target() {
    let mut shell = ShellApp::new();
    let session = demo_session("9");
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
    session.id = "".into();
    assert!(!shell.select_session_control(&session, SessionControlAction::Disconnect));
    assert!(shell.pending_session().is_none());
}

#[test]
fn confirm_emits_the_frozen_identity_and_clears_the_gate() {
    let mut shell = ShellApp::new();
    let session = demo_session("9");
    assert!(shell.select_session_control(&session, SessionControlAction::Disconnect));
    let armed_id = shell.pending_session().expect("gate armed").request_id;

    // A list refresh between arm and confirm must not retarget the request:
    // the frozen identity — not the cursor — is what confirm submits.
    shell.data.sessions = None;
    let Some(PlatformEffect::SessionControl(target)) = shell.confirm_session_control() else {
        panic!("confirm must emit a SessionControl effect");
    };
    assert_eq!(target.session_id.as_str(), session.id);
    assert_eq!(target.action, SessionControlAction::Disconnect);
    assert_eq!(target.request_id, armed_id);
    assert!(shell.pending_session().is_none());
    assert!(
        shell.confirm_session_control().is_none(),
        "confirming again without a gate must not emit"
    );
}

#[test]
fn dismiss_clears_the_gate_without_submitting() {
    let mut shell = ShellApp::new();
    let session = demo_session("2");
    assert!(shell.select_session_control(&session, SessionControlAction::Disconnect));
    shell.dismiss_overlay();
    assert!(shell.pending_session().is_none());
    assert!(shell.confirm_session_control().is_none());
}

#[test]
fn page_change_clears_the_gate() {
    let mut shell = ShellApp::new();
    let session = demo_session("2");
    assert!(shell.select_session_control(&session, SessionControlAction::Disconnect));
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Performance));
    assert!(
        shell.pending_session().is_none(),
        "leaving the Users page must drop the armed session gate"
    );
}

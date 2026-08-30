use super::*;
use taskmanager_core::core::process::ProcessLiveKey;

#[test]
fn confirmation_freezes_target_while_navigation_changes_only_verb() {
    let target = ControlTarget::Service("worker.service".to_owned());
    let mut session = ControlSession::new(
        target.clone(),
        vec![
            ControlChoice::enabled(ControlVerb::ServiceStop),
            ControlChoice::enabled(ControlVerb::ServiceRestart),
        ],
    );

    assert_eq!(session.target(), &target);
    session.advance(ControlInput::Down);
    let outcome = session
        .advance(ControlInput::Confirm)
        .expect("enabled choice confirms");
    assert_eq!(
        outcome,
        ControlOutcome::Confirmed(FrozenControl {
            target,
            verb: ControlVerb::ServiceRestart,
        })
    );
}

#[test]
fn disabled_action_is_visible_but_never_authorized() {
    let mut session = ControlSession::new(
        ControlTarget::Process(ProcessLiveKey::from_parts(42, 420).expect("live process key")),
        vec![ControlChoice::disabled(ControlVerb::Terminate)],
    );
    assert_eq!(session.advance(ControlInput::Confirm), None);
    assert_eq!(session.selection(), 0);
    assert_eq!(
        session.selected_choice().map(|choice| choice.enabled),
        Some(false)
    );
}

#[test]
fn cancel_reports_the_original_frozen_target() {
    let target = ControlTarget::Session("alice@tty1".to_owned());
    let mut session = ControlSession::new(
        target.clone(),
        vec![ControlChoice::enabled(ControlVerb::SessionDisconnect)],
    );
    let outcome = session
        .advance(ControlInput::Cancel)
        .expect("cancel is an explicit outcome");
    match outcome {
        ControlOutcome::Canceled(control) => {
            assert_eq!(control.target(), &target);
            assert_eq!(control.target_key(), "session:alice@tty1");
        }
        ControlOutcome::Confirmed(_) => panic!("cancel cannot authorize a control"),
    }
}

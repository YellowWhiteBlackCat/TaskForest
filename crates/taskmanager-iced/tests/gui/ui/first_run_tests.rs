// test-intent: behavior
//! Headless behavior tests for the Iced first-run dialog: the GPUI-parity
//! trigger/persistence fold, the typed action phases, side-effect-free
//! dismissal, and render coverage of the honest dialog states.

use super::*;
use std::path::PathBuf;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::setup::{SetupScriptAction, SetupScriptInfo};

fn info() -> SetupScriptInfo {
    SetupScriptInfo {
        path: PathBuf::from("/usr/share/taskforest/setup.sh"),
        run_command: "taskforest-setup run".to_owned(),
        revert_command: "taskforest-setup revert".to_owned(),
    }
}

fn available_state() -> FirstRunUiState {
    let mut state = FirstRunUiState::default();
    let _ = state.reduce(FirstRunEvent::ObservationCompleted(Some(info())));
    state
}

#[test]
fn observation_with_asset_shows_dialog_and_absence_keeps_it_hidden() {
    let mut state = FirstRunUiState::default();
    assert!(!state.visible(), "the dialog starts hidden");

    assert_eq!(
        state.reduce(FirstRunEvent::ObservationCompleted(Some(info()))),
        FirstRunTransition::Shown
    );
    assert_eq!(state.phase, FirstRunPhase::Available);
    assert!(state.info.is_some());
    assert!(state.visible());

    // The persisted done-state is the platform-side asset's absence (there
    // is no "do not show again" config field anywhere in the stack): after
    // Run/Revert consumes the script the next observation answers None and
    // the dialog stays hidden — never re-shown from stale state.
    assert_eq!(
        state.reduce(FirstRunEvent::ObservationCompleted(None)),
        FirstRunTransition::Hidden
    );
    assert_eq!(state.phase, FirstRunPhase::Hidden);
    assert!(state.info.is_none());
    assert!(!state.visible());
}

#[test]
fn submitted_actions_move_to_pending_and_completion_sets_the_next_phase() {
    let mut state = available_state();

    assert_eq!(
        state.reduce(FirstRunEvent::ActionSubmitted(SetupScriptAction::Run)),
        FirstRunTransition::Shown
    );
    assert_eq!(state.phase, FirstRunPhase::Running);
    assert!(state.action_pending());
    assert_eq!(state.last_action, Some(SetupScriptAction::Run));

    assert_eq!(
        state.reduce(FirstRunEvent::ActionCompleted(SetupScriptAction::Run)),
        FirstRunTransition::Shown
    );
    assert_eq!(state.phase, FirstRunPhase::RestartRequired);
    assert!(!state.action_pending());

    assert_eq!(
        state.reduce(FirstRunEvent::ActionSubmitted(SetupScriptAction::Revert)),
        FirstRunTransition::Shown
    );
    assert_eq!(state.phase, FirstRunPhase::Reverting);
    assert_eq!(
        state.reduce(FirstRunEvent::ActionCompleted(SetupScriptAction::Revert)),
        FirstRunTransition::Shown
    );
    assert_eq!(state.phase, FirstRunPhase::Available);
}

#[test]
fn typed_failures_surface_and_a_failed_observation_hides() {
    let mut state = available_state();

    // The failure follows a real submission, so the retry memory survives it.
    let _ = state.reduce(FirstRunEvent::ActionSubmitted(SetupScriptAction::Run));
    assert_eq!(
        state.reduce(FirstRunEvent::ActionFailed {
            action: SetupScriptAction::Run,
            kind: FailureKind::TimedOut,
        }),
        FirstRunTransition::Shown
    );
    assert_eq!(state.phase, FirstRunPhase::Failed(FailureKind::TimedOut));
    // The retry memory survives the failure.
    assert_eq!(state.last_action, Some(SetupScriptAction::Run));

    // A failed boot observation is the honest "capability cannot answer"
    // case: the dialog stays hidden instead of rendering a broken shell.
    let mut fresh = FirstRunUiState::default();
    let _ = fresh.reduce(FirstRunEvent::ActionSubmitted(SetupScriptAction::Observe));
    assert_eq!(fresh.phase, FirstRunPhase::Discovering);
    assert_eq!(
        fresh.reduce(FirstRunEvent::ActionFailed {
            action: SetupScriptAction::Observe,
            kind: FailureKind::Unsupported,
        }),
        FirstRunTransition::Hidden
    );
    assert_eq!(fresh.phase, FirstRunPhase::Hidden);
}

#[test]
fn dismissal_has_zero_side_effects() {
    let mut state = available_state();
    let _ = state.reduce(FirstRunEvent::ActionSubmitted(SetupScriptAction::Revert));
    let before = state.clone();

    assert_eq!(
        state.reduce(FirstRunEvent::Dismissed),
        FirstRunTransition::Unchanged
    );
    assert_eq!(
        state, before,
        "Escape/close must not mutate any dialog state"
    );
}

#[test]
fn render_covers_the_honest_dialog_states_without_panic() {
    let app = crate::IcedApp::demo();
    let theme_snapshot = app.theme();

    let _ = render_first_run(theme_snapshot, &available_state(), 1.0);

    let mut failed = available_state();
    let _ = failed.reduce(FirstRunEvent::ActionFailed {
        action: SetupScriptAction::Run,
        kind: FailureKind::PermissionDenied,
    });
    let _ = render_first_run(theme_snapshot, &failed, 1.0);

    // Still discovering (no descriptor yet): the honest waiting body.
    let mut discovering = FirstRunUiState::default();
    let _ = discovering.reduce(FirstRunEvent::ActionSubmitted(SetupScriptAction::Observe));
    let _ = render_first_run(theme_snapshot, &discovering, 0.5);

    // Restart-required renders the restart affordance branch.
    let mut restart = available_state();
    let _ = restart.reduce(FirstRunEvent::ActionSubmitted(SetupScriptAction::Run));
    let _ = restart.reduce(FirstRunEvent::ActionCompleted(SetupScriptAction::Run));
    let _ = render_first_run(theme_snapshot, &restart, 1.0);
}

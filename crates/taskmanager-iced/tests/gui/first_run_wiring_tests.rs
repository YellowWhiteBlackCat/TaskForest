// test-intent: behavior
//! Wiring-layer behavior tests for the first-run dialog's Iced lifecycle:
//! the boot observation lane, correlated platform-batch folding, the
//! surface-slot transitions, side-effect-free dismissal and the focus-target
//! registration. The dialog's own fold contract is proven by the
//! `ui::first_run` module tests; these prove the composition around it.

use std::collections::HashMap;
use std::path::PathBuf;

use taskmanager_application::{
    CapabilityId, CorrelatedSetupScriptEvent, EventSequence, FailureKind, OperationFailure,
    PlatformEventBatch, PlatformEventContext, RequestId, RetryDisposition, SetupScriptAction,
    SetupScriptEvent, SetupScriptInfo,
};

use super::*;
use crate::app::FocusTarget;

fn info() -> SetupScriptInfo {
    SetupScriptInfo {
        path: PathBuf::from("/usr/share/taskforest/setup.sh"),
        run_command: "taskforest-setup run".to_owned(),
        revert_command: "taskforest-setup revert".to_owned(),
    }
}

fn correlated(request_id: RequestId, event: SetupScriptEvent) -> CorrelatedSetupScriptEvent {
    CorrelatedSetupScriptEvent::new(
        PlatformEventContext {
            request_id,
            capability: CapabilityId::FIRST_RUN_SETUP,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 0,
        },
        event,
    )
}

fn operation_failure(request_id: RequestId, kind: FailureKind) -> OperationFailure {
    OperationFailure {
        request_id,
        capability: CapabilityId::FIRST_RUN_SETUP,
        sequence: EventSequence::new(2),
        kind,
        retry: RetryDisposition::Never,
        provider: None,
        observed_at_ms: 0,
    }
}

fn observe_request() -> (RequestId, HashMap<RequestId, SetupScriptAction>) {
    let id = RequestId::MIN;
    let mut pending = HashMap::new();
    pending.insert(id, SetupScriptAction::Observe);
    (id, pending)
}

/// An app with the observation already answered `Some(info)`: the dialog is
/// shown through the same wiring the tick lane uses.
fn shown_app() -> IcedApp {
    let mut app = IcedApp::new(None);
    let (id, pending) = observe_request();
    let batch = PlatformEventBatch {
        setup_script_events: vec![correlated(id, SetupScriptEvent::Observed(Some(info())))],
        ..PlatformEventBatch::default()
    };
    let events = extract_batch_events(&batch, &mut pending.clone());
    app.fold_first_run_events(events);
    app
}

#[test]
fn construction_without_platform_keeps_the_dialog_hidden() {
    let app = IcedApp::new(None);
    assert!(!app.first_run.visible(), "no platform means no answer");
    assert_eq!(app.local_surface_kind(), None);
    assert!(
        app.first_run_requests.is_empty(),
        "the failed boot observation leaves no pending request"
    );
}

#[test]
fn observation_answer_decides_visibility_through_the_surface_slot() {
    // With an asset: shown.
    let mut app = shown_app();
    assert_eq!(
        app.first_run.phase,
        crate::ui::first_run::FirstRunPhase::Available
    );
    assert!(app.first_run.visible());
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::FirstRun));

    // Without an asset (consumed by an earlier Run/Revert): hidden — the
    // persisted done-state is the platform-side script's absence.
    let (id, pending) = observe_request();
    let batch = PlatformEventBatch {
        setup_script_events: vec![correlated(id, SetupScriptEvent::Observed(None))],
        ..PlatformEventBatch::default()
    };
    let events = extract_batch_events(&batch, &mut pending.clone());
    app.fold_first_run_events(events);
    assert_eq!(
        app.first_run.phase,
        crate::ui::first_run::FirstRunPhase::Hidden
    );
    assert_eq!(app.local_surface_kind(), None);
}

#[test]
fn close_dismisses_the_surface_slot_without_touching_dialog_state() {
    let mut app = shown_app();
    let before = app.first_run.clone();

    let _ = app.update(Message::FirstRun(FirstRunMessage::Close));

    assert_eq!(app.local_surface_kind(), None, "the slot resets");
    assert_eq!(
        app.first_run, before,
        "dismissal keeps the dialog state bit-identical"
    );
    assert!(!app.shell.should_quit());
}

#[test]
fn escape_rides_the_local_surface_lane_and_keeps_dialog_state_bit_identical() {
    use taskmanager_application::{KeyCode, Modifiers};
    use taskmanager_shell::ShellKeyEvent;

    let mut app = shown_app();
    let before = app.first_run.clone();

    // Escape routes through the generic `LocalSurface` dismissal lane (the
    // dialog never installs its own keyboard handler), so the wiring must
    // close the slot without touching the dialog state.
    let _ = app.update(Message::Key(crate::app::IcedKey::Fixed(
        ShellKeyEvent::new(KeyCode::Escape, Modifiers::NONE),
    )));

    assert_eq!(app.local_surface_kind(), None);
    assert_eq!(app.first_run, before);
}

#[test]
fn request_action_without_a_platform_folds_the_typed_failure_and_stays_shown() {
    let mut app = shown_app();

    let _ = app.update(Message::FirstRun(FirstRunMessage::RequestAction(
        SetupScriptAction::Run,
    )));

    assert_eq!(
        app.first_run.phase,
        crate::ui::first_run::FirstRunPhase::Failed(FailureKind::TemporarilyUnavailable),
        "GPUI parity: a stopped runtime folds TemporarilyUnavailable, never a fake run"
    );
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::FirstRun));
    assert_eq!(
        app.first_run.last_action, None,
        "no retry memory for a rejected submission"
    );
}

#[test]
fn failed_observation_answer_hides_the_dialog() {
    let mut app = shown_app();
    let (id, pending) = observe_request();
    let batch = PlatformEventBatch {
        failures: vec![operation_failure(id, FailureKind::Unsupported)],
        ..PlatformEventBatch::default()
    };
    let events = extract_batch_events(&batch, &mut pending.clone());

    app.fold_first_run_events(events);

    assert_eq!(
        app.first_run.phase,
        crate::ui::first_run::FirstRunPhase::Hidden
    );
    assert_eq!(app.local_surface_kind(), None);
}

#[test]
fn untracked_correlated_answers_belong_to_other_lanes() {
    let mut app = shown_app();
    let (id, _) = observe_request();
    let batch = PlatformEventBatch {
        setup_script_events: vec![correlated(
            id,
            SetupScriptEvent::ActionCompleted {
                action: SetupScriptAction::Run,
            },
        )],
        ..PlatformEventBatch::default()
    };
    // No pending entry: the answer cannot retarget this dialog.
    let events = extract_batch_events(&batch, &mut HashMap::new());

    assert!(events.is_empty());
    app.fold_first_run_events(events);
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::FirstRun));
    assert_eq!(
        app.first_run.phase,
        crate::ui::first_run::FirstRunPhase::Available
    );
}

#[test]
fn action_completion_follows_platform_semantics_and_restart_quits() {
    let mut app = shown_app();

    // Run completes into RestartRequired (the platform consumed the asset).
    app.fold_first_run_events(vec![FirstRunEvent::ActionCompleted(SetupScriptAction::Run)]);
    assert_eq!(
        app.first_run.phase,
        crate::ui::first_run::FirstRunPhase::RestartRequired
    );
    assert!(!app.shell.should_quit());

    // Restart completes after the platform relaunched the application: this
    // instance records the quit under the dedicated Restart reason (GPUI's
    // post-restart app.quit()) — not a borrowed window-close label.
    app.fold_first_run_events(vec![FirstRunEvent::ActionCompleted(
        SetupScriptAction::Restart,
    )]);
    assert!(app.shell.should_quit());
    assert_eq!(
        app.shell.quit_reason(),
        Some(taskmanager_shell::QuitReason::Restart)
    );
}

#[test]
fn first_run_focus_targets_are_registered_in_the_focus_table() {
    let all = FocusTarget::ALL;
    for row in 0..=2u8 {
        assert!(all.contains(&FocusTarget::FirstRunCopy(row)));
    }
    for index in 0..=5u8 {
        assert!(all.contains(&FocusTarget::FirstRunAction(index)));
    }
}

#[test]
fn view_renders_the_mounted_dialog_without_panicking() {
    // The demo shell is not collecting, so the local-modal branch renders.
    let mut app = IcedApp::demo();
    app.fold_first_run_events(vec![FirstRunEvent::ObservationCompleted(Some(info()))]);
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::FirstRun));

    let _ = crate::ui::view(&app);
}

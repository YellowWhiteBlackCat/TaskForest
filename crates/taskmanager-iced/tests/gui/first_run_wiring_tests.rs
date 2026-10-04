//! test-intent: behavior

use super::*;
use taskmanager_application::first_run::FirstRunPhase;
use taskmanager_application::{
    CorrelatedSetupScriptEvent, PlatformEventBatch, PlatformEventContext,
};
use taskmanager_assets::product::REPOSITORY_URL;
use taskmanager_core::core::setup::{SetupScriptAction, SetupScriptEvent};
use taskmanager_platform_contract::{CapabilityId, EventSequence};
use taskmanager_shell::fixture::setup::setup_script_info;
use taskmanager_test_support::setup_script::{SetupScriptRecorder, platform};

fn observed_app() -> (IcedApp, SetupScriptRecorder) {
    let (platform, recorder) = platform();
    let mut app = IcedApp::new(Some(platform));
    let requests = recorder.submissions().expect("recorded");
    let request = requests.last().expect("boot Observe").id;
    let event = CorrelatedSetupScriptEvent::new(
        PlatformEventContext {
            request_id: request,
            capability: CapabilityId::FIRST_RUN_SETUP,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
        },
        SetupScriptEvent::Observed(Some(setup_script_info())),
    );
    let outcome = app.first_run.fold_batch(&PlatformEventBatch {
        setup_script_events: vec![event],
        ..Default::default()
    });
    app.apply_first_run_completion(outcome);
    (app, recorder)
}

#[test]
fn discovery_is_quiet_and_the_settings_entry_is_explicit() {
    let (mut app, recorder) = observed_app();
    assert_eq!(app.first_run.view().phase, FirstRunPhase::Available);
    assert_eq!(app.local_surface_kind(), None);
    assert_eq!(recorder.submissions().expect("recorded").len(), 1);
    let _ = app.update(Message::FirstRun(FirstRunMessage::Open));
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::FirstRun));
    let _ = app.update(Message::FirstRun(FirstRunMessage::Close));
    assert_eq!(app.local_surface_kind(), None);
    assert_eq!(app.first_run.view().phase, FirstRunPhase::Available);
    let _ = app.update(Message::FirstRun(FirstRunMessage::RequestAction(
        SetupScriptAction::Run,
    )));
    assert_eq!(
        recorder.submissions().expect("recorded").len(),
        1,
        "a stale action cannot run after dismissal"
    );
}

#[test]
fn a_missing_descriptor_never_opens_a_setup_surface() {
    let mut app = IcedApp::new(None);
    let _ = app.update(Message::FirstRun(FirstRunMessage::Open));
    assert!(app.local_surface_kind().is_none());
    assert!(app.first_run.view().info.is_none());
    assert!(app.first_run.view().observation_failure().is_some());
}

#[test]
fn explicit_native_actions_stay_pending_once_and_docs_use_the_url_port() {
    let (mut app, recorder) = observed_app();
    let _ = app.update(Message::FirstRun(FirstRunMessage::Open));
    let _ = app.update(Message::FirstRun(FirstRunMessage::RequestAction(
        SetupScriptAction::Run,
    )));
    let _ = app.update(Message::FirstRun(FirstRunMessage::RequestAction(
        SetupScriptAction::Revert,
    )));
    assert_eq!(app.first_run.view().phase, FirstRunPhase::Running);
    assert_eq!(recorder.submissions().expect("recorded").len(), 2);
    let result =
        app.reduce_first_run_message(Message::FirstRun(FirstRunMessage::OpenDocumentation));
    assert!(
        matches!(result.effect, Some(PlatformEffect::OpenUrl(request)) if request.url == REPOSITORY_URL)
    );
}

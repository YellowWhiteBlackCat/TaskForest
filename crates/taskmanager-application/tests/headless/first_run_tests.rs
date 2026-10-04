//! test-intent: behavior

use super::*;
use crate::{
    IntegrationFacets, PlatformEvent, PlatformEventContext, PlatformFacets, PlatformHandle,
};
use std::sync::{Arc, Mutex};
use taskmanager_platform_contract::{
    CapabilityDescriptor, CapabilitySnapshot, CapabilityStatus, EventEnvelope, EventPort,
    EventPortError, EventSequence, RequestEnvelope, RequestPort, RetryDisposition, SubmissionError,
};

#[derive(Default)]
struct Requests(Mutex<Vec<RequestEnvelope<SetupScriptRequest>>>);
impl RequestPort for Requests {
    type Request = SetupScriptRequest;
    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0.lock().expect("request recorder").push(request);
        Ok(())
    }
}
struct QuietEvents;
impl EventPort for QuietEvents {
    type Event = PlatformEvent;
    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}
fn platform() -> (PlatformClient, Arc<Requests>) {
    let requests = Arc::new(Requests::default());
    let catalog = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::FIRST_RUN_SETUP,
        status: CapabilityStatus::Available,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);
    (
        PlatformClient::new(PlatformHandle::new(
            Arc::new(catalog),
            Arc::new(QuietEvents),
            PlatformFacets::default()
                .with_integration(IntegrationFacets::default().with_setup_script(requests.clone())),
        )),
        requests,
    )
}
fn info() -> SetupScriptInfo {
    SetupScriptInfo {
        path: "setup.sh".into(),
        run_command: "descriptive run command".into(),
        revert_command: "descriptive revert command".into(),
    }
}
fn event(id: RequestId, event: SetupScriptEvent) -> CorrelatedSetupScriptEvent {
    CorrelatedSetupScriptEvent::new(
        PlatformEventContext {
            request_id: id,
            capability: CapabilityId::FIRST_RUN_SETUP,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
        },
        event,
    )
}
fn id(requests: &Requests) -> RequestId {
    requests
        .0
        .lock()
        .expect("recorder")
        .last()
        .expect("submitted")
        .id
}

#[test]
fn discovery_updates_metadata_without_a_presentation_transition() {
    let (mut platform, requests) = platform();
    let mut controller = FirstRunController::default();
    assert!(controller.observe(Some(&mut platform), 1));
    assert_eq!(controller.view().phase, FirstRunPhase::Discovering);
    assert_eq!(
        controller.complete(&event(
            id(&requests),
            SetupScriptEvent::Observed(Some(info()))
        )),
        FirstRunCompletion::Updated
    );
    assert_eq!(controller.view().phase, FirstRunPhase::Available);
    assert_eq!(controller.view().info, Some(info()));
    assert_eq!(
        controller.complete(&event(id(&requests), SetupScriptEvent::Observed(None))),
        FirstRunCompletion::Unchanged
    );
    assert!(
        controller.view().info.is_some(),
        "a duplicate cannot erase the observed descriptor"
    );
}

#[test]
fn explicit_actions_are_typed_admitted_once_and_correlated() {
    let (mut platform, requests) = platform();
    let mut controller = FirstRunController::from_observation(Some(info()));
    assert!(!controller.request(SetupScriptAction::Restart, Some(&mut platform), 1));
    assert_eq!(
        controller.view().phase,
        FirstRunPhase::Failed(FailureKind::Rejected)
    );
    assert!(requests.0.lock().expect("recorder").is_empty());
    assert!(controller.request(SetupScriptAction::Run, Some(&mut platform), 2));
    assert!(controller.view().action_pending());
    assert!(!controller.request(SetupScriptAction::Revert, Some(&mut platform), 3));
    assert_eq!(requests.0.lock().expect("recorder").len(), 1);
    let request = id(&requests);
    assert_eq!(
        controller.complete(&event(
            RequestId::new(request.get() + 1).expect("id"),
            SetupScriptEvent::ActionCompleted {
                action: SetupScriptAction::Run
            }
        )),
        FirstRunCompletion::Unchanged
    );
    assert_eq!(controller.view().phase, FirstRunPhase::Running);
    let mut wrong_capability = event(
        request,
        SetupScriptEvent::ActionCompleted {
            action: SetupScriptAction::Run,
        },
    );
    wrong_capability.capability = CapabilityId::TELEMETRY_GPU_ENGINES;
    assert_eq!(
        controller.complete(&wrong_capability),
        FirstRunCompletion::Unchanged
    );
    assert!(controller.view().action_pending());
    assert_eq!(
        controller.complete(&event(
            request,
            SetupScriptEvent::ActionCompleted {
                action: SetupScriptAction::Run
            }
        )),
        FirstRunCompletion::Updated
    );
    assert_eq!(controller.view().phase, FirstRunPhase::RestartRequired);
    assert!(!controller.view().action_pending());
    assert!(controller.request(SetupScriptAction::View, Some(&mut platform), 4));
    assert_eq!(
        controller.complete(&event(
            id(&requests),
            SetupScriptEvent::ActionCompleted {
                action: SetupScriptAction::View
            }
        )),
        FirstRunCompletion::Updated
    );
    assert_eq!(
        controller.view().phase,
        FirstRunPhase::RestartRequired,
        "reviewing the asset cannot erase the required restart"
    );
    assert!(controller.request(SetupScriptAction::Restart, Some(&mut platform), 5));
    assert_eq!(
        controller.complete(&event(
            id(&requests),
            SetupScriptEvent::ActionCompleted {
                action: SetupScriptAction::Restart
            }
        )),
        FirstRunCompletion::Restart
    );
}

#[test]
fn observation_failure_is_silent_typed_and_action_failure_retains_retry_intent() {
    let (mut platform, requests) = platform();
    let mut controller = FirstRunController::default();
    controller.observe(Some(&mut platform), 1);
    let failure = |request_id| OperationFailure {
        request_id,
        capability: CapabilityId::FIRST_RUN_SETUP,
        sequence: EventSequence::new(2),
        kind: FailureKind::PermissionDenied,
        retry: RetryDisposition::Never,
        provider: None,
        observed_at_ms: 2,
    };
    assert_eq!(
        controller.fail(&failure(id(&requests))),
        FirstRunCompletion::Updated
    );
    assert_eq!(controller.view().phase, FirstRunPhase::Hidden);
    assert_eq!(
        controller.view().observation_failure(),
        Some(FailureKind::PermissionDenied)
    );
    assert!(controller.view().info.is_none());
    controller = FirstRunController::from_observation(Some(info()));
    controller.request(SetupScriptAction::Revert, Some(&mut platform), 2);
    controller.fail(&failure(id(&requests)));
    assert_eq!(
        controller.view().phase,
        FirstRunPhase::Failed(FailureKind::PermissionDenied)
    );
    assert_eq!(
        controller.view().last_action,
        Some(SetupScriptAction::Revert)
    );
    assert!(controller.request(SetupScriptAction::Revert, Some(&mut platform), 3));
}

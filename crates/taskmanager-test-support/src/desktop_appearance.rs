//! Responding typed appearance port for native frontend boot/drain tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use taskmanager_application::{
    DesktopAppearanceEvent, DesktopAppearanceRequest, IntegrationFacets, PlatformClient,
    PlatformEvent, PlatformFacets, PlatformHandle,
};
use taskmanager_core::core::appearance::DesktopAppearance;
use taskmanager_platform_contract::{
    CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    CompositeSourceSnapshot, EventEnvelope, EventPort, EventPortError, EventSequence,
    RequestEnvelope, RequestPort, SubmissionError, SubmissionErrorKind,
};

#[derive(Clone, Default)]
pub struct AppearanceRecorder(Arc<Mutex<Vec<RequestEnvelope<DesktopAppearanceRequest>>>>);
impl AppearanceRecorder {
    pub fn submissions(
        &self,
    ) -> Result<Vec<RequestEnvelope<DesktopAppearanceRequest>>, &'static str> {
        self.0
            .lock()
            .map(|requests| requests.clone())
            .map_err(|_| "appearance recorder poisoned")
    }
}
#[derive(Default)]
struct Events(Mutex<VecDeque<EventEnvelope<PlatformEvent>>>);
impl EventPort for Events {
    type Event = PlatformEvent;
    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        self.0
            .lock()
            .map(|mut events| events.pop_front())
            .map_err(|_| EventPortError::RuntimeStopped)
    }
}
struct RespondingAppearance {
    recorder: AppearanceRecorder,
    events: Arc<Events>,
    value: DesktopAppearance,
}
impl RequestPort for RespondingAppearance {
    type Request = DesktopAppearanceRequest;
    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        let failure = || SubmissionError {
            capability: CapabilityId::DESKTOP_APPEARANCE,
            kind: SubmissionErrorKind::RuntimeStopped,
        };
        let mut requests = self.recorder.0.lock().map_err(|_| failure())?;
        let mut events = self.events.0.lock().map_err(|_| failure())?;
        events.push_back(EventEnvelope {
            request_id: request.id,
            capability: CapabilityId::DESKTOP_APPEARANCE,
            provider: None,
            sequence: EventSequence::new(request.id.get()),
            observed_at_ms: request.submitted_at_ms,
            outcome: Ok(PlatformEvent::DesktopAppearance(
                DesktopAppearanceEvent::Snapshot(CompositeSourceSnapshot {
                    value: self.value,
                    sources: Vec::new(),
                }),
            )),
        });
        requests.push(request);
        Ok(())
    }
}
pub fn platform(value: DesktopAppearance) -> (PlatformClient, AppearanceRecorder) {
    let recorder = AppearanceRecorder::default();
    let events = Arc::new(Events::default());
    let port = RespondingAppearance {
        recorder: recorder.clone(),
        events: events.clone(),
        value,
    };
    let catalog = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::DESKTOP_APPEARANCE,
        status: CapabilityStatus::Available,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);
    let facets = PlatformFacets::default()
        .with_integration(IntegrationFacets::default().with_desktop_appearance(Arc::new(port)));
    (
        PlatformClient::new(PlatformHandle::new(Arc::new(catalog), events, facets)),
        recorder,
    )
}

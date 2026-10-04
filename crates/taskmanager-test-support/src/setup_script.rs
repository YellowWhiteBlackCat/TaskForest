//! Quiet typed setup port with observable submissions for frontend behavior tests.

use std::sync::{Arc, Mutex};
use taskmanager_application::{
    IntegrationFacets, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    SetupScriptRequest,
};
use taskmanager_platform_contract::{
    CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus, EventEnvelope,
    EventPort, EventPortError, RequestEnvelope, RequestPort, SubmissionError, SubmissionErrorKind,
};

#[derive(Clone, Default)]
pub struct SetupScriptRecorder(Arc<Mutex<Vec<RequestEnvelope<SetupScriptRequest>>>>);

impl SetupScriptRecorder {
    pub fn submissions(&self) -> Result<Vec<RequestEnvelope<SetupScriptRequest>>, &'static str> {
        self.0
            .lock()
            .map(|requests| requests.clone())
            .map_err(|_| "setup recorder poisoned")
    }
}
impl RequestPort for SetupScriptRecorder {
    type Request = SetupScriptRequest;
    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        match self.0.lock() {
            Ok(mut requests) => {
                requests.push(request);
                Ok(())
            }
            Err(_) => Err(SubmissionError {
                capability: CapabilityId::FIRST_RUN_SETUP,
                kind: SubmissionErrorKind::RuntimeStopped,
            }),
        }
    }
}
struct QuietEvents;
impl EventPort for QuietEvents {
    type Event = PlatformEvent;
    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

pub fn platform() -> (PlatformClient, SetupScriptRecorder) {
    let recorder = SetupScriptRecorder::default();
    let catalog = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::FIRST_RUN_SETUP,
        status: CapabilityStatus::Available,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);
    let facets = PlatformFacets::default().with_integration(
        IntegrationFacets::default().with_setup_script(Arc::new(recorder.clone())),
    );
    (
        PlatformClient::new(PlatformHandle::new(
            Arc::new(catalog),
            Arc::new(QuietEvents),
            facets,
        )),
        recorder,
    )
}

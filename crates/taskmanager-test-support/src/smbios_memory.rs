//! Responding memory-inventory port for normal permission-entry integration tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use taskmanager_application::{
    PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle, SmbiosMemoryEvent,
    SmbiosMemoryRequest, SystemFacets,
};
use taskmanager_core::core::metrics::SmbiosMemorySnapshot;
use taskmanager_platform_contract::{
    CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus, EventEnvelope,
    EventPort, EventPortError, EventSequence, RequestEnvelope, RequestPort, SubmissionError,
    SubmissionErrorKind,
};

#[derive(Clone, Default)]
pub struct MemoryInventoryRecorder(Arc<Mutex<Vec<RequestEnvelope<SmbiosMemoryRequest>>>>);
impl MemoryInventoryRecorder {
    pub fn submissions(&self) -> Result<Vec<RequestEnvelope<SmbiosMemoryRequest>>, &'static str> {
        self.0
            .lock()
            .map(|requests| requests.clone())
            .map_err(|_| "memory recorder poisoned")
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

struct RespondingMemory {
    recorder: MemoryInventoryRecorder,
    events: Arc<Events>,
    value: SmbiosMemorySnapshot,
}
impl RequestPort for RespondingMemory {
    type Request = SmbiosMemoryRequest;
    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        let failure = || SubmissionError {
            capability: CapabilityId::TELEMETRY_MEMORY_SMBIOS,
            kind: SubmissionErrorKind::RuntimeStopped,
        };
        let mut requests = self.recorder.0.lock().map_err(|_| failure())?;
        let mut events = self.events.0.lock().map_err(|_| failure())?;
        events.push_back(EventEnvelope {
            request_id: request.id,
            capability: CapabilityId::TELEMETRY_MEMORY_SMBIOS,
            provider: None,
            sequence: EventSequence::new(request.id.get()),
            observed_at_ms: request.submitted_at_ms,
            outcome: Ok(PlatformEvent::SmbiosMemory(SmbiosMemoryEvent::Update(
                self.value.clone(),
            ))),
        });
        requests.push(request);
        Ok(())
    }
}

/// A native-shaped available authorization offer and its correlated response.
pub fn platform(value: SmbiosMemorySnapshot) -> (PlatformClient, MemoryInventoryRecorder) {
    let recorder = MemoryInventoryRecorder::default();
    let events = Arc::new(Events::default());
    let port = RespondingMemory {
        recorder: recorder.clone(),
        events: events.clone(),
        value,
    };
    let catalog = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_MEMORY_SMBIOS,
        status: CapabilityStatus::RequiresEscalation,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);
    let facets = PlatformFacets::default()
        .with_system(SystemFacets::default().with_smbios_memory(Arc::new(port)));
    (
        PlatformClient::new(PlatformHandle::new(Arc::new(catalog), events, facets)),
        recorder,
    )
}

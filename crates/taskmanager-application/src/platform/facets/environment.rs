//! Login-environment capability ports and events.
//!
//! Defines startup and session inventory, boot-evidence, and control requests
//! plus the `EnvironmentFacets` group of independently optional ports.

use std::sync::Arc;

use taskmanager_platform_contract::{
    CapabilityId, CapabilityRequest, PartialSourceSnapshot, RequestPort, RequestTracking,
    RequestTrackingError,
};

use crate::{
    SessionControlOutcome, SessionControlRequest, SessionItem, StartupBootEvidenceSnapshot,
    StartupControlOutcome, StartupControlRequest, StartupEntry,
};

#[derive(Clone, Debug)]
pub enum StartupEvent {
    Snapshot(PartialSourceSnapshot<StartupEntry>),
    Control(StartupControlOutcome),
}

impl StartupEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        match self {
            Self::Snapshot(_) => capability == &CapabilityId::STARTUP,
            Self::Control(_) => capability == &CapabilityId::STARTUP_CONTROL,
        }
    }
}

#[derive(Clone, Debug)]
pub enum StartupEvidenceEvent {
    Snapshot(StartupBootEvidenceSnapshot),
}

impl StartupEvidenceEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::STARTUP_EVIDENCE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupInventoryRequest {
    Refresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupEvidenceRequest {
    Refresh,
}

bind_request_capability!(StartupInventoryRequest, CapabilityId::STARTUP);
bind_request_capability!(StartupEvidenceRequest, CapabilityId::STARTUP_EVIDENCE);

impl CapabilityRequest for StartupControlRequest {
    const CAPABILITY: CapabilityId = CapabilityId::STARTUP_CONTROL;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        super::opaque_target_tracking(self.entry.id.as_str())
    }
}

#[derive(Clone, Debug)]
pub enum SessionEvent {
    Snapshot(PartialSourceSnapshot<SessionItem>),
    Control(SessionControlOutcome),
}

impl SessionEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        match self {
            Self::Snapshot(_) => capability == &CapabilityId::SESSIONS,
            Self::Control(_) => capability == &CapabilityId::SESSION_CONTROL,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionInventoryRequest {
    Refresh,
}

bind_request_capability!(SessionInventoryRequest, CapabilityId::SESSIONS);

impl CapabilityRequest for SessionControlRequest {
    const CAPABILITY: CapabilityId = CapabilityId::SESSION_CONTROL;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        super::opaque_target_tracking(self.session_id.as_str())
    }
}

pub type StartupInventoryRequestPort = dyn RequestPort<Request = StartupInventoryRequest>;
pub type StartupEvidenceRequestPort = dyn RequestPort<Request = StartupEvidenceRequest>;
pub type StartupControlRequestPort = dyn RequestPort<Request = StartupControlRequest>;
pub type SessionInventoryRequestPort = dyn RequestPort<Request = SessionInventoryRequest>;
pub type SessionControlRequestPort = dyn RequestPort<Request = SessionControlRequest>;

/// Independently optional login-environment ports. Startup and session work
/// retain separate queues and failure policies inside this construction group.
#[derive(Clone, Default)]
pub struct EnvironmentFacets {
    startup_inventory: Option<Arc<StartupInventoryRequestPort>>,
    startup_evidence: Option<Arc<StartupEvidenceRequestPort>>,
    startup_control: Option<Arc<StartupControlRequestPort>>,
    session_inventory: Option<Arc<SessionInventoryRequestPort>>,
    session_control: Option<Arc<SessionControlRequestPort>>,
}

impl EnvironmentFacets {
    #[must_use]
    pub fn with_startup_inventory(mut self, port: Arc<StartupInventoryRequestPort>) -> Self {
        self.startup_inventory = Some(port);
        self
    }

    #[must_use]
    pub fn with_startup_evidence(mut self, port: Arc<StartupEvidenceRequestPort>) -> Self {
        self.startup_evidence = Some(port);
        self
    }

    #[must_use]
    pub fn with_startup_control(mut self, port: Arc<StartupControlRequestPort>) -> Self {
        self.startup_control = Some(port);
        self
    }

    #[must_use]
    pub fn with_session_inventory(mut self, port: Arc<SessionInventoryRequestPort>) -> Self {
        self.session_inventory = Some(port);
        self
    }

    #[must_use]
    pub fn with_session_control(mut self, port: Arc<SessionControlRequestPort>) -> Self {
        self.session_control = Some(port);
        self
    }

    #[must_use]
    pub fn startup_inventory(&self) -> Option<&StartupInventoryRequestPort> {
        self.startup_inventory.as_deref()
    }

    #[must_use]
    pub fn startup_evidence(&self) -> Option<&StartupEvidenceRequestPort> {
        self.startup_evidence.as_deref()
    }

    #[must_use]
    pub fn startup_control(&self) -> Option<&StartupControlRequestPort> {
        self.startup_control.as_deref()
    }

    #[must_use]
    pub fn session_inventory(&self) -> Option<&SessionInventoryRequestPort> {
        self.session_inventory.as_deref()
    }

    #[must_use]
    pub fn session_control(&self) -> Option<&SessionControlRequestPort> {
        self.session_control.as_deref()
    }
}

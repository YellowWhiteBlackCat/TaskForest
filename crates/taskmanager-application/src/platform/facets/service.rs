//! Service capability ports and events.
//!
//! Defines service inventory, dependency, control, log-snapshot, and
//! log-stream requests plus the `ServiceFacets` group of independently
//! optional ports.

use std::sync::Arc;

use taskmanager_core::core::services::{ServiceAction, ServiceItem, ServiceLogQuery};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{
    CapabilityId, CapabilityRequest, PartialSourceSnapshot, RequestPort, RequestTracking,
    RequestTrackingError,
};

use crate::ServiceUpdate;

#[derive(Clone, Debug)]
pub enum ServiceEvent {
    Snapshot(PartialSourceSnapshot<ServiceItem>),
    Update(ServiceUpdate),
}

impl ServiceEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        match self {
            Self::Snapshot(_) => capability == &CapabilityId::SERVICES,
            Self::Update(ServiceUpdate::Action(_)) => capability == &CapabilityId::SERVICE_CONTROL,
            Self::Update(
                ServiceUpdate::Dependencies { .. } | ServiceUpdate::DependenciesUnavailable { .. },
            ) => capability == &CapabilityId::SERVICE_DEPENDENCIES,
            Self::Update(ServiceUpdate::Logs { .. }) => capability == &CapabilityId::SERVICE_LOGS,
            Self::Update(ServiceUpdate::LogStream { .. }) => {
                capability == &CapabilityId::SERVICE_LOG_STREAM
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceInventoryRequest {
    Refresh,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDependenciesRequest {
    pub service_id: ServiceId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceControlRequest {
    pub request_id: crate::ControlRequestId,
    pub service_id: ServiceId,
    pub action: ServiceAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceLogSnapshotRequest {
    pub service_id: ServiceId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceLogStreamRequest {
    pub query: ServiceLogQuery,
}

bind_request_capability!(ServiceInventoryRequest, CapabilityId::SERVICES);

impl CapabilityRequest for ServiceDependenciesRequest {
    const CAPABILITY: CapabilityId = CapabilityId::SERVICE_DEPENDENCIES;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        service_target_tracking(&self.service_id)
    }
}

impl CapabilityRequest for ServiceControlRequest {
    const CAPABILITY: CapabilityId = CapabilityId::SERVICE_CONTROL;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        service_target_tracking(&self.service_id)
    }
}

impl CapabilityRequest for ServiceLogSnapshotRequest {
    const CAPABILITY: CapabilityId = CapabilityId::SERVICE_LOGS;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        service_target_tracking(&self.service_id)
    }
}

impl CapabilityRequest for ServiceLogStreamRequest {
    const CAPABILITY: CapabilityId = CapabilityId::SERVICE_LOG_STREAM;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        service_target_tracking(&self.query.service_id)
    }
}

fn service_target_tracking(
    service_id: &ServiceId,
) -> Result<RequestTracking, RequestTrackingError> {
    super::opaque_target_tracking(service_id.as_str())
}

pub type ServiceInventoryRequestPort = dyn RequestPort<Request = ServiceInventoryRequest>;
pub type ServiceDependenciesRequestPort = dyn RequestPort<Request = ServiceDependenciesRequest>;
pub type ServiceControlRequestPort = dyn RequestPort<Request = ServiceControlRequest>;
pub type ServiceLogSnapshotRequestPort = dyn RequestPort<Request = ServiceLogSnapshotRequest>;
pub type ServiceLogStreamRequestPort = dyn RequestPort<Request = ServiceLogStreamRequest>;

/// Independently optional service ports grouped by their shared change domain.
#[derive(Clone, Default)]
pub struct ServiceFacets {
    inventory: Option<Arc<ServiceInventoryRequestPort>>,
    dependencies: Option<Arc<ServiceDependenciesRequestPort>>,
    control: Option<Arc<ServiceControlRequestPort>>,
    log_snapshot: Option<Arc<ServiceLogSnapshotRequestPort>>,
    log_stream: Option<Arc<ServiceLogStreamRequestPort>>,
}

impl ServiceFacets {
    #[must_use]
    pub fn with_inventory(mut self, port: Arc<ServiceInventoryRequestPort>) -> Self {
        self.inventory = Some(port);
        self
    }

    #[must_use]
    pub fn with_dependencies(mut self, port: Arc<ServiceDependenciesRequestPort>) -> Self {
        self.dependencies = Some(port);
        self
    }

    #[must_use]
    pub fn with_control(mut self, port: Arc<ServiceControlRequestPort>) -> Self {
        self.control = Some(port);
        self
    }

    #[must_use]
    pub fn with_log_snapshot(mut self, port: Arc<ServiceLogSnapshotRequestPort>) -> Self {
        self.log_snapshot = Some(port);
        self
    }

    #[must_use]
    pub fn with_log_stream(mut self, port: Arc<ServiceLogStreamRequestPort>) -> Self {
        self.log_stream = Some(port);
        self
    }

    #[must_use]
    pub fn inventory(&self) -> Option<&ServiceInventoryRequestPort> {
        self.inventory.as_deref()
    }

    #[must_use]
    pub fn dependencies(&self) -> Option<&ServiceDependenciesRequestPort> {
        self.dependencies.as_deref()
    }

    #[must_use]
    pub fn control(&self) -> Option<&ServiceControlRequestPort> {
        self.control.as_deref()
    }

    #[must_use]
    pub fn log_snapshot(&self) -> Option<&ServiceLogSnapshotRequestPort> {
        self.log_snapshot.as_deref()
    }

    #[must_use]
    pub fn log_stream(&self) -> Option<&ServiceLogStreamRequestPort> {
        self.log_stream.as_deref()
    }
}

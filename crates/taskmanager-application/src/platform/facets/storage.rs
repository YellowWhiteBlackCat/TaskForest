//! Storage-health and SMART capability ports and events.
//!
//! Defines filesystem-health, SMART observation, and SMART control requests,
//! the `SmartStateRevision` and `SmartObservationBatch` runtime types, and the
//! `StorageFacets` group of independently optional ports.

use std::sync::Arc;

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::storage::StorageDeviceTarget;
use taskmanager_core::core::storage_health::FilesystemHealthSnapshot;
use taskmanager_core::core::system_health::{SmartSelfTestIntent, SmartSelfTestObservation};
use taskmanager_platform_contract::{
    CapabilityId, CapabilityRequest, CompositeSourceSnapshot, RequestPort, RequestScope,
    RequestTracking, RequestTrackingError,
};

use crate::DirectoryUsageRequestPort;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageHealthRequest {
    Refresh,
}

#[derive(Clone, Debug)]
pub enum StorageHealthEvent {
    Snapshot(CompositeSourceSnapshot<FilesystemHealthSnapshot>),
}

impl StorageHealthEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::STORAGE_HEALTH
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmartObservationRequest {
    /// Poll every independently tracked physical target.
    RefreshAll,
    /// Poll only the exact identity, device generation, and native locator.
    RefreshTarget(StorageDeviceTarget),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmartControlRequest {
    StartSelfTest(SmartSelfTestIntent),
    /// Stop local observation without claiming that the drive-side test was
    /// aborted. A future native abort operation needs its own provider method.
    StopTracking(StorageDeviceTarget),
}

bind_request_capability!(StorageHealthRequest, CapabilityId::STORAGE_HEALTH);

impl CapabilityRequest for SmartObservationRequest {
    const CAPABILITY: CapabilityId = CapabilityId::SMART;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        match self {
            Self::RefreshAll => Ok(RequestTracking::Capability),
            Self::RefreshTarget(target) => {
                storage_target_scope(target).map(RequestTracking::Target)
            }
        }
    }
}

impl CapabilityRequest for SmartControlRequest {
    const CAPABILITY: CapabilityId = CapabilityId::SMART_CONTROL;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        match self {
            Self::StartSelfTest(intent) => storage_target_scope(&intent.target()),
            Self::StopTracking(target) => storage_target_scope(target),
        }
        .map(RequestTracking::Target)
    }
}

fn storage_target_scope(
    target: &StorageDeviceTarget,
) -> Result<RequestScope, RequestTrackingError> {
    let device_id = target.device_id.as_str();
    if device_id.is_empty() || !target.device_generation.is_valid() {
        return Err(RequestTrackingError::MissingTargetIdentity);
    }
    RequestScope::try_owned(format!(
        "{}:{device_id}:{}",
        device_id.len(),
        target.device_generation.get()
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmartTrackingEndReason {
    Requested,
    Expired,
    IdentityChanged,
    SupersededJob,
    DeviceGenerationChanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartTrackingEnd {
    pub target: StorageDeviceTarget,
    pub reason: SmartTrackingEndReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartObservationIssue {
    pub target: StorageDeviceTarget,
    pub failure: FailureKind,
}

/// Monotonic revision of the target-keyed SMART runtime projection.
///
/// This is neither a physical device generation nor a job generation. It lets
/// consumers reject an older full-state batch that crossed a newer control
/// completion in the independently scheduled event lanes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct SmartStateRevision(u64);

impl SmartStateRevision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

/// Complete target-keyed SMART tracking projection after one runtime action.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmartObservationBatch {
    pub revision: SmartStateRevision,
    /// Target directly addressed by this action, when there is one. Consumers
    /// may use it for selection without treating native locator as identity.
    pub subject: Option<StorageDeviceTarget>,
    pub observations: Vec<SmartSelfTestObservation>,
    pub issues: Vec<SmartObservationIssue>,
    pub ended: Vec<SmartTrackingEnd>,
}

#[derive(Clone, Debug)]
pub enum SmartEvent {
    Batch(SmartObservationBatch),
}

impl SmartEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::SMART || capability == &CapabilityId::SMART_CONTROL
    }
}

pub type StorageHealthRequestPort = dyn RequestPort<Request = StorageHealthRequest>;
pub type SmartObservationRequestPort = dyn RequestPort<Request = SmartObservationRequest>;
pub type SmartControlRequestPort = dyn RequestPort<Request = SmartControlRequest>;

/// Independently optional storage-health ports. Filesystem health, SMART,
/// and directory-usage analysis retain separate providers, queues, and
/// availability.
#[derive(Clone, Default)]
pub struct StorageFacets {
    health: Option<Arc<StorageHealthRequestPort>>,
    smart_observation: Option<Arc<SmartObservationRequestPort>>,
    smart_control: Option<Arc<SmartControlRequestPort>>,
    directory_usage: Option<Arc<DirectoryUsageRequestPort>>,
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_facets_storage_tests.rs"]
mod tests;

impl StorageFacets {
    #[must_use]
    pub fn with_health(mut self, port: Arc<StorageHealthRequestPort>) -> Self {
        self.health = Some(port);
        self
    }

    #[must_use]
    pub fn with_smart_observation(mut self, port: Arc<SmartObservationRequestPort>) -> Self {
        self.smart_observation = Some(port);
        self
    }

    #[must_use]
    pub fn with_smart_control(mut self, port: Arc<SmartControlRequestPort>) -> Self {
        self.smart_control = Some(port);
        self
    }

    #[must_use]
    pub fn with_directory_usage(mut self, port: Arc<DirectoryUsageRequestPort>) -> Self {
        self.directory_usage = Some(port);
        self
    }

    #[must_use]
    pub fn health(&self) -> Option<&StorageHealthRequestPort> {
        self.health.as_deref()
    }

    #[must_use]
    pub fn smart_observation(&self) -> Option<&SmartObservationRequestPort> {
        self.smart_observation.as_deref()
    }

    #[must_use]
    pub fn smart_control(&self) -> Option<&SmartControlRequestPort> {
        self.smart_control.as_deref()
    }

    #[must_use]
    pub fn directory_usage(&self) -> Option<&DirectoryUsageRequestPort> {
        self.directory_usage.as_deref()
    }
}

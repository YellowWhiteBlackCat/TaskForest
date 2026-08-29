//! Application-side client composition across independent capability axes.

use std::collections::HashMap;

use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityId, CapabilityRequest, RequestEnvelope, RequestId,
    RequestIdGenerator, RequestPort, SubmissionError, SubmissionErrorKind,
};

use super::{
    PlatformHandle, PowerSupplyRequest, ProcessInsightFacet, ProcessInsightsProjection,
    ProcessInsightsRevision, ProcessListRequest, SensorRequest, ServiceInventoryRequest,
    SessionInventoryRequest, SmartObservationRequest, StartupEvidenceProjection,
    StartupEvidenceRequest, StartupEvidenceRevision, StartupInventoryRequest, StorageHealthRequest,
    SystemTelemetryDomain, SystemTelemetryProjection, SystemTelemetryRevision,
};
use taskmanager_core::core::process::FrozenProcessIdentity;

mod drain;
#[cfg(test)]
#[path = "../../tests/headless/application_platform_client_drain_tests.rs"]
mod drain_tests;
mod environment;
mod handshake;
mod integration;
mod power;
mod process;
mod scheduler;
pub use scheduler::{
    AutomaticSchedule, AutomaticScheduleProfile, automatic_cadence_ms, automatic_schedules,
    default_automatic_cadence_ms,
};
mod sensor;
mod service;
mod startup_projection;
mod storage;
mod system;
mod system_projection;

/// Application-owned client that allocates request IDs and talks only to
/// independently composable platform facets.
pub struct PlatformClient {
    handle: PlatformHandle,
    request_ids: RequestIdGenerator,
    process_insights_projection: ProcessInsightsProjection,
    process_insight_requests: HashMap<RequestId, PendingProcessInsightRequest>,
    process_insights_revision: ProcessInsightsRevision,
    system_telemetry_projection: SystemTelemetryProjection,
    system_telemetry_requests: HashMap<RequestId, PendingSystemTelemetryRequest>,
    system_telemetry_revision: SystemTelemetryRevision,
    startup_evidence_projection: StartupEvidenceProjection,
    startup_evidence_requests: HashMap<RequestId, StartupEvidenceRevision>,
    startup_evidence_revision: StartupEvidenceRevision,
}

#[derive(Clone)]
struct PendingProcessInsightRequest {
    target: FrozenProcessIdentity,
    revision: ProcessInsightsRevision,
    facet: ProcessInsightFacet,
}

#[derive(Clone, Copy)]
struct PendingSystemTelemetryRequest {
    revision: SystemTelemetryRevision,
    domain: SystemTelemetryDomain,
}

impl PlatformClient {
    #[must_use]
    pub fn new(handle: PlatformHandle) -> Self {
        Self {
            handle,
            request_ids: RequestIdGenerator::default(),
            process_insights_projection: ProcessInsightsProjection::default(),
            process_insight_requests: HashMap::new(),
            process_insights_revision: ProcessInsightsRevision::default(),
            system_telemetry_projection: SystemTelemetryProjection::default(),
            system_telemetry_requests: HashMap::new(),
            system_telemetry_revision: SystemTelemetryRevision::default(),
            startup_evidence_projection: StartupEvidenceProjection::default(),
            startup_evidence_requests: HashMap::new(),
            startup_evidence_revision: StartupEvidenceRevision::default(),
        }
    }

    #[must_use]
    pub fn capabilities(&self) -> &dyn CapabilityCatalog {
        self.handle.capabilities()
    }

    pub fn request_refresh(
        &mut self,
        request: crate::RefreshRequest,
        submitted_at_ms: u64,
    ) -> Vec<Result<RequestId, SubmissionError>> {
        match request {
            crate::RefreshRequest::Dashboard => {
                let mut outcomes = self.system_refresh_results(submitted_at_ms);
                outcomes.extend([
                    self.submit_process_list(ProcessListRequest::Refresh, submitted_at_ms),
                    self.submit_storage_health(StorageHealthRequest::Refresh, submitted_at_ms),
                    self.submit_sensor(SensorRequest::Refresh, submitted_at_ms),
                    self.submit_power_supply(PowerSupplyRequest::Refresh, submitted_at_ms),
                    self.submit_smart_observation(
                        SmartObservationRequest::RefreshAll,
                        submitted_at_ms,
                    ),
                ]);
                outcomes
            }
            crate::RefreshRequest::All => {
                let mut outcomes = self.system_refresh_results(submitted_at_ms);
                outcomes.extend([
                    self.submit_hardware_inventory(submitted_at_ms),
                    self.submit_container_rollup(submitted_at_ms),
                    self.submit_process_list(ProcessListRequest::Refresh, submitted_at_ms),
                    self.submit_service_inventory(
                        ServiceInventoryRequest::Refresh,
                        submitted_at_ms,
                    ),
                    self.submit_startup_inventory(
                        StartupInventoryRequest::Refresh,
                        submitted_at_ms,
                    ),
                    self.submit_startup_evidence(StartupEvidenceRequest::Refresh, submitted_at_ms),
                    self.submit_session_inventory(
                        SessionInventoryRequest::Refresh,
                        submitted_at_ms,
                    ),
                    self.submit_storage_health(StorageHealthRequest::Refresh, submitted_at_ms),
                    self.submit_sensor(SensorRequest::Refresh, submitted_at_ms),
                    self.submit_power_supply(PowerSupplyRequest::Refresh, submitted_at_ms),
                    self.submit_smart_observation(
                        SmartObservationRequest::RefreshAll,
                        submitted_at_ms,
                    ),
                ]);
                outcomes
            }
            crate::RefreshRequest::Telemetry => self.system_refresh_results(submitted_at_ms),
            crate::RefreshRequest::HardwareInventory => {
                vec![self.submit_hardware_inventory(submitted_at_ms)]
            }
            crate::RefreshRequest::Containers => {
                vec![self.submit_container_rollup(submitted_at_ms)]
            }
            crate::RefreshRequest::Processes => {
                vec![self.submit_process_list(ProcessListRequest::Refresh, submitted_at_ms)]
            }
            crate::RefreshRequest::PlatformLists => vec![
                self.submit_service_inventory(ServiceInventoryRequest::Refresh, submitted_at_ms),
                self.submit_startup_inventory(StartupInventoryRequest::Refresh, submitted_at_ms),
                self.submit_session_inventory(SessionInventoryRequest::Refresh, submitted_at_ms),
            ],
            crate::RefreshRequest::Services => vec![
                self.submit_service_inventory(ServiceInventoryRequest::Refresh, submitted_at_ms),
            ],
            crate::RefreshRequest::Startup => vec![
                self.submit_startup_inventory(StartupInventoryRequest::Refresh, submitted_at_ms),
                self.submit_startup_evidence(StartupEvidenceRequest::Refresh, submitted_at_ms),
            ],
            crate::RefreshRequest::Sessions => vec![
                self.submit_session_inventory(SessionInventoryRequest::Refresh, submitted_at_ms),
            ],
            crate::RefreshRequest::Health => vec![
                self.submit_storage_health(StorageHealthRequest::Refresh, submitted_at_ms),
                self.submit_sensor(SensorRequest::Refresh, submitted_at_ms),
                self.submit_power_supply(PowerSupplyRequest::Refresh, submitted_at_ms),
                self.submit_smart_observation(SmartObservationRequest::RefreshAll, submitted_at_ms),
            ],
            crate::RefreshRequest::Power => {
                vec![self.submit_power_supply(PowerSupplyRequest::Refresh, submitted_at_ms)]
            }
        }
    }
}

fn submit_request<T: CapabilityRequest>(
    id: RequestId,
    port: Option<&dyn RequestPort<Request = T>>,
    submitted_at_ms: u64,
    payload: T,
) -> Result<(), SubmissionError> {
    let capability = T::CAPABILITY.clone();
    let Some(port) = port else {
        return Err(missing_capability(capability));
    };
    port.try_submit(RequestEnvelope {
        id,
        capability,
        submitted_at_ms,
        payload,
    })
}

fn missing_capability(capability: CapabilityId) -> SubmissionError {
    SubmissionError {
        capability,
        kind: SubmissionErrorKind::UnsupportedCapability,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/application_platform_client_tests.rs"]
mod tests;

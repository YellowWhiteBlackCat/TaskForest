//! Application bridge from ECS capability plans to typed business requests.
//!
//! The runtime never constructs application payloads. This module is the one
//! translation point that turns a claimed capability into the existing
//! revision-aware request method, so periodic work follows the same reducer,
//! correlation, and failure path as a user-triggered refresh.

use taskmanager_platform_contract::{
    CapabilityId, CapabilityRecoveryOutcome, CapabilityRecoveryTrigger, RequestId, SubmissionError,
    SubmissionErrorKind,
};

use super::PlatformClient;
use crate::platform::{
    NpuInventoryRequest, PowerSupplyRequest, ProcessListRequest, SensorRequest,
    ServiceInventoryRequest, SessionInventoryRequest, SmartObservationRequest,
    StartupEvidenceRequest, StartupInventoryRequest, StorageHealthRequest, SystemTelemetryDomain,
};

const TELEMETRY_CADENCE_MS: u64 = 1_000;
const INVENTORY_CADENCE_MS: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScheduledCapability {
    System(SystemTelemetryDomain),
    HardwareInventory,
    Containers,
    Processes,
    Services,
    Startup,
    StartupEvidence,
    Sessions,
    StorageHealth,
    Smart,
    Sensors,
    PowerSupplies,
    NpuInventory,
}

/// Closed automatic-scheduling shape selected by the composition owner.
/// Frontends use the complete product registry; the history session records
/// only facts that feed persisted history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutomaticScheduleProfile {
    #[default]
    FullProduct,
    ContinuousHistory,
}

impl ScheduledCapability {
    const ALL: [Self; 18] = [
        Self::System(SystemTelemetryDomain::Host),
        Self::System(SystemTelemetryDomain::Cpu),
        Self::System(SystemTelemetryDomain::Memory),
        Self::System(SystemTelemetryDomain::Storage),
        Self::System(SystemTelemetryDomain::Network),
        Self::System(SystemTelemetryDomain::Gpu),
        Self::Processes,
        Self::HardwareInventory,
        Self::Containers,
        Self::Services,
        Self::Startup,
        Self::StartupEvidence,
        Self::Sessions,
        Self::StorageHealth,
        Self::Smart,
        Self::Sensors,
        Self::PowerSupplies,
        Self::NpuInventory,
    ];

    fn from_capability(capability: &CapabilityId) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|scheduled| scheduled.capability() == *capability)
    }

    const fn capability(self) -> CapabilityId {
        match self {
            Self::System(domain) => domain.capability(),
            Self::HardwareInventory => CapabilityId::HARDWARE_INVENTORY,
            Self::Containers => CapabilityId::CONTAINERS,
            Self::Processes => CapabilityId::PROCESS_LIST,
            Self::Services => CapabilityId::SERVICES,
            Self::Startup => CapabilityId::STARTUP,
            Self::StartupEvidence => CapabilityId::STARTUP_EVIDENCE,
            Self::Sessions => CapabilityId::SESSIONS,
            Self::StorageHealth => CapabilityId::STORAGE_HEALTH,
            Self::Smart => CapabilityId::SMART,
            Self::Sensors => CapabilityId::SENSORS,
            Self::PowerSupplies => CapabilityId::POWER_SUPPLIES,
            Self::NpuInventory => CapabilityId::ACCELERATOR_NPU,
        }
    }

    const fn cadence_ms(self) -> u64 {
        match self {
            Self::System(_) | Self::Processes => TELEMETRY_CADENCE_MS,
            Self::HardwareInventory
            | Self::Containers
            | Self::Services
            | Self::Startup
            | Self::StartupEvidence
            | Self::Sessions
            | Self::StorageHealth
            | Self::Smart
            | Self::Sensors
            | Self::PowerSupplies
            | Self::NpuInventory => INVENTORY_CADENCE_MS,
        }
    }

    const fn enabled_in(self, profile: AutomaticScheduleProfile) -> bool {
        match profile {
            AutomaticScheduleProfile::FullProduct => true,
            AutomaticScheduleProfile::ContinuousHistory => matches!(
                self,
                Self::System(_) | Self::Processes | Self::Sensors | Self::PowerSupplies
            ),
        }
    }
}

/// One product-owned automatic scheduling entry shared by native route
/// construction and the typed application dispatcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomaticSchedule {
    pub capability: CapabilityId,
    pub cadence_ms: u64,
}

/// Iterate the closed automatic scheduling registry. Manual capabilities are
/// deliberately absent and remain available only through explicit requests.
pub fn automatic_schedules() -> impl ExactSizeIterator<Item = AutomaticSchedule> {
    ScheduledCapability::ALL
        .into_iter()
        .map(|scheduled| AutomaticSchedule {
            capability: scheduled.capability(),
            cadence_ms: scheduled.cadence_ms(),
        })
}

/// Product scheduling registry consumed by the runtime route builder and the
/// application dispatcher. Manual capabilities return `None`.
#[must_use]
pub fn automatic_cadence_ms(capability: &CapabilityId) -> Option<u64> {
    ScheduledCapability::from_capability(capability).map(ScheduledCapability::cadence_ms)
}

impl PlatformClient {
    /// Select the automatic capability set owned by this composition.
    /// Disabled routes remain available for explicit typed requests; only
    /// their periodic cadence is removed.
    pub fn apply_automatic_schedule_profile(&self, profile: AutomaticScheduleProfile) {
        let Some(scheduler) = self.handle.scheduler() else {
            return;
        };
        for scheduled in ScheduledCapability::ALL {
            scheduler.set_cadence_ms(
                &scheduled.capability(),
                scheduled
                    .enabled_in(profile)
                    .then(|| scheduled.cadence_ms()),
            );
        }
    }

    /// Make a transiently failed route immediately ready for an explicit
    /// retry. An active or stalled owner is never replaced.
    pub fn retry_capability(&self, capability: &CapabilityId) -> CapabilityRecoveryOutcome {
        self.request_capability_recovery(capability, CapabilityRecoveryTrigger::ExplicitRetry)
    }

    /// Notify the runtime that permissions, dependencies, or device identity
    /// changed and a route waiting on that prerequisite may be probed again.
    pub fn notify_capability_changed(
        &self,
        capability: &CapabilityId,
    ) -> CapabilityRecoveryOutcome {
        self.request_capability_recovery(capability, CapabilityRecoveryTrigger::CapabilityChanged)
    }

    fn request_capability_recovery(
        &self,
        capability: &CapabilityId,
        trigger: CapabilityRecoveryTrigger,
    ) -> CapabilityRecoveryOutcome {
        self.handle
            .scheduler()
            .map_or(CapabilityRecoveryOutcome::UnknownCapability, |scheduler| {
                scheduler.request_recovery(capability, trigger)
            })
    }

    /// Run one runtime-owned ECS scheduling pass and submit every due
    /// capability through the normal typed application request path.
    ///
    /// System telemetry remains one application revision even though ECS
    /// schedules six independent capability entities. A submission failure is
    /// handed back to ECS so a Ready entity cannot strand the scheduler.
    pub fn run_scheduled_refresh(
        &mut self,
        now_ms: u64,
    ) -> Vec<Result<RequestId, SubmissionError>> {
        let Some(scheduler) = self.handle.scheduler() else {
            return Vec::new();
        };
        let planned = scheduler.poll_due(now_ms);
        if planned.is_empty() {
            return Vec::new();
        }

        let system_due: Vec<_> = planned
            .iter()
            .filter(|capability| SystemTelemetryDomain::from_capability(capability).is_some())
            .cloned()
            .collect();
        let mut outcomes = Vec::new();
        if !system_due.is_empty() {
            for (capability, outcome) in self.scheduled_system_refresh_results(&system_due, now_ms)
            {
                if outcome.is_err() {
                    scheduler.mark_submission_failed(&capability, now_ms);
                }
                outcomes.push(outcome);
            }
        }

        for capability in planned {
            if SystemTelemetryDomain::from_capability(&capability).is_some() {
                continue;
            }
            let outcome = self.submit_scheduled_capability(&capability, now_ms);
            if outcome.is_err() {
                scheduler.mark_submission_failed(&capability, now_ms);
            }
            outcomes.push(outcome);
        }
        outcomes
    }

    /// Apply the user-selected telemetry cadence to the ECS routes. Pause is
    /// deliberately not encoded here: it is a UI/read-model decision, while
    /// an interval is runtime scheduling policy.
    pub fn set_telemetry_interval(&self, interval: crate::TelemetryInterval) {
        let Some(scheduler) = self.handle.scheduler() else {
            return;
        };
        let cadence_ms = u64::try_from(interval.duration().as_millis()).unwrap_or(u64::MAX);
        for domain in SystemTelemetryDomain::ALL {
            scheduler.set_cadence_ms(&domain.capability(), Some(cadence_ms));
        }
    }

    /// Apply the configured continuous-history cadence to every high-rate
    /// persisted fact. Slow sensor/power inventories retain their own product
    /// cadence; system domains and per-application process snapshots advance
    /// together.
    pub fn set_history_collection_interval(&self, interval: crate::TelemetryInterval) {
        self.set_telemetry_interval(interval);
        let Some(scheduler) = self.handle.scheduler() else {
            return;
        };
        let cadence_ms = u64::try_from(interval.duration().as_millis()).unwrap_or(u64::MAX);
        scheduler.set_cadence_ms(&CapabilityId::PROCESS_LIST, Some(cadence_ms));
    }

    fn submit_scheduled_capability(
        &mut self,
        capability: &CapabilityId,
        now_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let Some(scheduled) = ScheduledCapability::from_capability(capability) else {
            return Err(SubmissionError {
                capability: capability.clone(),
                kind: SubmissionErrorKind::InvalidRequest,
            });
        };
        match scheduled {
            ScheduledCapability::System(_) => Err(SubmissionError {
                capability: capability.clone(),
                kind: SubmissionErrorKind::InvalidRequest,
            }),
            ScheduledCapability::HardwareInventory => self.submit_hardware_inventory(now_ms),
            ScheduledCapability::Containers => self.submit_container_rollup(now_ms),
            ScheduledCapability::Processes => {
                self.submit_process_list(ProcessListRequest::Refresh, now_ms)
            }
            ScheduledCapability::Services => {
                self.submit_service_inventory(ServiceInventoryRequest::Refresh, now_ms)
            }
            ScheduledCapability::Startup => {
                self.submit_startup_inventory(StartupInventoryRequest::Refresh, now_ms)
            }
            ScheduledCapability::StartupEvidence => {
                self.submit_startup_evidence(StartupEvidenceRequest::Refresh, now_ms)
            }
            ScheduledCapability::Sessions => {
                self.submit_session_inventory(SessionInventoryRequest::Refresh, now_ms)
            }
            ScheduledCapability::StorageHealth => {
                self.submit_storage_health(StorageHealthRequest::Refresh, now_ms)
            }
            ScheduledCapability::Smart => {
                self.submit_smart_observation(SmartObservationRequest::RefreshAll, now_ms)
            }
            ScheduledCapability::Sensors => self.submit_sensor(SensorRequest::Refresh, now_ms),
            ScheduledCapability::PowerSupplies => {
                self.submit_power_supply(PowerSupplyRequest::Refresh, now_ms)
            }
            ScheduledCapability::NpuInventory => {
                self.submit_npu_inventory(NpuInventoryRequest {}, now_ms)
            }
        }
    }
}

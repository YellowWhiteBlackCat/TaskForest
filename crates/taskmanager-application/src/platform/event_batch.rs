//! Bounded groups of correlated domain events drained by a frontend tick.

use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::setup::SetupScriptEvent;
use taskmanager_platform_contract::{
    CapabilityId, EventEnvelope, EventSequence, OperationFailure, ProviderFailure, RequestId,
};

use super::facets::PlatformEventVisitor;
use super::{
    ContainerRollupEvent, DesktopAppearanceEvent, DirectoryUsageEvent, GpuEngineRowsEvent,
    HardwareInventoryEvent, MsrReadoutEvent, NpuInventoryEvent, PlatformEvent, PowerSupplyEvent,
    ProcessAffinityEvent, ProcessEvent, ProjectedProcessInsights, ProjectedStartupEvidence,
    ProjectedSystemTelemetry, RaplPowerEvent, SensorEvent, ServiceEvent, SessionEvent, ShellEvent,
    SmartEvent, SmbiosMemoryEvent, StartupEvent, StartupEvidenceEvent, StorageHealthEvent,
};

mod environment;
mod integration;
mod power;
mod process;
mod sensor;
mod service;
mod storage;
mod system;

pub use environment::{CorrelatedSessionEvent, CorrelatedStartupEvent};
pub use integration::{
    CorrelatedDesktopAppearanceEvent, CorrelatedSetupScriptEvent, CorrelatedShellEvent,
};
pub use power::CorrelatedPowerSupplyEvent;
pub use process::{CorrelatedProcessAffinityEvent, CorrelatedProcessEvent};
pub use sensor::CorrelatedSensorEvent;
pub use service::CorrelatedServiceEvent;
pub use storage::{
    CorrelatedDirectoryUsageEvent, CorrelatedSmartEvent, CorrelatedStorageHealthEvent,
};
pub use system::{
    CorrelatedGpuEngineRowsEvent, CorrelatedHardwareInventoryEvent, CorrelatedMsrReadoutEvent,
    CorrelatedNpuInventoryEvent, CorrelatedRaplPowerEvent, CorrelatedSmbiosMemoryEvent,
    CorrelatedSystemTelemetryOutcome,
};

pub type CorrelatedContainerRollupEvent = CorrelatedEvent<ContainerRollupEvent>;

#[derive(Clone, Debug, Default)]
pub struct PlatformEventBatch {
    /// Unified correlated completion stream. Every entry has passed pending
    /// request and domain correlation; provider errors are explicit gaps.
    pub system_telemetry_outcomes: Vec<CorrelatedSystemTelemetryOutcome>,
    /// Projection snapshots emitted after each accepted event or terminal
    /// accepted-provider failure.
    pub system_telemetry_projections: Vec<ProjectedSystemTelemetry>,
    pub hardware_inventory_events: Vec<CorrelatedHardwareInventoryEvent>,
    pub containers_events: Vec<CorrelatedContainerRollupEvent>,
    pub process_events: Vec<CorrelatedProcessEvent>,
    pub process_affinity_events: Vec<CorrelatedProcessAffinityEvent>,
    /// Application-owned partial/current projections. Raw process-facet
    /// events never cross this frontend batch boundary.
    pub process_insight_projections: Vec<ProjectedProcessInsights>,
    pub service_events: Vec<CorrelatedServiceEvent>,
    pub startup_events: Vec<CorrelatedStartupEvent>,
    /// Application-owned startup evidence projections. Raw provider events
    /// terminate inside `PlatformClient` after request/revision correlation.
    pub startup_evidence_projections: Vec<ProjectedStartupEvidence>,
    pub session_events: Vec<CorrelatedSessionEvent>,
    pub shell_events: Vec<CorrelatedShellEvent>,
    pub setup_script_events: Vec<CorrelatedSetupScriptEvent>,
    pub desktop_appearance_events: Vec<CorrelatedDesktopAppearanceEvent>,
    pub storage_health_events: Vec<CorrelatedStorageHealthEvent>,
    pub sensor_events: Vec<CorrelatedSensorEvent>,
    pub power_supply_events: Vec<CorrelatedPowerSupplyEvent>,
    pub smart_events: Vec<CorrelatedSmartEvent>,
    pub directory_usage_events: Vec<CorrelatedDirectoryUsageEvent>,
    pub gpu_engine_rows_events: Vec<CorrelatedGpuEngineRowsEvent>,
    pub npu_inventory_events: Vec<CorrelatedNpuInventoryEvent>,
    pub smbios_memory_events: Vec<CorrelatedSmbiosMemoryEvent>,
    pub rapl_power_events: Vec<CorrelatedRaplPowerEvent>,
    pub msr_readout_events: Vec<CorrelatedMsrReadoutEvent>,
    pub failures: Vec<OperationFailure>,
}

#[derive(Clone, Debug)]
pub struct CorrelatedEvent<T> {
    pub request_id: RequestId,
    pub capability: CapabilityId,
    pub provider: Option<ProviderId>,
    pub sequence: EventSequence,
    pub observed_at_ms: u64,
    pub event: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformEventContext {
    pub request_id: RequestId,
    pub capability: CapabilityId,
    pub provider: Option<ProviderId>,
    pub sequence: EventSequence,
    pub observed_at_ms: u64,
}

impl PlatformEventContext {
    #[must_use]
    pub fn from_envelope<T>(envelope: &EventEnvelope<T>) -> Self {
        Self {
            request_id: envelope.request_id,
            capability: envelope.capability.clone(),
            provider: envelope.provider.clone(),
            sequence: envelope.sequence,
            observed_at_ms: envelope.observed_at_ms,
        }
    }
}

impl<T> CorrelatedEvent<T> {
    #[must_use]
    pub fn new(context: PlatformEventContext, event: T) -> Self {
        Self {
            request_id: context.request_id,
            capability: context.capability,
            provider: context.provider,
            sequence: context.sequence,
            observed_at_ms: context.observed_at_ms,
            event,
        }
    }

    #[must_use]
    pub fn context(&self) -> PlatformEventContext {
        PlatformEventContext {
            request_id: self.request_id,
            capability: self.capability.clone(),
            provider: self.provider.clone(),
            sequence: self.sequence,
            observed_at_ms: self.observed_at_ms,
        }
    }
}

impl PlatformEventBatch {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let Self {
            system_telemetry_outcomes,
            system_telemetry_projections,
            hardware_inventory_events,
            containers_events,
            process_events,
            process_affinity_events,
            process_insight_projections,
            service_events,
            startup_events,
            startup_evidence_projections,
            session_events,
            shell_events,
            setup_script_events,
            desktop_appearance_events,
            storage_health_events,
            sensor_events,
            power_supply_events,
            smart_events,
            directory_usage_events,
            gpu_engine_rows_events,
            npu_inventory_events,
            smbios_memory_events,
            rapl_power_events,
            msr_readout_events,
            failures,
        } = self;
        system_telemetry_outcomes.is_empty()
            && system_telemetry_projections.is_empty()
            && hardware_inventory_events.is_empty()
            && containers_events.is_empty()
            && process_events.is_empty()
            && process_affinity_events.is_empty()
            && process_insight_projections.is_empty()
            && service_events.is_empty()
            && startup_events.is_empty()
            && startup_evidence_projections.is_empty()
            && session_events.is_empty()
            && shell_events.is_empty()
            && setup_script_events.is_empty()
            && desktop_appearance_events.is_empty()
            && storage_health_events.is_empty()
            && sensor_events.is_empty()
            && power_supply_events.is_empty()
            && smart_events.is_empty()
            && directory_usage_events.is_empty()
            && gpu_engine_rows_events.is_empty()
            && npu_inventory_events.is_empty()
            && smbios_memory_events.is_empty()
            && rapl_power_events.is_empty()
            && msr_readout_events.is_empty()
            && failures.is_empty()
    }

    /// Canonicalize order only inside each independently folded domain.
    ///
    /// `EventSequence` is the runtime publication authority, but fair
    /// control/observation delivery is not a global sequence merge. A batch
    /// therefore never assigns cross-domain last-writer semantics. Consumers
    /// may exchange independent domains or declare an explicit fold phase;
    /// within one domain, the oldest correlated publication is always folded
    /// first. Application-owned projections use their typed revision and keep
    /// stable arrival order for multiple partial projections of one revision.
    #[must_use]
    pub fn into_domain_ordered(mut self) -> Self {
        let Self {
            system_telemetry_outcomes,
            system_telemetry_projections,
            hardware_inventory_events,
            containers_events,
            process_events,
            process_affinity_events,
            process_insight_projections,
            service_events,
            startup_events,
            startup_evidence_projections,
            session_events,
            shell_events,
            setup_script_events,
            desktop_appearance_events,
            storage_health_events,
            sensor_events,
            power_supply_events,
            smart_events,
            directory_usage_events,
            gpu_engine_rows_events,
            npu_inventory_events,
            smbios_memory_events,
            rapl_power_events,
            msr_readout_events,
            failures,
        } = &mut self;
        sort_correlated(system_telemetry_outcomes);
        system_telemetry_projections.sort_by_key(|projection| projection.revision);
        sort_correlated(hardware_inventory_events);
        sort_correlated(containers_events);
        sort_correlated(process_events);
        sort_correlated(process_affinity_events);
        process_insight_projections.sort_by_key(|projection| projection.revision);
        sort_correlated(service_events);
        sort_correlated(startup_events);
        startup_evidence_projections.sort_by_key(|projection| projection.revision);
        sort_correlated(session_events);
        sort_correlated(shell_events);
        sort_correlated(setup_script_events);
        sort_correlated(desktop_appearance_events);
        sort_correlated(storage_health_events);
        sort_correlated(sensor_events);
        sort_correlated(power_supply_events);
        sort_correlated(smart_events);
        sort_correlated(directory_usage_events);
        sort_correlated(gpu_engine_rows_events);
        sort_correlated(npu_inventory_events);
        sort_correlated(smbios_memory_events);
        sort_correlated(rapl_power_events);
        sort_correlated(msr_readout_events);
        failures.sort_by_key(|failure| failure.sequence);
        self
    }

    pub(super) fn merge(&mut self, context: PlatformEventContext, event: PlatformEvent) {
        event.visit(&mut BatchEventVisitor {
            batch: self,
            context,
        });
    }
}

fn sort_correlated<T>(events: &mut [CorrelatedEvent<T>]) {
    events.sort_by_key(|event| event.sequence);
}

struct BatchEventVisitor<'a> {
    batch: &'a mut PlatformEventBatch,
    context: PlatformEventContext,
}

impl PlatformEventVisitor for BatchEventVisitor<'_> {
    fn visit_hardware_inventory(&mut self, event: HardwareInventoryEvent) {
        system::push_hardware_inventory(self.batch, self.context.clone(), event);
    }

    fn visit_processes(&mut self, event: ProcessEvent) {
        process::push_processes(self.batch, self.context.clone(), event);
    }

    fn visit_process_affinity(&mut self, event: ProcessAffinityEvent) {
        process::push_process_affinity(self.batch, self.context.clone(), event);
    }

    fn visit_services(&mut self, event: ServiceEvent) {
        service::push_services(self.batch, self.context.clone(), event);
    }

    fn visit_startup(&mut self, event: StartupEvent) {
        environment::push_startup(self.batch, self.context.clone(), event);
    }

    fn visit_startup_evidence(&mut self, event: StartupEvidenceEvent) {
        let _ = event;
        let kind = taskmanager_core::FailureKind::ProviderFault;
        self.batch.failures.push(OperationFailure {
            request_id: self.context.request_id,
            capability: self.context.capability.clone(),
            sequence: self.context.sequence,
            kind,
            retry: ProviderFailure::from_kind(kind).retry(),
            provider: self.context.provider.clone(),
            observed_at_ms: self.context.observed_at_ms,
        });
    }

    fn visit_sessions(&mut self, event: SessionEvent) {
        environment::push_sessions(self.batch, self.context.clone(), event);
    }

    fn visit_shell(&mut self, event: ShellEvent) {
        integration::push_shell(self.batch, self.context.clone(), event);
    }

    fn visit_setup_script(&mut self, event: SetupScriptEvent) {
        integration::push_setup_script(self.batch, self.context.clone(), event);
    }

    fn visit_desktop_appearance(&mut self, event: DesktopAppearanceEvent) {
        integration::push_desktop_appearance(self.batch, self.context.clone(), event);
    }

    fn visit_storage_health(&mut self, event: StorageHealthEvent) {
        storage::push_storage_health(self.batch, self.context.clone(), event);
    }

    fn visit_sensors(&mut self, event: SensorEvent) {
        sensor::push_sensors(self.batch, self.context.clone(), event);
    }

    fn visit_power_supplies(&mut self, event: PowerSupplyEvent) {
        power::push_power_supplies(self.batch, self.context.clone(), event);
    }

    fn visit_smart(&mut self, event: SmartEvent) {
        storage::push_smart(self.batch, self.context.clone(), event);
    }

    fn visit_containers(&mut self, event: ContainerRollupEvent) {
        system::push_containers(self.batch, self.context.clone(), event);
    }

    fn visit_directory_usage(&mut self, event: DirectoryUsageEvent) {
        storage::push_directory_usage(self.batch, self.context.clone(), event);
    }

    fn visit_gpu_engine_rows(&mut self, event: GpuEngineRowsEvent) {
        system::push_gpu_engine_rows(self.batch, self.context.clone(), event);
    }

    fn visit_npu_inventory(&mut self, event: NpuInventoryEvent) {
        system::push_npu_inventory(self.batch, self.context.clone(), event);
    }

    fn visit_smbios_memory(&mut self, event: SmbiosMemoryEvent) {
        system::push_smbios_memory(self.batch, self.context.clone(), event);
    }

    fn visit_rapl_power(&mut self, event: RaplPowerEvent) {
        system::push_rapl_power(self.batch, self.context.clone(), event);
    }

    fn visit_msr_readout(&mut self, event: MsrReadoutEvent) {
        system::push_msr_readout(self.batch, self.context.clone(), event);
    }
}

#[cfg(test)]
#[path = "../../tests/common/platform_event_batch.rs"]
mod test_support;

#[cfg(test)]
#[path = "../../tests/headless/application_platform_event_batch_tests.rs"]
mod tests;

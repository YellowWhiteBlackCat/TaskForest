//! Request and event contracts grouped by independent capability change axis.

use taskmanager_core::core::setup::SetupScriptEvent;
use taskmanager_platform_contract::{CapabilityId, EventPort};

/// Bind one application request DTO to exactly one platform-neutral
/// capability. The runtime consumes this association at the typed lane
/// boundary, so native adapters never repeat it.
macro_rules! bind_request_capability {
    ($request:ty, $capability:expr) => {
        impl taskmanager_platform_contract::CapabilityRequest for $request {
            const CAPABILITY: taskmanager_platform_contract::CapabilityId = $capability;
        }
    };
}

fn opaque_target_tracking(
    target: &str,
) -> Result<
    taskmanager_platform_contract::RequestTracking,
    taskmanager_platform_contract::RequestTrackingError,
> {
    taskmanager_platform_contract::RequestScope::try_from_str(target)
        .map(taskmanager_platform_contract::RequestTracking::Target)
}

mod directory_usage;
mod environment;
mod gpu_engine_rows;
mod integration;
mod msr_readout;
mod npu_inventory;
mod power;
mod process;
mod rapl_power;
mod sensor;
mod service;
mod smbios_memory;
mod storage;
mod system;

pub use directory_usage::*;
pub use environment::*;
pub use gpu_engine_rows::*;
pub use integration::*;
pub use msr_readout::*;
pub use npu_inventory::*;
pub use power::*;
pub use process::*;
pub use rapl_power::*;
pub use sensor::*;
pub use service::*;
pub use smbios_memory::*;
pub use storage::*;
pub use system::*;

#[derive(Clone, Debug)]
pub enum PlatformEvent {
    SystemTelemetry(SystemTelemetryDomainEvent),
    HardwareInventory(HardwareInventoryEvent),
    Processes(ProcessEvent),
    ProcessAffinity(ProcessAffinityEvent),
    ProcessInsightFacet(ProcessInsightFacetEvent),
    Services(ServiceEvent),
    Startup(StartupEvent),
    StartupEvidence(StartupEvidenceEvent),
    Sessions(SessionEvent),
    Shell(ShellEvent),
    SetupScript(SetupScriptEvent),
    DesktopAppearance(DesktopAppearanceEvent),
    StorageHealth(StorageHealthEvent),
    Sensors(SensorEvent),
    PowerSupplies(PowerSupplyEvent),
    Smart(SmartEvent),
    Containers(ContainerRollupEvent),
    DirectoryUsage(DirectoryUsageEvent),
    GpuEngineRows(GpuEngineRowsEvent),
    NpuInventory(NpuInventoryEvent),
    SmbiosMemory(SmbiosMemoryEvent),
    RaplPower(RaplPowerEvent),
    MsrReadout(MsrReadoutEvent),
}

pub(crate) trait PlatformEventVisitor {
    fn visit_hardware_inventory(&mut self, event: HardwareInventoryEvent);
    fn visit_processes(&mut self, event: ProcessEvent);
    fn visit_process_affinity(&mut self, event: ProcessAffinityEvent);
    fn visit_services(&mut self, event: ServiceEvent);
    fn visit_startup(&mut self, event: StartupEvent);
    fn visit_startup_evidence(&mut self, event: StartupEvidenceEvent);
    fn visit_sessions(&mut self, event: SessionEvent);
    fn visit_shell(&mut self, event: ShellEvent);
    fn visit_setup_script(&mut self, event: SetupScriptEvent);
    fn visit_desktop_appearance(&mut self, event: DesktopAppearanceEvent);
    fn visit_storage_health(&mut self, event: StorageHealthEvent);
    fn visit_sensors(&mut self, event: SensorEvent);
    fn visit_power_supplies(&mut self, event: PowerSupplyEvent);
    fn visit_smart(&mut self, event: SmartEvent);
    fn visit_containers(&mut self, event: ContainerRollupEvent);
    fn visit_directory_usage(&mut self, event: DirectoryUsageEvent);
    fn visit_gpu_engine_rows(&mut self, event: GpuEngineRowsEvent);
    fn visit_npu_inventory(&mut self, event: NpuInventoryEvent);
    fn visit_smbios_memory(&mut self, event: SmbiosMemoryEvent);
    fn visit_rapl_power(&mut self, event: RaplPowerEvent);
    fn visit_msr_readout(&mut self, event: MsrReadoutEvent);
}

impl PlatformEvent {
    /// Validate that a successful payload belongs to the capability carried by
    /// its envelope.
    ///
    /// Native runtimes enforce this before publishing, and `PlatformClient`
    /// repeats the check for adapters that implement `EventPort` directly.
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        match self {
            Self::SystemTelemetry(event) => event.accepts_capability(capability),
            Self::HardwareInventory(event) => event.accepts_capability(capability),
            Self::Processes(event) => event.accepts_capability(capability),
            Self::ProcessAffinity(event) => event.accepts_capability(capability),
            Self::ProcessInsightFacet(event) => event.accepts_capability(capability),
            Self::Services(event) => event.accepts_capability(capability),
            Self::Startup(event) => event.accepts_capability(capability),
            Self::StartupEvidence(event) => event.accepts_capability(capability),
            Self::Sessions(event) => event.accepts_capability(capability),
            Self::Shell(event) => event.accepts_capability(capability),
            Self::SetupScript(_) => capability == &CapabilityId::FIRST_RUN_SETUP,
            Self::DesktopAppearance(event) => event.accepts_capability(capability),
            Self::StorageHealth(event) => event.accepts_capability(capability),
            Self::Sensors(event) => event.accepts_capability(capability),
            Self::PowerSupplies(event) => event.accepts_capability(capability),
            Self::Smart(event) => event.accepts_capability(capability),
            Self::Containers(event) => event.accepts_capability(capability),
            Self::DirectoryUsage(event) => event.accepts_capability(capability),
            Self::GpuEngineRows(event) => event.accepts_capability(capability),
            Self::NpuInventory(event) => event.accepts_capability(capability),
            Self::SmbiosMemory(event) => event.accepts_capability(capability),
            Self::RaplPower(event) => event.accepts_capability(capability),
            Self::MsrReadout(event) => event.accepts_capability(capability),
        }
    }

    pub(crate) fn visit(self, visitor: &mut impl PlatformEventVisitor) {
        match self {
            // Domain telemetry is consumed into outcomes/projections by
            // PlatformClient before non-projection events reach this visitor.
            Self::SystemTelemetry(_) => {}
            Self::HardwareInventory(event) => visitor.visit_hardware_inventory(event),
            Self::Processes(event) => visitor.visit_processes(event),
            Self::ProcessAffinity(event) => visitor.visit_process_affinity(event),
            // Raw facet events are consumed by the application projection
            // reducer before a frontend batch is constructed.
            Self::ProcessInsightFacet(_) => {}
            Self::Services(event) => visitor.visit_services(event),
            Self::Startup(event) => visitor.visit_startup(event),
            Self::StartupEvidence(event) => visitor.visit_startup_evidence(event),
            Self::Sessions(event) => visitor.visit_sessions(event),
            Self::Shell(event) => visitor.visit_shell(event),
            Self::SetupScript(event) => visitor.visit_setup_script(event),
            Self::DesktopAppearance(event) => visitor.visit_desktop_appearance(event),
            Self::StorageHealth(event) => visitor.visit_storage_health(event),
            Self::Sensors(event) => visitor.visit_sensors(event),
            Self::PowerSupplies(event) => visitor.visit_power_supplies(event),
            Self::Smart(event) => visitor.visit_smart(event),
            Self::Containers(event) => visitor.visit_containers(event),
            Self::DirectoryUsage(event) => visitor.visit_directory_usage(event),
            Self::GpuEngineRows(event) => visitor.visit_gpu_engine_rows(event),
            Self::NpuInventory(event) => visitor.visit_npu_inventory(event),
            Self::SmbiosMemory(event) => visitor.visit_smbios_memory(event),
            Self::RaplPower(event) => visitor.visit_rapl_power(event),
            Self::MsrReadout(event) => visitor.visit_msr_readout(event),
        }
    }
}

pub type PlatformEventPort = dyn EventPort<Event = PlatformEvent>;

#[cfg(test)]
#[path = "../../tests/headless/application_platform_facets_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/application_request_tracking_tests.rs"]
mod request_tracking_tests;

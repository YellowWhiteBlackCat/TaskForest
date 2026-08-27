//! Immutable composition of platform capability facets.

use std::any::Any;
use std::sync::Arc;

use taskmanager_platform_contract::{CapabilityCatalog, CapabilityScheduler};

use super::{
    CommandLaunchRequestPort, CpuTelemetryRequestPort, DesktopAppearanceRequestPort,
    EnvironmentFacets, GpuTelemetryRequestPort, HardwareInventoryRequestPort,
    HostTelemetryRequestPort, IntegrationFacets, MemoryTelemetryRequestPort,
    NetworkTelemetryRequestPort, PlatformEventPort, PowerFacets, PowerSupplyRequestPort,
    ProcessAffinityControlRequestPort, ProcessAffinityRequestPort, ProcessControlRequestPort,
    ProcessEnvironmentRequestPort, ProcessFacets, ProcessGpuRequestPort,
    ProcessIsolationRequestPort, ProcessListRequestPort, ProcessNetworkEscalationRequestPort,
    ProcessNetworkRequestPort, ProcessOpenFilesRequestPort, ProcessResourceControlRequestPort,
    ProcessResourcesRequestPort, ProcessThreadsRequestPort, ResourceRevealRequestPort,
    SensorFacets, SensorRequestPort, ServiceControlRequestPort, ServiceDependenciesRequestPort,
    ServiceFacets, ServiceInventoryRequestPort, ServiceLogSnapshotRequestPort,
    ServiceLogStreamRequestPort, SessionControlRequestPort, SessionInventoryRequestPort,
    SetupScriptRequestPort, SmartControlRequestPort, SmartObservationRequestPort,
    StartupControlRequestPort, StartupEvidenceRequestPort, StartupInventoryRequestPort,
    StorageFacets, StorageHealthRequestPort, StorageTelemetryRequestPort, SystemFacets,
    UrlOpenRequestPort,
};

/// Domain-grouped construction value for independently optional capability
/// ports.
///
/// A group is not a runtime queue or aggregate provider. Every port inside it
/// retains its own request type, availability, provider, and bounded execution
/// lane. Grouping only keeps adapter composition stable as capabilities grow.
#[derive(Clone, Default)]
pub struct PlatformFacets {
    system: SystemFacets,
    process: ProcessFacets,
    service: ServiceFacets,
    environment: EnvironmentFacets,
    integration: IntegrationFacets,
    storage: StorageFacets,
    sensor: SensorFacets,
    power: PowerFacets,
}

impl PlatformFacets {
    #[must_use]
    pub fn with_system(mut self, facets: SystemFacets) -> Self {
        self.system = facets;
        self
    }

    #[must_use]
    pub fn with_process(mut self, facets: ProcessFacets) -> Self {
        self.process = facets;
        self
    }

    #[must_use]
    pub fn with_service(mut self, facets: ServiceFacets) -> Self {
        self.service = facets;
        self
    }

    #[must_use]
    pub fn with_environment(mut self, facets: EnvironmentFacets) -> Self {
        self.environment = facets;
        self
    }

    #[must_use]
    pub fn with_integration(mut self, facets: IntegrationFacets) -> Self {
        self.integration = facets;
        self
    }

    #[must_use]
    pub fn with_storage(mut self, facets: StorageFacets) -> Self {
        self.storage = facets;
        self
    }

    #[must_use]
    pub fn with_sensor(mut self, facets: SensorFacets) -> Self {
        self.sensor = facets;
        self
    }

    #[must_use]
    pub fn with_power(mut self, facets: PowerFacets) -> Self {
        self.power = facets;
        self
    }

    #[must_use]
    pub const fn system(&self) -> &SystemFacets {
        &self.system
    }

    #[must_use]
    pub const fn process(&self) -> &ProcessFacets {
        &self.process
    }

    #[must_use]
    pub const fn service(&self) -> &ServiceFacets {
        &self.service
    }

    #[must_use]
    pub const fn environment(&self) -> &EnvironmentFacets {
        &self.environment
    }

    #[must_use]
    pub const fn integration(&self) -> &IntegrationFacets {
        &self.integration
    }

    #[must_use]
    pub const fn storage(&self) -> &StorageFacets {
        &self.storage
    }

    #[must_use]
    pub const fn sensor(&self) -> &SensorFacets {
        &self.sensor
    }

    #[must_use]
    pub const fn power(&self) -> &PowerFacets {
        &self.power
    }
}

/// Runtime platform composition assembled from independent capability facets.
#[derive(Clone)]
pub struct PlatformHandle {
    capabilities: Arc<dyn CapabilityCatalog>,
    scheduler: Option<Arc<dyn CapabilityScheduler>>,
    events: Arc<PlatformEventPort>,
    facets: PlatformFacets,
    // Declared last so request ports are released before the runtime owner.
    // The application deliberately knows nothing about the native owner type.
    runtime_lifetime: Option<Arc<dyn Any + Send + Sync>>,
}

impl PlatformHandle {
    #[must_use]
    pub fn new(
        capabilities: Arc<dyn CapabilityCatalog>,
        events: Arc<PlatformEventPort>,
        facets: PlatformFacets,
    ) -> Self {
        Self {
            capabilities,
            scheduler: None,
            events,
            facets,
            runtime_lifetime: None,
        }
    }

    /// Attach the runtime-owned scheduler without making scheduling a
    /// prerequisite for test-only or absent platform handles.
    #[must_use]
    pub fn with_scheduler(mut self, scheduler: Arc<dyn CapabilityScheduler>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    /// Keep an opaque native runtime owner alive for exactly as long as the
    /// last clone of this handle.
    #[must_use]
    pub fn with_runtime_lifetime<T>(mut self, owner: T) -> Self
    where
        T: Any + Send + Sync,
    {
        self.runtime_lifetime = Some(Arc::new(owner));
        self
    }

    #[must_use]
    pub fn capabilities(&self) -> &dyn CapabilityCatalog {
        self.capabilities.as_ref()
    }

    #[must_use]
    pub fn scheduler(&self) -> Option<Arc<dyn CapabilityScheduler>> {
        self.scheduler.as_ref().map(Arc::clone)
    }

    #[must_use]
    pub fn events(&self) -> &PlatformEventPort {
        self.events.as_ref()
    }

    #[must_use]
    pub const fn facets(&self) -> &PlatformFacets {
        &self.facets
    }

    #[must_use]
    pub fn host_telemetry(&self) -> Option<&HostTelemetryRequestPort> {
        self.facets.system().host()
    }

    #[must_use]
    pub fn cpu_telemetry(&self) -> Option<&CpuTelemetryRequestPort> {
        self.facets.system().cpu()
    }

    #[must_use]
    pub fn memory_telemetry(&self) -> Option<&MemoryTelemetryRequestPort> {
        self.facets.system().memory()
    }

    #[must_use]
    pub fn storage_telemetry(&self) -> Option<&StorageTelemetryRequestPort> {
        self.facets.system().storage()
    }

    #[must_use]
    pub fn network_telemetry(&self) -> Option<&NetworkTelemetryRequestPort> {
        self.facets.system().network()
    }

    #[must_use]
    pub fn gpu_telemetry(&self) -> Option<&GpuTelemetryRequestPort> {
        self.facets.system().gpu()
    }

    #[must_use]
    pub fn hardware_inventory(&self) -> Option<&HardwareInventoryRequestPort> {
        self.facets.system().hardware_inventory()
    }

    #[must_use]
    pub fn process_list(&self) -> Option<&ProcessListRequestPort> {
        self.facets.process().list()
    }

    #[must_use]
    pub fn process_control(&self) -> Option<&ProcessControlRequestPort> {
        self.facets.process().control()
    }

    #[must_use]
    pub fn process_network(&self) -> Option<&ProcessNetworkRequestPort> {
        self.facets.process().network()
    }

    #[must_use]
    pub fn process_gpu(&self) -> Option<&ProcessGpuRequestPort> {
        self.facets.process().gpu()
    }

    #[must_use]
    pub fn process_resources(&self) -> Option<&ProcessResourcesRequestPort> {
        self.facets.process().resources()
    }

    #[must_use]
    pub fn process_isolation(&self) -> Option<&ProcessIsolationRequestPort> {
        self.facets.process().isolation()
    }

    #[must_use]
    pub fn process_threads(&self) -> Option<&ProcessThreadsRequestPort> {
        self.facets.process().threads()
    }

    #[must_use]
    pub fn process_open_files(&self) -> Option<&ProcessOpenFilesRequestPort> {
        self.facets.process().open_files()
    }

    #[must_use]
    pub fn process_environment(&self) -> Option<&ProcessEnvironmentRequestPort> {
        self.facets.process().environment()
    }

    #[must_use]
    pub fn process_affinity(&self) -> Option<&ProcessAffinityRequestPort> {
        self.facets.process().affinity()
    }

    #[must_use]
    pub fn process_affinity_control(&self) -> Option<&ProcessAffinityControlRequestPort> {
        self.facets.process().affinity_control()
    }

    #[must_use]
    pub fn process_resource_control(&self) -> Option<&ProcessResourceControlRequestPort> {
        self.facets.process().resource_control()
    }

    #[must_use]
    pub fn process_network_escalation(&self) -> Option<&ProcessNetworkEscalationRequestPort> {
        self.facets.process().network_escalation()
    }

    #[must_use]
    pub fn service_inventory(&self) -> Option<&ServiceInventoryRequestPort> {
        self.facets.service().inventory()
    }

    #[must_use]
    pub fn service_dependencies(&self) -> Option<&ServiceDependenciesRequestPort> {
        self.facets.service().dependencies()
    }

    #[must_use]
    pub fn service_control(&self) -> Option<&ServiceControlRequestPort> {
        self.facets.service().control()
    }

    #[must_use]
    pub fn service_log_snapshot(&self) -> Option<&ServiceLogSnapshotRequestPort> {
        self.facets.service().log_snapshot()
    }

    #[must_use]
    pub fn service_log_stream(&self) -> Option<&ServiceLogStreamRequestPort> {
        self.facets.service().log_stream()
    }

    #[must_use]
    pub fn startup_inventory(&self) -> Option<&StartupInventoryRequestPort> {
        self.facets.environment().startup_inventory()
    }

    #[must_use]
    pub fn startup_evidence(&self) -> Option<&StartupEvidenceRequestPort> {
        self.facets.environment().startup_evidence()
    }

    #[must_use]
    pub fn startup_control(&self) -> Option<&StartupControlRequestPort> {
        self.facets.environment().startup_control()
    }

    #[must_use]
    pub fn session_inventory(&self) -> Option<&SessionInventoryRequestPort> {
        self.facets.environment().session_inventory()
    }

    #[must_use]
    pub fn session_control(&self) -> Option<&SessionControlRequestPort> {
        self.facets.environment().session_control()
    }

    #[must_use]
    pub fn command_launch(&self) -> Option<&CommandLaunchRequestPort> {
        self.facets.integration().command_launch()
    }

    #[must_use]
    pub fn resource_reveal(&self) -> Option<&ResourceRevealRequestPort> {
        self.facets.integration().resource_reveal()
    }

    #[must_use]
    pub fn url_open(&self) -> Option<&UrlOpenRequestPort> {
        self.facets.integration().url_open()
    }

    #[must_use]
    pub fn desktop_appearance(&self) -> Option<&DesktopAppearanceRequestPort> {
        self.facets.integration().desktop_appearance()
    }

    #[must_use]
    pub fn setup_script(&self) -> Option<&SetupScriptRequestPort> {
        self.facets.integration().setup_script()
    }

    #[must_use]
    pub fn storage_health(&self) -> Option<&StorageHealthRequestPort> {
        self.facets.storage().health()
    }

    #[must_use]
    pub fn sensors(&self) -> Option<&SensorRequestPort> {
        self.facets.sensor().observation()
    }

    #[must_use]
    pub fn power_supplies(&self) -> Option<&PowerSupplyRequestPort> {
        self.facets.power().supplies()
    }

    #[must_use]
    pub fn smart_observation(&self) -> Option<&SmartObservationRequestPort> {
        self.facets.storage().smart_observation()
    }

    #[must_use]
    pub fn smart_control(&self) -> Option<&SmartControlRequestPort> {
        self.facets.storage().smart_control()
    }
}

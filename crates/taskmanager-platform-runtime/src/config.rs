//! Explicit native composition inputs: queue bounds, observation and
//! scheduling clocks, and per-capability provider bindings.
//!
//! Bindings are construction data only; an absent provider creates no catalog
//! descriptor, request port, or execution lane.

use std::sync::OnceLock;
use std::time::Instant;

use taskmanager_application::{
    CommandLaunchRequest, ContainerRollupRequest, CpuTelemetryRequest, DesktopAppearanceRequest,
    DesktopNotificationRequest, DirectoryUsageRequest, GpuEngineRowsRequest, GpuTelemetryRequest,
    HardwareInventoryRequest, HostTelemetryRequest, MemoryTelemetryRequest, MsrReadoutRequest,
    NetworkTelemetryRequest, NpuInventoryRequest, PowerSupplyRequest,
    ProcessAffinityControlRequest, ProcessAffinityRequest, ProcessControlRequest,
    ProcessEnvironmentRequest, ProcessGpuRequest, ProcessIsolationRequest, ProcessListRequest,
    ProcessNetworkEscalationRequest, ProcessNetworkRequest, ProcessOpenFilesRequest,
    ProcessResourceControlRequest, ProcessResourcesRequest, ProcessThreadsRequest,
    RaplPowerRequest, ResourceRevealRequest, SensorRequest, ServiceControlRequest,
    ServiceDependenciesRequest, ServiceInventoryRequest, ServiceLogSnapshotRequest,
    ServiceLogStreamRequest, SessionControlRequest, SessionInventoryRequest, SetupScriptRequest,
    SmartControlRequest, SmartObservationRequest, SmbiosMemoryRequest, StartupControlRequest,
    StartupEvidenceRequest, StartupInventoryRequest, StorageHealthRequest, StorageTelemetryRequest,
    UrlOpenRequest,
};
use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{
    CapabilityId, CapabilityRequest, CapabilityStatus, MAX_REQUEST_SCOPE_BYTES, SidebandPolicy,
};

use crate::registration::ProviderBinding;

static MONOTONIC_EPOCH: OnceLock<Instant> = OnceLock::new();

/// Process-local monotonic milliseconds for scheduling and timeout decisions.
///
/// The value has no wall-clock meaning and must never be published as an
/// observation timestamp. Its only contract is monotonic elapsed time within
/// this process.
#[must_use]
pub fn monotonic_clock_ms() -> u64 {
    let elapsed = MONOTONIC_EPOCH.get_or_init(Instant::now).elapsed();
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// Queue bounds shared by every native adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueCapacities {
    pub observation_requests: usize,
    pub control_requests: usize,
    pub control_events: usize,
    pub observation_events: usize,
}

/// One power-of-two ceiling above the current 45 typed routes, aligned with
/// the default 64 worker-lane ceiling so route growth cannot silently outrun
/// executable lanes.
pub const DEFAULT_RUNTIME_ROUTE_LIMIT: usize = 64;
/// Default control event queue and the corresponding terminal-delivery
/// headroom that observation work is never allowed to consume.
pub const DEFAULT_CONTROL_EVENT_QUEUE_CAPACITY: usize = 32;
/// Four fully occupied per-capability target partitions. The current target
/// surface is smaller; this is explicit extension headroom, not an unbounded
/// multiplier of route count.
pub const DEFAULT_ACTIVE_TARGET_LIMIT: usize = 4 * DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY;
pub const DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY: usize = 64;
/// Two full target-heavy capabilities may coexist inside one typed domain.
pub const DEFAULT_ACTIVE_TARGET_LIMIT_PER_DOMAIN: usize =
    2 * DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY;
/// Every retained target scope is independently capped at 4 KiB by the
/// platform contract; the global byte ceiling is therefore exact.
pub const DEFAULT_TARGET_SCOPE_BYTE_LIMIT: usize =
    DEFAULT_ACTIVE_TARGET_LIMIT * MAX_REQUEST_SCOPE_BYTES;
/// One undrained terminal for every capability owner plus every global target
/// owner, followed by control headroom equal to the default control event
/// queue. Primary queues remain separately bounded by `QueueCapacities`.
pub const DEFAULT_CONTROL_DELIVERY_RESERVE: usize = DEFAULT_CONTROL_EVENT_QUEUE_CAPACITY;
pub const DEFAULT_PENDING_DELIVERY_LIMIT: usize =
    DEFAULT_RUNTIME_ROUTE_LIMIT + DEFAULT_ACTIVE_TARGET_LIMIT + DEFAULT_CONTROL_DELIVERY_RESERVE;
/// Five in-flight leases without any terminal, progress, or lease renewal is
/// the retention window for a possible late completion before the scheduler
/// itself retires the stalled owner and requeues the route. Healthy long work
/// (directory scans) renews its lease through progress publications, so this
/// deadline only fires for executors that stopped without publishing.
pub const DEFAULT_MAX_STALLED_LIFETIME_MS: u64 = 5 * crate::ecs::DEFAULT_IN_FLIGHT_LEASE_MS;

/// Explicit cardinality and retained-memory budgets for one runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeBudgets {
    pub route_limit: usize,
    pub active_target_limit: usize,
    pub active_target_limit_per_capability: usize,
    pub active_target_limit_per_domain: usize,
    pub target_scope_byte_limit: usize,
    /// Maximum admitted lifecycles whose terminal event has not yet been
    /// drained by the application. This also bounds the terminal mailbox.
    pub pending_delivery_limit: usize,
    /// Slots reserved for control terminals even when observation owners fill
    /// their entire delivery partition.
    pub control_delivery_reserve: usize,
    /// How long a stalled owner is retained for a possible late completion
    /// before the scheduler retires it and requeues the route.
    pub max_stalled_lifetime_ms: u64,
}

impl RuntimeBudgets {
    pub const DEFAULT: Self = Self {
        route_limit: DEFAULT_RUNTIME_ROUTE_LIMIT,
        active_target_limit: DEFAULT_ACTIVE_TARGET_LIMIT,
        active_target_limit_per_capability: DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY,
        active_target_limit_per_domain: DEFAULT_ACTIVE_TARGET_LIMIT_PER_DOMAIN,
        target_scope_byte_limit: DEFAULT_TARGET_SCOPE_BYTE_LIMIT,
        pending_delivery_limit: DEFAULT_PENDING_DELIVERY_LIMIT,
        control_delivery_reserve: DEFAULT_CONTROL_DELIVERY_RESERVE,
        max_stalled_lifetime_ms: DEFAULT_MAX_STALLED_LIFETIME_MS,
    };
}

impl Default for RuntimeBudgets {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Default for QueueCapacities {
    fn default() -> Self {
        Self {
            observation_requests: 8,
            control_requests: 16,
            control_events: DEFAULT_CONTROL_EVENT_QUEUE_CAPACITY,
            observation_events: 64,
        }
    }
}

/// Explicit native composition inputs for the OS-neutral runtime.
#[derive(Clone, Copy)]
pub struct RuntimeConfig {
    pub queues: QueueCapacities,
    pub budgets: RuntimeBudgets,
    /// Wall-clock milliseconds used only for externally observed facts.
    pub clock_ms: fn() -> u64,
    /// Monotonic milliseconds used only for cadence, retry, and lease state.
    pub monotonic_clock_ms: fn() -> u64,
}

impl RuntimeConfig {
    #[must_use]
    pub const fn new(clock_ms: fn() -> u64) -> Self {
        Self {
            queues: QueueCapacities {
                observation_requests: 8,
                control_requests: 16,
                control_events: DEFAULT_CONTROL_EVENT_QUEUE_CAPACITY,
                observation_events: 64,
            },
            budgets: RuntimeBudgets::DEFAULT,
            clock_ms,
            monotonic_clock_ms,
        }
    }

    #[must_use]
    pub const fn with_queues(mut self, queues: QueueCapacities) -> Self {
        self.queues = queues;
        self
    }

    #[must_use]
    pub const fn with_budgets(mut self, budgets: RuntimeBudgets) -> Self {
        self.budgets = budgets;
        self
    }

    /// Override the scheduling clock, primarily for deterministic tests and
    /// embedding runtimes with an existing monotonic time authority.
    #[must_use]
    pub const fn with_monotonic_clock(mut self, clock_ms: fn() -> u64) -> Self {
        self.monotonic_clock_ms = clock_ms;
        self
    }
}

#[derive(Clone, Default)]
pub struct SystemProviderBindings {
    pub(crate) host: crate::ProviderBinding<HostTelemetryRequest>,
    pub(crate) cpu: crate::ProviderBinding<CpuTelemetryRequest>,
    pub(crate) memory: crate::ProviderBinding<MemoryTelemetryRequest>,
    pub(crate) storage: crate::ProviderBinding<StorageTelemetryRequest>,
    pub(crate) network: crate::ProviderBinding<NetworkTelemetryRequest>,
    pub(crate) gpu: crate::ProviderBinding<GpuTelemetryRequest>,
    pub(crate) hardware_inventory: crate::ProviderBinding<HardwareInventoryRequest>,
    pub(crate) containers: crate::ProviderBinding<ContainerRollupRequest>,
    pub(crate) gpu_engine_rows: crate::ProviderBinding<GpuEngineRowsRequest>,
    pub(crate) npu_inventory: crate::ProviderBinding<NpuInventoryRequest>,
    pub(crate) smbios_memory: crate::ProviderBinding<SmbiosMemoryRequest>,
    pub(crate) rapl_power: crate::ProviderBinding<RaplPowerRequest>,
    pub(crate) msr_readout: crate::ProviderBinding<MsrReadoutRequest>,
}

/// Required system capability bindings installed as one named composition
/// transaction. Optional accelerators remain explicit builder steps.
pub struct SystemProviderBindingsInput {
    pub host: crate::ProviderBinding<HostTelemetryRequest>,
    pub cpu: crate::ProviderBinding<CpuTelemetryRequest>,
    pub memory: crate::ProviderBinding<MemoryTelemetryRequest>,
    pub storage: crate::ProviderBinding<StorageTelemetryRequest>,
    pub network: crate::ProviderBinding<NetworkTelemetryRequest>,
    pub gpu: crate::ProviderBinding<GpuTelemetryRequest>,
    pub hardware_inventory: crate::ProviderBinding<HardwareInventoryRequest>,
    pub containers: crate::ProviderBinding<ContainerRollupRequest>,
}

impl SystemProviderBindings {
    #[must_use]
    pub fn new(input: SystemProviderBindingsInput) -> Self {
        Self {
            host: input.host,
            cpu: input.cpu,
            memory: input.memory,
            storage: input.storage,
            network: input.network,
            gpu: input.gpu,
            hardware_inventory: input.hardware_inventory,
            containers: input.containers,
            gpu_engine_rows: crate::ProviderBinding::absent(),
            npu_inventory: crate::ProviderBinding::absent(),
            smbios_memory: crate::ProviderBinding::absent(),
            rapl_power: crate::ProviderBinding::absent(),
            msr_readout: crate::ProviderBinding::absent(),
        }
    }

    /// Attach the optional per-engine GPU utilization provider. Native
    /// adapters without this facet leave it absent: no catalog descriptor,
    /// request port, or execution lane is created (mirrors
    /// `with_directory_usage`), and the UI reports the capability as honestly
    /// unavailable.
    #[must_use]
    pub fn with_gpu_engine_rows<P>(
        mut self,
        gpu_engine_rows: &crate::ProviderRegistration<GpuEngineRowsRequest, P>,
    ) -> Self {
        self.gpu_engine_rows = gpu_engine_rows.binding();
        self
    }

    /// Attach the optional NPU accelerator inventory provider. Native
    /// adapters without this facet leave it absent: no catalog descriptor,
    /// request port, or execution lane is created (mirrors
    /// `with_gpu_engine_rows`), and the UI reports the capability as honestly
    /// unavailable.
    #[must_use]
    pub fn with_npu_inventory<P>(
        mut self,
        npu_inventory: &crate::ProviderRegistration<NpuInventoryRequest, P>,
    ) -> Self {
        self.npu_inventory = npu_inventory.binding();
        self
    }

    /// Attach the optional SMBIOS memory-inventory provider. Native adapters
    /// without this facet leave it absent: no catalog descriptor, request
    /// port, or execution lane is created (mirrors `with_gpu_engine_rows`),
    /// and the UI reports the capability as honestly unavailable.
    #[must_use]
    pub fn with_smbios_memory<P>(
        mut self,
        smbios_memory: &crate::ProviderRegistration<SmbiosMemoryRequest, P>,
    ) -> Self {
        self.smbios_memory = smbios_memory.binding();
        self
    }

    /// Attach the optional CPU package-power provider. Native adapters
    /// without this facet leave it absent: no catalog descriptor, request
    /// port, or execution lane is created (mirrors `with_gpu_engine_rows`),
    /// and the UI reports the capability as honestly unavailable.
    #[must_use]
    pub fn with_rapl_power<P>(
        mut self,
        rapl_power: &crate::ProviderRegistration<RaplPowerRequest, P>,
    ) -> Self {
        self.rapl_power = rapl_power.binding();
        self
    }

    /// Attach the optional CPU MSR-readout provider. Native adapters without
    /// this facet leave it absent: no catalog descriptor, request port, or
    /// execution lane is created (mirrors `with_gpu_engine_rows`), and the UI
    /// reports the capability as honestly unavailable.
    #[must_use]
    pub fn with_msr_readout<P>(
        mut self,
        msr_readout: &crate::ProviderRegistration<MsrReadoutRequest, P>,
    ) -> Self {
        self.msr_readout = msr_readout.binding();
        self
    }
}

#[derive(Clone, Default)]
pub struct ProcessProviderBindings {
    pub(crate) list: crate::ProviderBinding<ProcessListRequest>,
    pub(crate) control: crate::ProviderBinding<ProcessControlRequest>,
    pub(crate) network: crate::ProviderBinding<ProcessNetworkRequest>,
    pub(crate) gpu: crate::ProviderBinding<ProcessGpuRequest>,
    pub(crate) resources: crate::ProviderBinding<ProcessResourcesRequest>,
    pub(crate) isolation: crate::ProviderBinding<ProcessIsolationRequest>,
    pub(crate) threads: crate::ProviderBinding<ProcessThreadsRequest>,
    pub(crate) affinity: crate::ProviderBinding<ProcessAffinityRequest>,
    pub(crate) affinity_control: crate::ProviderBinding<ProcessAffinityControlRequest>,
    pub(crate) resource_control: crate::ProviderBinding<ProcessResourceControlRequest>,
    pub(crate) network_escalation: crate::ProviderBinding<ProcessNetworkEscalationRequest>,
    pub(crate) open_files: crate::ProviderBinding<ProcessOpenFilesRequest>,
    pub(crate) environment: crate::ProviderBinding<ProcessEnvironmentRequest>,
}

/// Required process capability bindings installed as one named composition
/// transaction. Optional insight facets remain explicit builder steps.
pub struct ProcessProviderBindingsInput {
    pub list: crate::ProviderBinding<ProcessListRequest>,
    pub control: crate::ProviderBinding<ProcessControlRequest>,
    pub network: crate::ProviderBinding<ProcessNetworkRequest>,
    pub gpu: crate::ProviderBinding<ProcessGpuRequest>,
    pub resources: crate::ProviderBinding<ProcessResourcesRequest>,
    pub isolation: crate::ProviderBinding<ProcessIsolationRequest>,
    pub threads: crate::ProviderBinding<ProcessThreadsRequest>,
    pub affinity: crate::ProviderBinding<ProcessAffinityRequest>,
    pub affinity_control: crate::ProviderBinding<ProcessAffinityControlRequest>,
    pub resource_control: crate::ProviderBinding<ProcessResourceControlRequest>,
    pub network_escalation: crate::ProviderBinding<ProcessNetworkEscalationRequest>,
}

impl ProcessProviderBindings {
    #[must_use]
    pub fn new(input: ProcessProviderBindingsInput) -> Self {
        Self {
            list: input.list,
            control: input.control,
            network: input.network,
            gpu: input.gpu,
            resources: input.resources,
            isolation: input.isolation,
            threads: input.threads,
            affinity: input.affinity,
            affinity_control: input.affinity_control,
            resource_control: input.resource_control,
            network_escalation: input.network_escalation,
            open_files: crate::ProviderBinding::absent(),
            environment: crate::ProviderBinding::absent(),
        }
    }

    /// Attach the optional OpenFiles insight provider. Native adapters without
    /// this facet leave it absent: no catalog descriptor, request port, or
    /// execution lane is created (mirrors `with_setup_script`).
    #[must_use]
    pub fn with_open_files<P>(
        mut self,
        open_files: &crate::ProviderRegistration<ProcessOpenFilesRequest, P>,
    ) -> Self {
        self.open_files = open_files.binding();
        self
    }

    /// Attach the optional Environment insight provider. Native adapters
    /// without this facet leave it absent: no catalog descriptor, request
    /// port, or execution lane is created (mirrors `with_open_files`).
    #[must_use]
    pub fn with_environment<P>(
        mut self,
        environment: &crate::ProviderRegistration<ProcessEnvironmentRequest, P>,
    ) -> Self {
        self.environment = environment.binding();
        self
    }
}

#[derive(Clone, Default)]
pub struct ServiceProviderBindings {
    pub(crate) inventory: crate::ProviderBinding<ServiceInventoryRequest>,
    pub(crate) dependencies: crate::ProviderBinding<ServiceDependenciesRequest>,
    pub(crate) control: crate::ProviderBinding<ServiceControlRequest>,
    pub(crate) log_snapshot: crate::ProviderBinding<ServiceLogSnapshotRequest>,
    pub(crate) log_stream: crate::ProviderBinding<ServiceLogStreamRequest>,
}

impl ServiceProviderBindings {
    #[must_use]
    pub fn from_registrations<I, D, C, L, S>(
        inventory: &crate::ProviderRegistration<ServiceInventoryRequest, I>,
        dependencies: &crate::ProviderRegistration<ServiceDependenciesRequest, D>,
        control: &crate::ProviderRegistration<ServiceControlRequest, C>,
        log_snapshot: &crate::ProviderRegistration<ServiceLogSnapshotRequest, L>,
        log_stream: &crate::ProviderRegistration<ServiceLogStreamRequest, S>,
    ) -> Self {
        Self {
            inventory: inventory.binding(),
            dependencies: dependencies.binding(),
            control: control.binding(),
            log_snapshot: log_snapshot.binding(),
            log_stream: log_stream.binding(),
        }
    }
}

#[derive(Clone, Default)]
pub struct EnvironmentProviderBindings {
    pub(crate) startup_inventory: crate::ProviderBinding<StartupInventoryRequest>,
    pub(crate) startup_evidence: crate::ProviderBinding<StartupEvidenceRequest>,
    pub(crate) startup_control: crate::ProviderBinding<StartupControlRequest>,
    pub(crate) session_inventory: crate::ProviderBinding<SessionInventoryRequest>,
    pub(crate) session_control: crate::ProviderBinding<SessionControlRequest>,
}

impl EnvironmentProviderBindings {
    #[must_use]
    pub fn from_registrations<I, E, C, S, M>(
        startup_inventory: &crate::ProviderRegistration<StartupInventoryRequest, I>,
        startup_evidence: &crate::ProviderRegistration<StartupEvidenceRequest, E>,
        startup_control: &crate::ProviderRegistration<StartupControlRequest, C>,
        session_inventory: &crate::ProviderRegistration<SessionInventoryRequest, S>,
        session_control: &crate::ProviderRegistration<SessionControlRequest, M>,
    ) -> Self {
        Self {
            startup_inventory: startup_inventory.binding(),
            startup_evidence: startup_evidence.binding(),
            startup_control: startup_control.binding(),
            session_inventory: session_inventory.binding(),
            session_control: session_control.binding(),
        }
    }
}

#[derive(Clone, Default)]
pub struct IntegrationProviderBindings {
    pub(crate) command_launch: crate::ProviderBinding<CommandLaunchRequest>,
    pub(crate) resource_reveal: crate::ProviderBinding<ResourceRevealRequest>,
    pub(crate) url_open: crate::ProviderBinding<UrlOpenRequest>,
    pub(crate) desktop_appearance: crate::ProviderBinding<DesktopAppearanceRequest>,
    pub(crate) desktop_notification: crate::ProviderBinding<DesktopNotificationRequest>,
    pub(crate) setup_script: crate::ProviderBinding<SetupScriptRequest>,
}

impl IntegrationProviderBindings {
    #[must_use]
    pub fn from_registrations<C, R, U, D>(
        command_launch: &crate::ProviderRegistration<CommandLaunchRequest, C>,
        resource_reveal: &crate::ProviderRegistration<ResourceRevealRequest, R>,
        url_open: &crate::ProviderRegistration<UrlOpenRequest, U>,
        desktop_appearance: &crate::ProviderRegistration<DesktopAppearanceRequest, D>,
    ) -> Self {
        Self {
            command_launch: command_launch.binding(),
            resource_reveal: resource_reveal.binding(),
            url_open: url_open.binding(),
            desktop_appearance: desktop_appearance.binding(),
            desktop_notification: crate::ProviderBinding::absent(),
            setup_script: crate::ProviderBinding::absent(),
        }
    }

    #[must_use]
    pub fn with_desktop_notification<N>(
        mut self,
        desktop_notification: &crate::ProviderRegistration<DesktopNotificationRequest, N>,
    ) -> Self {
        self.desktop_notification = desktop_notification.binding();
        self
    }

    #[must_use]
    pub fn with_setup_script<P>(
        mut self,
        setup_script: &crate::ProviderRegistration<SetupScriptRequest, P>,
    ) -> Self {
        self.setup_script = setup_script.binding();
        self
    }
}

#[derive(Clone, Default)]
pub struct StorageProviderBindings {
    pub(crate) health: crate::ProviderBinding<StorageHealthRequest>,
    pub(crate) smart_observation: crate::ProviderBinding<SmartObservationRequest>,
    pub(crate) smart_control: crate::ProviderBinding<SmartControlRequest>,
    pub(crate) directory_usage: crate::ProviderBinding<DirectoryUsageRequest>,
}

impl StorageProviderBindings {
    #[must_use]
    pub fn from_registrations<H, O, C>(
        health: &crate::ProviderRegistration<StorageHealthRequest, H>,
        smart_observation: &crate::ProviderRegistration<SmartObservationRequest, O>,
        smart_control: &crate::ProviderRegistration<SmartControlRequest, C>,
    ) -> Self {
        Self {
            health: health.binding(),
            smart_observation: smart_observation.binding(),
            smart_control: smart_control.binding(),
            directory_usage: crate::ProviderBinding::absent(),
        }
    }

    /// Attach the optional directory-usage scan provider. Native adapters
    /// without this facet leave it absent: no catalog descriptor, request
    /// port, or execution lane is created (mirrors `with_open_files`), and
    /// the UI reports the capability as honestly unavailable.
    #[must_use]
    pub fn with_directory_usage<P>(
        mut self,
        directory_usage: &crate::ProviderRegistration<DirectoryUsageRequest, P>,
    ) -> Self {
        self.directory_usage = directory_usage.binding();
        self
    }
}

#[derive(Clone, Default)]
pub struct SensorProviderBindings {
    pub(crate) observation: crate::ProviderBinding<SensorRequest>,
}

impl SensorProviderBindings {
    #[must_use]
    pub fn from_registration<P>(
        observation: &crate::ProviderRegistration<SensorRequest, P>,
    ) -> Self {
        Self {
            observation: observation.binding(),
        }
    }
}

#[derive(Clone, Default)]
pub struct PowerProviderBindings {
    pub(crate) supplies: crate::ProviderBinding<PowerSupplyRequest>,
}

impl PowerProviderBindings {
    #[must_use]
    pub fn from_registration<P>(
        supplies: &crate::ProviderRegistration<PowerSupplyRequest, P>,
    ) -> Self {
        Self {
            supplies: supplies.binding(),
        }
    }
}

/// Optional provider attribution for application capabilities.
///
/// Grouping mirrors `PlatformFacets`; it is construction data, not a provider
/// trait bag or an aggregate execution lane. An absent provider creates no
/// catalog descriptor, request port, or execution lane.
#[derive(Clone, Default)]
pub struct RuntimeProviderBindings {
    pub system: SystemProviderBindings,
    pub process: ProcessProviderBindings,
    pub service: ServiceProviderBindings,
    pub environment: EnvironmentProviderBindings,
    pub integration: IntegrationProviderBindings,
    pub storage: StorageProviderBindings,
    pub sensor: SensorProviderBindings,
    pub power: PowerProviderBindings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryClass {
    Control,
    Observation,
}

/// Single runtime-domain authority for ECS plugin routing.
///
/// A capability route belongs to exactly one primary domain. Cross-cutting
/// presentation or telemetry relationships do not duplicate ownership here;
/// plugins consume this typed field instead of maintaining parallel lists of
/// `CapabilityId` constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeDomain {
    System,
    Process,
    Storage,
    Service,
    Environment,
    Integration,
    Sensor,
    Power,
}

impl RuntimeDomain {
    pub(crate) const COUNT: usize = 8;
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::System,
        Self::Process,
        Self::Storage,
        Self::Service,
        Self::Environment,
        Self::Integration,
        Self::Sensor,
        Self::Power,
    ];

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Process => 1,
            Self::Storage => 2,
            Self::Service => 3,
            Self::Environment => 4,
            Self::Integration => 5,
            Self::Sensor => 6,
            Self::Power => 7,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CapabilityRoute {
    pub capability: CapabilityId,
    pub provider: ProviderId,
    pub delivery: DeliveryClass,
    pub domain: RuntimeDomain,
    pub cadence_ms: Option<u64>,
    pub sideband_policy: SidebandPolicy,
}

impl RuntimeProviderBindings {
    pub(crate) fn routes(&self) -> Vec<CapabilityRoute> {
        self.routed_capabilities()
            .into_iter()
            .map(|(route, _status)| route)
            .collect()
    }

    pub(crate) fn routes_with_initial_statuses(
        &self,
    ) -> (
        Vec<CapabilityRoute>,
        std::collections::BTreeMap<CapabilityId, CapabilityStatus>,
    ) {
        let routed = self.routed_capabilities();
        let mut routes = Vec::with_capacity(routed.len());
        let mut initial_statuses = std::collections::BTreeMap::new();
        for (route, status) in routed {
            initial_statuses.insert(route.capability.clone(), status);
            routes.push(route);
        }
        (routes, initial_statuses)
    }

    fn routed_capabilities(&self) -> Vec<(CapabilityRoute, CapabilityStatus)> {
        let observation = DeliveryClass::Observation;
        let control = DeliveryClass::Control;
        let system = RuntimeDomain::System;
        let process = RuntimeDomain::Process;
        let storage = RuntimeDomain::Storage;
        let service = RuntimeDomain::Service;
        let environment = RuntimeDomain::Environment;
        let integration = RuntimeDomain::Integration;
        let sensor = RuntimeDomain::Sensor;
        let power = RuntimeDomain::Power;
        [
            route::<HostTelemetryRequest>(&self.system.host, observation, system),
            route::<CpuTelemetryRequest>(&self.system.cpu, observation, system),
            route::<MemoryTelemetryRequest>(&self.system.memory, observation, system),
            route::<StorageTelemetryRequest>(&self.system.storage, observation, system),
            route::<NetworkTelemetryRequest>(&self.system.network, observation, system),
            route::<GpuTelemetryRequest>(&self.system.gpu, observation, system),
            route::<HardwareInventoryRequest>(&self.system.hardware_inventory, observation, system),
            route::<ContainerRollupRequest>(&self.system.containers, observation, system),
            route::<GpuEngineRowsRequest>(&self.system.gpu_engine_rows, observation, system),
            route::<NpuInventoryRequest>(&self.system.npu_inventory, observation, system),
            route::<SmbiosMemoryRequest>(&self.system.smbios_memory, observation, system),
            route::<RaplPowerRequest>(&self.system.rapl_power, observation, system),
            route::<MsrReadoutRequest>(&self.system.msr_readout, observation, system),
            route::<ProcessListRequest>(&self.process.list, observation, process),
            route::<ProcessControlRequest>(&self.process.control, control, process),
            route::<ProcessNetworkRequest>(&self.process.network, observation, process),
            route::<ProcessGpuRequest>(&self.process.gpu, observation, process),
            route::<ProcessResourcesRequest>(&self.process.resources, observation, process),
            route::<ProcessIsolationRequest>(&self.process.isolation, observation, process),
            route::<ProcessThreadsRequest>(&self.process.threads, observation, process),
            route::<ProcessOpenFilesRequest>(&self.process.open_files, observation, process),
            route::<ProcessEnvironmentRequest>(&self.process.environment, observation, process),
            route::<ProcessAffinityRequest>(&self.process.affinity, observation, process),
            route::<ProcessAffinityControlRequest>(
                &self.process.affinity_control,
                control,
                process,
            ),
            route::<ProcessResourceControlRequest>(
                &self.process.resource_control,
                control,
                process,
            ),
            route::<ProcessNetworkEscalationRequest>(
                &self.process.network_escalation,
                control,
                process,
            ),
            route::<ServiceInventoryRequest>(&self.service.inventory, observation, service),
            route::<ServiceDependenciesRequest>(&self.service.dependencies, observation, service),
            route::<ServiceControlRequest>(&self.service.control, control, service),
            route::<ServiceLogSnapshotRequest>(&self.service.log_snapshot, observation, service),
            route::<ServiceLogStreamRequest>(&self.service.log_stream, observation, service),
            route::<StartupInventoryRequest>(
                &self.environment.startup_inventory,
                observation,
                environment,
            ),
            route::<StartupEvidenceRequest>(
                &self.environment.startup_evidence,
                observation,
                environment,
            ),
            route::<StartupControlRequest>(&self.environment.startup_control, control, environment),
            route::<SessionInventoryRequest>(
                &self.environment.session_inventory,
                observation,
                environment,
            ),
            route::<SessionControlRequest>(&self.environment.session_control, control, environment),
            route::<CommandLaunchRequest>(&self.integration.command_launch, control, integration),
            route::<ResourceRevealRequest>(&self.integration.resource_reveal, control, integration),
            route::<UrlOpenRequest>(&self.integration.url_open, control, integration),
            route::<DesktopAppearanceRequest>(
                &self.integration.desktop_appearance,
                observation,
                integration,
            ),
            route::<SetupScriptRequest>(&self.integration.setup_script, control, integration),
            route::<DesktopNotificationRequest>(
                &self.integration.desktop_notification,
                control,
                integration,
            ),
            route::<StorageHealthRequest>(&self.storage.health, observation, storage),
            route::<DirectoryUsageRequest>(&self.storage.directory_usage, observation, storage),
            route::<SensorRequest>(&self.sensor.observation, observation, sensor),
            route::<PowerSupplyRequest>(&self.power.supplies, observation, power),
            route::<SmartObservationRequest>(&self.storage.smart_observation, observation, storage),
            route::<SmartControlRequest>(&self.storage.smart_control, control, storage),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

fn route<R: CapabilityRequest>(
    binding: &ProviderBinding<R>,
    delivery: DeliveryClass,
    domain: RuntimeDomain,
) -> Option<(CapabilityRoute, CapabilityStatus)> {
    let (provider, initial_status) = binding.route_parts()?;
    Some((
        CapabilityRoute {
            capability: R::CAPABILITY.clone(),
            provider: provider.clone(),
            delivery,
            domain,
            cadence_ms: default_automatic_cadence_ms(&R::CAPABILITY),
            sideband_policy: R::SIDEBAND_POLICY,
        },
        initial_status,
    ))
}

/// Product-owned automatic cadence defaults. Manual capabilities remain in
/// the ECS catalog so their lifecycle is correlated, but they do not become
/// background work merely because a provider exists.
fn default_automatic_cadence_ms(capability: &CapabilityId) -> Option<u64> {
    taskmanager_application::default_automatic_cadence_ms(capability)
}

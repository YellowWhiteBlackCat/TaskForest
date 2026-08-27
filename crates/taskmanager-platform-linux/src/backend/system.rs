//! Linux system telemetry providers bound to shared system observation and auxiliary executors.
//!
//! Owns `SystemProviders`, split into `SystemObservationProviders`
//! (host, cpu, memory, storage, network, gpu, containers) and `SystemAuxiliaryProviders` (hardware
//! inventory).

use taskmanager_application::{
    ContainerRollupRequest, CpuTelemetryRequest, GpuEngineRowsRequest, GpuTelemetryRequest,
    HardwareInventoryRequest, HostTelemetryRequest, MemoryTelemetryRequest,
    NetworkTelemetryRequest, NpuInventoryRequest, StorageTelemetryRequest,
};
use taskmanager_platform_provider::{
    ContainerRollupProvider, CpuTelemetryProvider, GpuEngineRowsProvider, GpuTelemetryProvider,
    HardwareInventoryProvider, HostTelemetryProvider, MemoryTelemetryProvider,
    NetworkTelemetryProvider, NpuInventoryProvider, StorageTelemetryProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, SystemAuxiliaryExecutors, SystemExecutors, SystemObservationExecutors,
    SystemProviderBindings, SystemProviderBindingsInput,
};

type HostRegistration = ProviderRegistration<HostTelemetryRequest, Box<dyn HostTelemetryProvider>>;
type CpuRegistration = ProviderRegistration<CpuTelemetryRequest, Box<dyn CpuTelemetryProvider>>;
type MemoryRegistration =
    ProviderRegistration<MemoryTelemetryRequest, Box<dyn MemoryTelemetryProvider>>;
type StorageRegistration =
    ProviderRegistration<StorageTelemetryRequest, Box<dyn StorageTelemetryProvider>>;
type NetworkRegistration =
    ProviderRegistration<NetworkTelemetryRequest, Box<dyn NetworkTelemetryProvider>>;
type GpuRegistration = ProviderRegistration<GpuTelemetryRequest, Box<dyn GpuTelemetryProvider>>;
type HardwareInventoryRegistration =
    ProviderRegistration<HardwareInventoryRequest, Box<dyn HardwareInventoryProvider>>;
type ContainerRegistration =
    ProviderRegistration<ContainerRollupRequest, Box<dyn ContainerRollupProvider>>;
type GpuEngineRowsRegistration =
    ProviderRegistration<GpuEngineRowsRequest, Box<dyn GpuEngineRowsProvider>>;
type NpuInventoryRegistration =
    ProviderRegistration<NpuInventoryRequest, Box<dyn NpuInventoryProvider>>;

/// Seven independently scheduled Linux observation providers.
pub struct SystemObservationProviders {
    host: HostRegistration,
    cpu: CpuRegistration,
    memory: MemoryRegistration,
    storage: StorageRegistration,
    network: NetworkRegistration,
    gpu: GpuRegistration,
    containers: ContainerRegistration,
}

impl SystemObservationProviders {
    #[must_use]
    pub fn new<H, C, M, S, N, G, D>(
        host: ProviderRegistration<HostTelemetryRequest, H>,
        cpu: ProviderRegistration<CpuTelemetryRequest, C>,
        memory: ProviderRegistration<MemoryTelemetryRequest, M>,
        storage: ProviderRegistration<StorageTelemetryRequest, S>,
        network: ProviderRegistration<NetworkTelemetryRequest, N>,
        gpu: ProviderRegistration<GpuTelemetryRequest, G>,
        containers: ProviderRegistration<ContainerRollupRequest, D>,
    ) -> Self
    where
        H: HostTelemetryProvider,
        C: CpuTelemetryProvider,
        M: MemoryTelemetryProvider,
        S: StorageTelemetryProvider,
        N: NetworkTelemetryProvider,
        G: GpuTelemetryProvider,
        D: ContainerRollupProvider,
    {
        Self {
            host: host
                .map_provider(|provider| Box::new(provider) as Box<dyn HostTelemetryProvider>),
            cpu: cpu.map_provider(|provider| Box::new(provider) as Box<dyn CpuTelemetryProvider>),
            memory: memory
                .map_provider(|provider| Box::new(provider) as Box<dyn MemoryTelemetryProvider>),
            storage: storage
                .map_provider(|provider| Box::new(provider) as Box<dyn StorageTelemetryProvider>),
            network: network
                .map_provider(|provider| Box::new(provider) as Box<dyn NetworkTelemetryProvider>),
            gpu: gpu.map_provider(|provider| Box::new(provider) as Box<dyn GpuTelemetryProvider>),
            containers: containers
                .map_provider(|provider| Box::new(provider) as Box<dyn ContainerRollupProvider>),
        }
    }

    fn into_runtime(self) -> SystemObservationExecutors {
        let Self {
            host,
            cpu,
            memory,
            storage,
            network,
            gpu,
            containers,
        } = self;
        let mut host = host.into_provider();
        let mut cpu = cpu.into_provider();
        let mut memory = memory.into_provider();
        let mut storage = storage.into_provider();
        let mut network = network.into_provider();
        let mut gpu = gpu.into_provider();
        let mut containers = containers.into_provider();
        SystemObservationExecutors::new(
            move |observed_at_ms| host.refresh(observed_at_ms),
            move |observed_at_ms| cpu.refresh(observed_at_ms),
            move |observed_at_ms| memory.refresh(observed_at_ms),
            move |observed_at_ms| storage.refresh(observed_at_ms),
            move |observed_at_ms| network.refresh(observed_at_ms),
            move |observed_at_ms| gpu.refresh(observed_at_ms),
            move |now_ms| containers.refresh(now_ms),
        )
    }
}

/// Hardware operations outside domain observation ownership.
pub struct SystemAuxiliaryProviders {
    hardware_inventory: HardwareInventoryRegistration,
    gpu_engine_rows: GpuEngineRowsRegistration,
    npu_inventory: NpuInventoryRegistration,
}

impl SystemAuxiliaryProviders {
    #[must_use]
    pub fn new<P, E, N>(
        hardware_inventory: ProviderRegistration<HardwareInventoryRequest, P>,
        gpu_engine_rows: ProviderRegistration<GpuEngineRowsRequest, E>,
        npu_inventory: ProviderRegistration<NpuInventoryRequest, N>,
    ) -> Self
    where
        P: HardwareInventoryProvider,
        E: GpuEngineRowsProvider,
        N: NpuInventoryProvider,
    {
        Self {
            hardware_inventory: hardware_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn HardwareInventoryProvider>),
            gpu_engine_rows: gpu_engine_rows
                .map_provider(|provider| Box::new(provider) as Box<dyn GpuEngineRowsProvider>),
            npu_inventory: npu_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn NpuInventoryProvider>),
        }
    }

    fn into_runtime(self) -> SystemAuxiliaryExecutors {
        let Self {
            hardware_inventory,
            gpu_engine_rows,
            npu_inventory,
        } = self;
        let mut hardware_inventory = hardware_inventory.into_provider();
        let mut gpu_engine_rows = gpu_engine_rows.into_provider();
        let mut npu_inventory = npu_inventory.into_provider();
        SystemAuxiliaryExecutors::new(move || hardware_inventory.refresh())
            .with_gpu_engine_rows(move |request| {
                gpu_engine_rows.read_engine_rows(&request.device_id)
            })
            .with_npu_inventory(move |observed_at_ms| npu_inventory.read_inventory(observed_at_ms))
    }
}

/// Linux system provider composition grouped by scheduling responsibility.
pub struct SystemProviders {
    observations: SystemObservationProviders,
    auxiliary: SystemAuxiliaryProviders,
}

impl SystemProviders {
    #[must_use]
    pub const fn new(
        observations: SystemObservationProviders,
        auxiliary: SystemAuxiliaryProviders,
    ) -> Self {
        Self {
            observations,
            auxiliary,
        }
    }

    pub(crate) fn runtime_bindings(&self) -> SystemProviderBindings {
        SystemProviderBindings::new(SystemProviderBindingsInput {
            host: self.observations.host.binding(),
            cpu: self.observations.cpu.binding(),
            memory: self.observations.memory.binding(),
            storage: self.observations.storage.binding(),
            network: self.observations.network.binding(),
            gpu: self.observations.gpu.binding(),
            hardware_inventory: self.auxiliary.hardware_inventory.binding(),
            containers: self.observations.containers.binding(),
        })
        .with_gpu_engine_rows(&self.auxiliary.gpu_engine_rows)
        .with_npu_inventory(&self.auxiliary.npu_inventory)
    }

    pub(crate) fn into_runtime(self) -> SystemExecutors {
        SystemExecutors::new(
            self.observations.into_runtime(),
            self.auxiliary.into_runtime(),
        )
    }
}

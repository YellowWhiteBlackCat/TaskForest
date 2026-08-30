//! OS-neutral system execution contracts and six independent telemetry lanes.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::{
    ContainerRollupEvent, ContainerRollupRequest, CpuTelemetryRequest, GpuEngineRowsEvent,
    GpuEngineRowsRequest, GpuTelemetryRequest, HardwareInventoryEvent, HardwareInventoryRequest,
    HostTelemetryRequest, MemoryTelemetryRequest, MsrReadoutRequest, NetworkTelemetryRequest,
    NpuInventoryEvent, NpuInventoryRequest, PlatformEvent, RaplPowerRequest, SmbiosMemoryRequest,
    StorageTelemetryRequest, SystemTelemetryDomainEvent,
};
use taskmanager_core::{
    ContainerRollup, CpuTelemetryObservation, GpuEngineRowsSnapshot, GpuTelemetryObservation,
    HardwareInfo, HostRuntimeObservation, MemoryTelemetryObservation, MsrReadoutSnapshot,
    NetworkTelemetryObservation, NpuInventorySnapshot, RaplPowerSnapshot, SmbiosMemorySnapshot,
    StorageTelemetryObservation,
};
use taskmanager_platform_contract::{CapabilityId, CompositeSourceSnapshot, ProviderFailure};

use crate::delivery::{recv_or_shutdown_with_idle, spawn_or_register_lane};
use crate::health::CapabilityHealth;
use crate::{
    Queued, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, spawn_lazy_observation_lane,
};

mod snapshot_lanes;

type HostExecutor =
    dyn FnMut(u64) -> Result<HostRuntimeObservation, ProviderFailure> + Send + 'static;
type CpuExecutor =
    dyn FnMut(u64) -> Result<CpuTelemetryObservation, ProviderFailure> + Send + 'static;
type MemoryExecutor =
    dyn FnMut(u64) -> Result<MemoryTelemetryObservation, ProviderFailure> + Send + 'static;
type StorageExecutor =
    dyn FnMut(u64) -> Result<StorageTelemetryObservation, ProviderFailure> + Send + 'static;
type NetworkExecutor =
    dyn FnMut(u64) -> Result<NetworkTelemetryObservation, ProviderFailure> + Send + 'static;
type GpuExecutor =
    dyn FnMut(u64) -> Result<GpuTelemetryObservation, ProviderFailure> + Send + 'static;
type HardwareInventoryExecutor =
    dyn FnMut() -> Result<CompositeSourceSnapshot<HardwareInfo>, ProviderFailure> + Send + 'static;
type ContainerExecutor =
    dyn FnMut(u64) -> Result<ContainerRollup, ProviderFailure> + Send + 'static;
type GpuEngineRowsExecutor = dyn FnMut(&GpuEngineRowsRequest) -> Result<GpuEngineRowsSnapshot, ProviderFailure>
    + Send
    + 'static;
type NpuInventoryExecutor =
    dyn FnMut(u64) -> Result<NpuInventorySnapshot, ProviderFailure> + Send + 'static;
type SmbiosMemoryExecutor =
    dyn FnMut() -> Result<SmbiosMemorySnapshot, ProviderFailure> + Send + 'static;
type RaplPowerExecutor = dyn FnMut() -> Result<RaplPowerSnapshot, ProviderFailure> + Send + 'static;
type MsrReadoutExecutor =
    dyn FnMut() -> Result<MsrReadoutSnapshot, ProviderFailure> + Send + 'static;

/// Six blocking observations that execute on physically independent lanes.
pub struct SystemObservationExecutors {
    host: Box<HostExecutor>,
    cpu: Box<CpuExecutor>,
    memory: Box<MemoryExecutor>,
    storage: Box<StorageExecutor>,
    network: Box<NetworkExecutor>,
    gpu: Box<GpuExecutor>,
    containers: Box<ContainerExecutor>,
}

impl SystemObservationExecutors {
    #[must_use]
    pub fn new<H, C, M, S, N, G, D>(
        host: H,
        cpu: C,
        memory: M,
        storage: S,
        network: N,
        gpu: G,
        containers: D,
    ) -> Self
    where
        H: FnMut(u64) -> Result<HostRuntimeObservation, ProviderFailure> + Send + 'static,
        C: FnMut(u64) -> Result<CpuTelemetryObservation, ProviderFailure> + Send + 'static,
        M: FnMut(u64) -> Result<MemoryTelemetryObservation, ProviderFailure> + Send + 'static,
        S: FnMut(u64) -> Result<StorageTelemetryObservation, ProviderFailure> + Send + 'static,
        N: FnMut(u64) -> Result<NetworkTelemetryObservation, ProviderFailure> + Send + 'static,
        G: FnMut(u64) -> Result<GpuTelemetryObservation, ProviderFailure> + Send + 'static,
        D: FnMut(u64) -> Result<ContainerRollup, ProviderFailure> + Send + 'static,
    {
        Self {
            host: Box::new(host),
            cpu: Box::new(cpu),
            memory: Box::new(memory),
            storage: Box::new(storage),
            network: Box::new(network),
            gpu: Box::new(gpu),
            containers: Box::new(containers),
        }
    }
}

/// Non-domain system operations kept outside the telemetry executor group.
///
/// Per-engine GPU utilization is an optional facet: `None` matches an absent
/// binding, so the capability is honestly unavailable rather than hanging.
pub struct SystemAuxiliaryExecutors {
    hardware_inventory: Box<HardwareInventoryExecutor>,
    gpu_engine_rows: Option<Box<GpuEngineRowsExecutor>>,
    npu_inventory: Option<Box<NpuInventoryExecutor>>,
    smbios_memory: Option<Box<SmbiosMemoryExecutor>>,
    rapl_power: Option<Box<RaplPowerExecutor>>,
    msr_readout: Option<Box<MsrReadoutExecutor>>,
}

impl SystemAuxiliaryExecutors {
    #[must_use]
    pub fn new<H>(hardware_inventory: H) -> Self
    where
        H: FnMut() -> Result<CompositeSourceSnapshot<HardwareInfo>, ProviderFailure>
            + Send
            + 'static,
    {
        Self {
            hardware_inventory: Box::new(hardware_inventory),
            gpu_engine_rows: None,
            npu_inventory: None,
            smbios_memory: None,
            rapl_power: None,
            msr_readout: None,
        }
    }

    /// Attach the optional per-engine GPU utilization executor (mirrors the
    /// optional binding; absence means the capability is honestly
    /// unavailable).
    #[must_use]
    pub fn with_gpu_engine_rows<G>(mut self, gpu_engine_rows: G) -> Self
    where
        G: FnMut(&GpuEngineRowsRequest) -> Result<GpuEngineRowsSnapshot, ProviderFailure>
            + Send
            + 'static,
    {
        self.gpu_engine_rows = Some(Box::new(gpu_engine_rows));
        self
    }

    /// Attach the optional NPU accelerator inventory executor (mirrors the
    /// optional binding; absence means the capability is honestly
    /// unavailable).
    #[must_use]
    pub fn with_npu_inventory<N>(mut self, npu_inventory: N) -> Self
    where
        N: FnMut(u64) -> Result<NpuInventorySnapshot, ProviderFailure> + Send + 'static,
    {
        self.npu_inventory = Some(Box::new(npu_inventory));
        self
    }

    /// Attach the optional SMBIOS memory-inventory executor (mirrors the
    /// optional binding; absence means the capability is honestly
    /// unavailable).
    #[must_use]
    pub fn with_smbios_memory<S>(mut self, smbios_memory: S) -> Self
    where
        S: FnMut() -> Result<SmbiosMemorySnapshot, ProviderFailure> + Send + 'static,
    {
        self.smbios_memory = Some(Box::new(smbios_memory));
        self
    }

    /// Attach the optional CPU package-power executor (mirrors the optional
    /// binding; absence means the capability is honestly unavailable).
    #[must_use]
    pub fn with_rapl_power<R>(mut self, rapl_power: R) -> Self
    where
        R: FnMut() -> Result<RaplPowerSnapshot, ProviderFailure> + Send + 'static,
    {
        self.rapl_power = Some(Box::new(rapl_power));
        self
    }

    /// Attach the optional CPU MSR-readout executor (mirrors the optional
    /// binding; absence means the capability is honestly unavailable).
    #[must_use]
    pub fn with_msr_readout<M>(mut self, msr_readout: M) -> Self
    where
        M: FnMut() -> Result<MsrReadoutSnapshot, ProviderFailure> + Send + 'static,
    {
        self.msr_readout = Some(Box::new(msr_readout));
        self
    }
}

/// Native system operations adapted into OS-independent executor closures.
pub struct SystemExecutors {
    observations: SystemObservationExecutors,
    auxiliary: SystemAuxiliaryExecutors,
}

impl SystemExecutors {
    #[must_use]
    pub const fn new(
        observations: SystemObservationExecutors,
        auxiliary: SystemAuxiliaryExecutors,
    ) -> Self {
        Self {
            observations,
            auxiliary,
        }
    }
}

/// Optional receivers for the six standard system observation capabilities.
pub struct PendingSystemObservationLanes {
    pub host_rx: Option<Receiver<Queued<HostTelemetryRequest>>>,
    pub cpu_rx: Option<Receiver<Queued<CpuTelemetryRequest>>>,
    pub memory_rx: Option<Receiver<Queued<MemoryTelemetryRequest>>>,
    pub storage_rx: Option<Receiver<Queued<StorageTelemetryRequest>>>,
    pub network_rx: Option<Receiver<Queued<NetworkTelemetryRequest>>>,
    pub gpu_rx: Option<Receiver<Queued<GpuTelemetryRequest>>>,
    pub containers_rx: Option<Receiver<Queued<ContainerRollupRequest>>>,
}

impl PendingSystemObservationLanes {
    #[must_use]
    pub(crate) fn new(
        host_rx: Option<Receiver<Queued<HostTelemetryRequest>>>,
        cpu_rx: Option<Receiver<Queued<CpuTelemetryRequest>>>,
        memory_rx: Option<Receiver<Queued<MemoryTelemetryRequest>>>,
        storage_rx: Option<Receiver<Queued<StorageTelemetryRequest>>>,
        network_rx: Option<Receiver<Queued<NetworkTelemetryRequest>>>,
        gpu_rx: Option<Receiver<Queued<GpuTelemetryRequest>>>,
        containers_rx: Option<Receiver<Queued<ContainerRollupRequest>>>,
    ) -> Self {
        Self {
            host_rx,
            cpu_rx,
            memory_rx,
            storage_rx,
            network_rx,
            gpu_rx,
            containers_rx,
        }
    }
}

pub struct PendingSystemAuxiliaryLanes {
    pub hardware_inventory_rx: Option<Receiver<Queued<HardwareInventoryRequest>>>,
    pub gpu_engine_rows_rx: Option<Receiver<Queued<GpuEngineRowsRequest>>>,
    pub npu_inventory_rx: Option<Receiver<Queued<NpuInventoryRequest>>>,
    pub smbios_memory_rx: Option<Receiver<Queued<SmbiosMemoryRequest>>>,
    pub rapl_power_rx: Option<Receiver<Queued<RaplPowerRequest>>>,
    pub msr_readout_rx: Option<Receiver<Queued<MsrReadoutRequest>>>,
}

impl PendingSystemAuxiliaryLanes {
    #[must_use]
    pub(crate) fn new(
        hardware_inventory_rx: Option<Receiver<Queued<HardwareInventoryRequest>>>,
        gpu_engine_rows_rx: Option<Receiver<Queued<GpuEngineRowsRequest>>>,
        npu_inventory_rx: Option<Receiver<Queued<NpuInventoryRequest>>>,
        smbios_memory_rx: Option<Receiver<Queued<SmbiosMemoryRequest>>>,
        rapl_power_rx: Option<Receiver<Queued<RaplPowerRequest>>>,
        msr_readout_rx: Option<Receiver<Queued<MsrReadoutRequest>>>,
    ) -> Self {
        Self {
            hardware_inventory_rx,
            gpu_engine_rows_rx,
            npu_inventory_rx,
            smbios_memory_rx,
            rapl_power_rx,
            msr_readout_rx,
        }
    }
}

/// Optional system receivers while native capability bindings are assembled.
pub struct PendingSystemRuntimeLanes {
    pub observations: PendingSystemObservationLanes,
    pub auxiliary: PendingSystemAuxiliaryLanes,
}

impl PendingSystemRuntimeLanes {
    #[must_use]
    pub(crate) const fn new(
        observations: PendingSystemObservationLanes,
        auxiliary: PendingSystemAuxiliaryLanes,
    ) -> Self {
        Self {
            observations,
            auxiliary,
        }
    }

    pub(crate) fn missing_capabilities(&self) -> impl Iterator<Item = CapabilityId> {
        [
            (
                self.observations.host_rx.is_none(),
                CapabilityId::TELEMETRY_HOST,
            ),
            (
                self.observations.cpu_rx.is_none(),
                CapabilityId::TELEMETRY_CPU,
            ),
            (
                self.observations.memory_rx.is_none(),
                CapabilityId::TELEMETRY_MEMORY,
            ),
            (
                self.observations.storage_rx.is_none(),
                CapabilityId::TELEMETRY_STORAGE,
            ),
            (
                self.observations.network_rx.is_none(),
                CapabilityId::TELEMETRY_NETWORK,
            ),
            (
                self.observations.gpu_rx.is_none(),
                CapabilityId::TELEMETRY_GPU,
            ),
            (
                self.observations.containers_rx.is_none(),
                CapabilityId::CONTAINERS,
            ),
            (
                self.auxiliary.hardware_inventory_rx.is_none(),
                CapabilityId::HARDWARE_INVENTORY,
            ),
        ]
        .into_iter()
        .filter_map(|(missing, capability)| missing.then_some(capability))
    }

    /// Promote only after all six domain lanes, the container lane, and the
    /// hardware-inventory auxiliary lane all exist. The optional engine-rows
    /// lane rides along when bound.
    #[must_use]
    pub fn try_complete(self) -> Option<SystemRuntimeLanes> {
        let Self {
            observations:
                PendingSystemObservationLanes {
                    host_rx: Some(host),
                    cpu_rx: Some(cpu),
                    memory_rx: Some(memory),
                    storage_rx: Some(storage),
                    network_rx: Some(network),
                    gpu_rx: Some(gpu),
                    containers_rx: Some(containers),
                },
            auxiliary:
                PendingSystemAuxiliaryLanes {
                    hardware_inventory_rx: Some(hardware_inventory),
                    gpu_engine_rows_rx,
                    npu_inventory_rx,
                    smbios_memory_rx,
                    rapl_power_rx,
                    msr_readout_rx,
                },
        } = self
        else {
            return None;
        };
        Some(SystemRuntimeLanes {
            observations: SystemObservationLanes {
                host,
                cpu,
                memory,
                storage,
                network,
                gpu,
                containers,
            },
            auxiliary: SystemAuxiliaryLanes {
                hardware_inventory,
                gpu_engine_rows: gpu_engine_rows_rx,
                npu_inventory: npu_inventory_rx,
                smbios_memory: smbios_memory_rx,
                rapl_power: rapl_power_rx,
                msr_readout: msr_readout_rx,
            },
        })
    }
}

pub struct SystemRuntimeLanes {
    observations: SystemObservationLanes,
    auxiliary: SystemAuxiliaryLanes,
}

struct SystemObservationLanes {
    host: Receiver<Queued<HostTelemetryRequest>>,
    cpu: Receiver<Queued<CpuTelemetryRequest>>,
    memory: Receiver<Queued<MemoryTelemetryRequest>>,
    storage: Receiver<Queued<StorageTelemetryRequest>>,
    network: Receiver<Queued<NetworkTelemetryRequest>>,
    gpu: Receiver<Queued<GpuTelemetryRequest>>,
    containers: Receiver<Queued<ContainerRollupRequest>>,
}

struct SystemAuxiliaryLanes {
    hardware_inventory: Receiver<Queued<HardwareInventoryRequest>>,
    gpu_engine_rows: Option<Receiver<Queued<GpuEngineRowsRequest>>>,
    npu_inventory: Option<Receiver<Queued<NpuInventoryRequest>>>,
    smbios_memory: Option<Receiver<Queued<SmbiosMemoryRequest>>>,
    rapl_power: Option<Receiver<Queued<RaplPowerRequest>>>,
    msr_readout: Option<Receiver<Queued<MsrReadoutRequest>>>,
}

/// Attach every system operation to its own typed worker.
pub fn spawn_system_lanes(
    workers: &WorkerRuntime,
    lanes: SystemRuntimeLanes,
    executors: SystemExecutors,
    events: Arc<RuntimeEventPublisher>,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError> {
    let SystemRuntimeLanes {
        observations:
            SystemObservationLanes {
                host,
                cpu,
                memory,
                storage,
                network,
                gpu,
                containers,
            },
        auxiliary:
            SystemAuxiliaryLanes {
                hardware_inventory,
                gpu_engine_rows,
                npu_inventory,
                smbios_memory,
                rapl_power,
                msr_readout,
            },
    } = lanes;
    let SystemExecutors {
        observations:
            SystemObservationExecutors {
                host: mut execute_host,
                cpu: mut execute_cpu,
                memory: mut execute_memory,
                storage: mut execute_storage,
                network: mut execute_network,
                gpu: mut execute_gpu,
                containers: mut execute_containers,
            },
        auxiliary:
            SystemAuxiliaryExecutors {
                hardware_inventory: mut execute_hardware_inventory,
                gpu_engine_rows: execute_gpu_engine_rows,
                npu_inventory: execute_npu_inventory,
                smbios_memory: execute_smbios_memory,
                rapl_power: execute_rapl_power,
                msr_readout: execute_msr_readout,
            },
    } = executors;

    spawn_lazy_observation_lane(
        workers,
        host,
        events.clone(),
        move |HostTelemetryRequest { revision }| {
            execute_host(clock_ms()).map(|value| (revision, value))
        },
        |(revision, observation)| {
            PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Host {
                revision,
                observation: Box::new(observation),
            })
        },
    )?;
    spawn_lazy_observation_lane(
        workers,
        cpu,
        events.clone(),
        move |CpuTelemetryRequest { revision }| {
            execute_cpu(clock_ms()).map(|value| (revision, value))
        },
        |(revision, observation)| {
            PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Cpu {
                revision,
                observation: Box::new(observation),
            })
        },
    )?;
    spawn_lazy_observation_lane(
        workers,
        memory,
        events.clone(),
        move |MemoryTelemetryRequest { revision }| {
            execute_memory(clock_ms()).map(|value| (revision, value))
        },
        |(revision, observation)| {
            PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Memory {
                revision,
                observation: Box::new(observation),
            })
        },
    )?;
    spawn_lazy_observation_lane(
        workers,
        containers,
        events.clone(),
        move |ContainerRollupRequest::Refresh| execute_containers(clock_ms()),
        |rollup| PlatformEvent::Containers(ContainerRollupEvent::Snapshot(Box::new(rollup))),
    )?;
    spawn_lazy_observation_lane(
        workers,
        storage,
        events.clone(),
        move |StorageTelemetryRequest { revision }| {
            execute_storage(clock_ms()).map(|value| (revision, value))
        },
        |(revision, observation)| {
            PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Storage {
                revision,
                observation: Box::new(observation),
            })
        },
    )?;
    spawn_lazy_observation_lane(
        workers,
        network,
        events.clone(),
        move |NetworkTelemetryRequest { revision }| {
            execute_network(clock_ms()).map(|value| (revision, value))
        },
        |(revision, observation)| {
            PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Network {
                revision,
                observation: Box::new(observation),
            })
        },
    )?;
    spawn_lazy_observation_lane(
        workers,
        gpu,
        events.clone(),
        move |GpuTelemetryRequest { revision }| {
            execute_gpu(clock_ms()).map(|value| (revision, value))
        },
        |(revision, observation)| {
            PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Gpu {
                revision,
                observation: Box::new(observation),
            })
        },
    )?;
    if let (Some(receiver), Some(execute)) = (gpu_engine_rows, execute_gpu_engine_rows) {
        spawn_gpu_engine_rows_lane(workers, receiver, events.clone(), execute)?;
    }
    if let (Some(receiver), Some(execute)) = (npu_inventory, execute_npu_inventory) {
        spawn_npu_inventory_lane(workers, receiver, events.clone(), execute, clock_ms)?;
    }
    if let (Some(receiver), Some(execute)) = (smbios_memory, execute_smbios_memory) {
        snapshot_lanes::spawn_smbios_memory_lane(workers, receiver, events.clone(), execute)?;
    }
    if let (Some(receiver), Some(execute)) = (rapl_power, execute_rapl_power) {
        snapshot_lanes::spawn_rapl_power_lane(workers, receiver, events.clone(), execute)?;
    }
    if let (Some(receiver), Some(execute)) = (msr_readout, execute_msr_readout) {
        snapshot_lanes::spawn_msr_readout_lane(workers, receiver, events.clone(), execute)?;
    }
    spawn_lazy_observation_lane(
        workers,
        hardware_inventory,
        events,
        move |HardwareInventoryRequest::Refresh| execute_hardware_inventory(),
        |snapshot| {
            PlatformEvent::HardwareInventory(HardwareInventoryEvent::Snapshot(Box::new(snapshot)))
        },
    )
}

/// Spawn the NPU accelerator inventory lane: one bounded executor call per
/// queued request, answered with exactly one correlated publication -- a
/// sorted device list on success (an empty list is the honest no-NPU host), a
/// typed failure snapshot otherwise. Health is recorded per answer so the
/// catalog reflects the latest enumeration outcome.
fn spawn_npu_inventory_lane(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<NpuInventoryRequest>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: Box<NpuInventoryExecutor>,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError> {
    let lane = CapabilityId::ACCELERATOR_NPU.to_string();
    spawn_or_register_lane(
        workers,
        Some(CapabilityId::ACCELERATOR_NPU),
        receiver,
        publisher,
        execute,
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = crate::delivery::LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                let observed_at_ms = clock_ms();
                let (snapshot, health) = match crate::delivery::execute_isolated(
                    &panic_notes,
                    crate::delivery::ProviderPanicContext {
                        lane: lane.clone(),
                        capability: queued.capability.clone(),
                        request_id: queued.request_id,
                    },
                    || {
                        let mut execute = execute
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        execute(observed_at_ms)
                    },
                ) {
                    Ok(snapshot) => (snapshot, CapabilityHealth::Available),
                    Err(failure) => (
                        NpuInventorySnapshot::failed(
                            failure.kind(),
                            format!("provider failure: {failure:?}"),
                            observed_at_ms,
                        ),
                        CapabilityHealth::Unavailable(failure),
                    ),
                };
                let event = PlatformEvent::NpuInventory(NpuInventoryEvent::Update(snapshot));
                if crate::delivery::shutdown_requested(&shutdown)
                    || publisher
                        .publish_health(
                            queued.request_id,
                            queued.capability,
                            queued.provider,
                            event,
                            health,
                        )
                        .is_stop()
                {
                    break;
                }
            }
        },
    )
}

/// Spawn the per-engine GPU utilization lane: one bounded executor call per
/// queued request, answered with exactly one correlated publication — real
/// rows on success, a typed failure snapshot otherwise (never a fabricated
/// row). Health is recorded per answer so the catalog reflects the latest
/// helper outcome, mirroring the directory-usage lane's terminal handling.
fn spawn_gpu_engine_rows_lane(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<GpuEngineRowsRequest>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: Box<GpuEngineRowsExecutor>,
) -> Result<(), WorkerSpawnError> {
    let lane = CapabilityId::TELEMETRY_GPU_ENGINES.to_string();
    spawn_or_register_lane(
        workers,
        Some(CapabilityId::TELEMETRY_GPU_ENGINES),
        receiver,
        publisher,
        execute,
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = crate::delivery::LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                let request = queued.payload;
                let (snapshot, health) = match crate::delivery::execute_isolated(
                    &panic_notes,
                    crate::delivery::ProviderPanicContext {
                        lane: lane.clone(),
                        capability: queued.capability.clone(),
                        request_id: queued.request_id,
                    },
                    || {
                        let mut execute = execute
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        execute(&request)
                    },
                ) {
                    Ok(snapshot) => (snapshot, CapabilityHealth::Available),
                    Err(failure) => (
                        GpuEngineRowsSnapshot::failed(
                            request.device_id,
                            failure.kind(),
                            format!("provider failure: {failure:?}"),
                        ),
                        CapabilityHealth::Unavailable(failure),
                    ),
                };
                let event = PlatformEvent::GpuEngineRows(GpuEngineRowsEvent::Update(snapshot));
                if crate::delivery::shutdown_requested(&shutdown)
                    || publisher
                        .publish_health(
                            queued.request_id,
                            queued.capability,
                            queued.provider,
                            event,
                            health,
                        )
                        .is_stop()
                {
                    break;
                }
            }
        },
    )
}

#[cfg(test)]
#[path = "../tests/headless/system.rs"]
mod tests;

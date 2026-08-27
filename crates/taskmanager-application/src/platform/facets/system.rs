//! System telemetry and hardware-inventory capability ports and events.
//!
//! Defines the six-domain telemetry requests, `SystemTelemetryRevision`,
//! `SystemTelemetryDomainOutcome`, and the `SystemFacets` group of
//! independently optional ports.

use std::sync::Arc;

use taskmanager_core::ContainerRollup;
use taskmanager_platform_contract::{
    CapabilityId, CompositeSourceSnapshot, RequestId, RequestPort, SubmissionError,
};

use crate::{
    CpuTelemetryObservation, GpuTelemetryObservation, HardwareInfo, HostRuntimeObservation,
    MemoryTelemetryObservation, NetworkTelemetryObservation, StorageTelemetryObservation,
};

use super::gpu_engine_rows::GpuEngineRowsRequestPort;
use super::npu_inventory::NpuInventoryRequestPort;

/// Application-owned generation correlating one six-domain refresh.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemTelemetryRevision(u64);

impl SystemTelemetryRevision {
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

macro_rules! system_refresh_request {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name {
            pub revision: SystemTelemetryRevision,
        }
    };
}

system_refresh_request!(HostTelemetryRequest);
system_refresh_request!(CpuTelemetryRequest);
system_refresh_request!(MemoryTelemetryRequest);
system_refresh_request!(StorageTelemetryRequest);
system_refresh_request!(NetworkTelemetryRequest);
system_refresh_request!(GpuTelemetryRequest);

bind_request_capability!(HostTelemetryRequest, CapabilityId::TELEMETRY_HOST);
bind_request_capability!(CpuTelemetryRequest, CapabilityId::TELEMETRY_CPU);
bind_request_capability!(MemoryTelemetryRequest, CapabilityId::TELEMETRY_MEMORY);
bind_request_capability!(StorageTelemetryRequest, CapabilityId::TELEMETRY_STORAGE);
bind_request_capability!(NetworkTelemetryRequest, CapabilityId::TELEMETRY_NETWORK);
bind_request_capability!(GpuTelemetryRequest, CapabilityId::TELEMETRY_GPU);

/// One independently scheduled domain completion.
#[derive(Clone, Debug)]
pub enum SystemTelemetryDomainEvent {
    Host {
        revision: SystemTelemetryRevision,
        observation: Box<HostRuntimeObservation>,
    },
    Cpu {
        revision: SystemTelemetryRevision,
        observation: Box<CpuTelemetryObservation>,
    },
    Memory {
        revision: SystemTelemetryRevision,
        observation: Box<MemoryTelemetryObservation>,
    },
    Storage {
        revision: SystemTelemetryRevision,
        observation: Box<StorageTelemetryObservation>,
    },
    Network {
        revision: SystemTelemetryRevision,
        observation: Box<NetworkTelemetryObservation>,
    },
    Gpu {
        revision: SystemTelemetryRevision,
        observation: Box<GpuTelemetryObservation>,
    },
}

impl SystemTelemetryDomainEvent {
    #[must_use]
    pub const fn revision(&self) -> SystemTelemetryRevision {
        match self {
            Self::Host { revision, .. }
            | Self::Cpu { revision, .. }
            | Self::Memory { revision, .. }
            | Self::Storage { revision, .. }
            | Self::Network { revision, .. }
            | Self::Gpu { revision, .. } => *revision,
        }
    }

    #[must_use]
    pub const fn domain(&self) -> super::super::SystemTelemetryDomain {
        match self {
            Self::Host { .. } => super::super::SystemTelemetryDomain::Host,
            Self::Cpu { .. } => super::super::SystemTelemetryDomain::Cpu,
            Self::Memory { .. } => super::super::SystemTelemetryDomain::Memory,
            Self::Storage { .. } => super::super::SystemTelemetryDomain::Storage,
            Self::Network { .. } => super::super::SystemTelemetryDomain::Network,
            Self::Gpu { .. } => super::super::SystemTelemetryDomain::Gpu,
        }
    }

    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        match self {
            Self::Host { .. } => capability == &CapabilityId::TELEMETRY_HOST,
            Self::Cpu { .. } => capability == &CapabilityId::TELEMETRY_CPU,
            Self::Memory { .. } => capability == &CapabilityId::TELEMETRY_MEMORY,
            Self::Storage { .. } => capability == &CapabilityId::TELEMETRY_STORAGE,
            Self::Network { .. } => capability == &CapabilityId::TELEMETRY_NETWORK,
            Self::Gpu { .. } => capability == &CapabilityId::TELEMETRY_GPU,
        }
    }
}

/// One application-correlated system-domain completion.
///
/// Runtime provider failures do not contain an observation payload, but they
/// are still a real completed sample and must advance typed history with a
/// gap. Submission failures are deliberately excluded because no runtime
/// request/event correlation exists for them.
#[derive(Clone, Debug)]
pub enum SystemTelemetryDomainOutcome {
    Observed(SystemTelemetryDomainEvent),
    Unavailable {
        revision: SystemTelemetryRevision,
        domain: super::super::SystemTelemetryDomain,
        reason: super::super::SystemTelemetryUnavailable,
    },
}

impl SystemTelemetryDomainOutcome {
    #[must_use]
    pub const fn revision(&self) -> SystemTelemetryRevision {
        match self {
            Self::Observed(event) => event.revision(),
            Self::Unavailable { revision, .. } => *revision,
        }
    }

    #[must_use]
    pub const fn domain(&self) -> super::super::SystemTelemetryDomain {
        match self {
            Self::Observed(event) => event.domain(),
            Self::Unavailable { domain, .. } => *domain,
        }
    }
}

/// Honest result of six independent bounded submissions.
///
/// Accepted domains keep running when a sibling is busy or absent. This value
/// is deliberately not an aggregate `Result`.
#[derive(Clone, Debug)]
pub struct SystemTelemetrySubmission {
    pub revision: SystemTelemetryRevision,
    pub host: Result<RequestId, SubmissionError>,
    pub cpu: Result<RequestId, SubmissionError>,
    pub memory: Result<RequestId, SubmissionError>,
    pub storage: Result<RequestId, SubmissionError>,
    pub network: Result<RequestId, SubmissionError>,
    pub gpu: Result<RequestId, SubmissionError>,
    pub projection: super::super::ProjectedSystemTelemetry,
}

impl SystemTelemetrySubmission {
    #[must_use]
    pub fn has_pending_requests(&self) -> bool {
        self.host.is_ok()
            || self.cpu.is_ok()
            || self.memory.is_ok()
            || self.storage.is_ok()
            || self.network.is_ok()
            || self.gpu.is_ok()
    }

    #[must_use]
    pub fn into_request_results(self) -> Vec<Result<RequestId, SubmissionError>> {
        vec![
            self.host,
            self.cpu,
            self.memory,
            self.storage,
            self.network,
            self.gpu,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTelemetrySubmissionError {
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardwareInventoryRequest {
    Refresh,
}

bind_request_capability!(HardwareInventoryRequest, CapabilityId::HARDWARE_INVENTORY);

#[derive(Clone, Debug)]
pub enum HardwareInventoryEvent {
    Snapshot(Box<CompositeSourceSnapshot<HardwareInfo>>),
}

impl HardwareInventoryEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::HARDWARE_INVENTORY
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerRollupRequest {
    Refresh,
}

bind_request_capability!(ContainerRollupRequest, CapabilityId::CONTAINERS);

#[derive(Clone, Debug)]
pub enum ContainerRollupEvent {
    Snapshot(Box<ContainerRollup>),
}

impl ContainerRollupEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::CONTAINERS
    }
}

pub type HostTelemetryRequestPort = dyn RequestPort<Request = HostTelemetryRequest>;
pub type CpuTelemetryRequestPort = dyn RequestPort<Request = CpuTelemetryRequest>;
pub type MemoryTelemetryRequestPort = dyn RequestPort<Request = MemoryTelemetryRequest>;
pub type StorageTelemetryRequestPort = dyn RequestPort<Request = StorageTelemetryRequest>;
pub type NetworkTelemetryRequestPort = dyn RequestPort<Request = NetworkTelemetryRequest>;
pub type GpuTelemetryRequestPort = dyn RequestPort<Request = GpuTelemetryRequest>;
pub type HardwareInventoryRequestPort = dyn RequestPort<Request = HardwareInventoryRequest>;
pub type ContainerRollupRequestPort = dyn RequestPort<Request = ContainerRollupRequest>;

/// Independently optional system capability ports, grouped only for immutable
/// runtime composition.
#[derive(Clone, Default)]
pub struct SystemFacets {
    host: Option<Arc<HostTelemetryRequestPort>>,
    cpu: Option<Arc<CpuTelemetryRequestPort>>,
    memory: Option<Arc<MemoryTelemetryRequestPort>>,
    storage: Option<Arc<StorageTelemetryRequestPort>>,
    network: Option<Arc<NetworkTelemetryRequestPort>>,
    gpu: Option<Arc<GpuTelemetryRequestPort>>,
    hardware_inventory: Option<Arc<HardwareInventoryRequestPort>>,
    containers: Option<Arc<ContainerRollupRequestPort>>,
    gpu_engine_rows: Option<Arc<GpuEngineRowsRequestPort>>,
    npu_inventory: Option<Arc<NpuInventoryRequestPort>>,
}

impl SystemFacets {
    #[must_use]
    pub fn with_host(mut self, port: Arc<HostTelemetryRequestPort>) -> Self {
        self.host = Some(port);
        self
    }

    #[must_use]
    pub fn with_cpu(mut self, port: Arc<CpuTelemetryRequestPort>) -> Self {
        self.cpu = Some(port);
        self
    }

    #[must_use]
    pub fn with_memory(mut self, port: Arc<MemoryTelemetryRequestPort>) -> Self {
        self.memory = Some(port);
        self
    }

    #[must_use]
    pub fn with_storage(mut self, port: Arc<StorageTelemetryRequestPort>) -> Self {
        self.storage = Some(port);
        self
    }

    #[must_use]
    pub fn with_network(mut self, port: Arc<NetworkTelemetryRequestPort>) -> Self {
        self.network = Some(port);
        self
    }

    #[must_use]
    pub fn with_gpu(mut self, port: Arc<GpuTelemetryRequestPort>) -> Self {
        self.gpu = Some(port);
        self
    }

    #[must_use]
    pub fn with_hardware_inventory(mut self, port: Arc<HardwareInventoryRequestPort>) -> Self {
        self.hardware_inventory = Some(port);
        self
    }

    #[must_use]
    pub fn with_containers(mut self, port: Arc<ContainerRollupRequestPort>) -> Self {
        self.containers = Some(port);
        self
    }

    #[must_use]
    pub fn with_gpu_engine_rows(mut self, port: Arc<GpuEngineRowsRequestPort>) -> Self {
        self.gpu_engine_rows = Some(port);
        self
    }

    #[must_use]
    pub fn with_npu_inventory(mut self, port: Arc<NpuInventoryRequestPort>) -> Self {
        self.npu_inventory = Some(port);
        self
    }

    #[must_use]
    pub fn host(&self) -> Option<&HostTelemetryRequestPort> {
        self.host.as_deref()
    }

    #[must_use]
    pub fn cpu(&self) -> Option<&CpuTelemetryRequestPort> {
        self.cpu.as_deref()
    }

    #[must_use]
    pub fn memory(&self) -> Option<&MemoryTelemetryRequestPort> {
        self.memory.as_deref()
    }

    #[must_use]
    pub fn storage(&self) -> Option<&StorageTelemetryRequestPort> {
        self.storage.as_deref()
    }

    #[must_use]
    pub fn network(&self) -> Option<&NetworkTelemetryRequestPort> {
        self.network.as_deref()
    }

    #[must_use]
    pub fn gpu(&self) -> Option<&GpuTelemetryRequestPort> {
        self.gpu.as_deref()
    }

    #[must_use]
    pub fn hardware_inventory(&self) -> Option<&HardwareInventoryRequestPort> {
        self.hardware_inventory.as_deref()
    }

    #[must_use]
    pub fn containers(&self) -> Option<&ContainerRollupRequestPort> {
        self.containers.as_deref()
    }

    #[must_use]
    pub fn gpu_engine_rows(&self) -> Option<&GpuEngineRowsRequestPort> {
        self.gpu_engine_rows.as_deref()
    }

    #[must_use]
    pub fn npu_inventory(&self) -> Option<&NpuInventoryRequestPort> {
        self.npu_inventory.as_deref()
    }
}

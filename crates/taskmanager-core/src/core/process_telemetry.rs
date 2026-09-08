//! Provider-neutral read models for per-process telemetry.
//!
//! Platform providers may obtain these facts from procfs, eBPF, ETW, Endpoint
//! Security, vendor APIs, or another source. The shared model deliberately does
//! not encode how a value was collected.

mod capabilities;
mod connection;
mod containers;
mod environment;
mod gpu;
mod gpu_engines;
mod isolation;
mod namespaces;
mod open_files;
mod resources;
mod snapshot;
mod threads;

pub use capabilities::{CapabilityRiskLevel, LinuxCapability, ProcessCapabilities};
pub use connection::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionProviderKey, ConnectionState,
    ConnectionTransport, NetworkConnectionCounters, ProcessConnection, ProcessNetworkSnapshot,
};
pub use containers::{ContainerRollup, ContainerSummary};
pub use environment::{
    MAX_ENVIRONMENT_BYTES, MAX_ENVIRONMENT_ENTRIES, ProcessEnvironment, ProcessEnvironmentEntry,
};
pub use gpu::{ProcessGpuDevice, ProcessGpuSnapshot};
pub use gpu_engines::{
    GpuEngineClass, ProcessGpuEngineClass, ProcessGpuEngineKind, ProcessGpuEngineType,
    ProcessGpuEngineUsage, ProcessGpuEngines,
};
pub use isolation::{IsolationKind, ProcessIsolation};
pub use namespaces::{
    LinuxNamespaceAudit, LinuxNamespaceKind, NamespaceAuditEntry, NamespaceAuditStatus,
    parse_namespace_inode,
};
pub use open_files::{OpenFileEntry, OpenFileItem, OpenFileKind, ProcessOpenFiles};
pub use resources::{
    LimitValue, ProcessResourceObservations, ProcessResourceSnapshot, ResourceGroupCpuLimit,
    ResourceGroupLimitRequest, ResourceGroupMembership, ResourceLastObservation, ResourceLimit,
    ResourceLimitKind, ResourceObservation,
};
pub use snapshot::{ProcessIdentity, ProcessInsightSnapshot, ProcessTelemetrySnapshot};
pub use threads::{ProcessThreadInfo, ProcessThreads, ThreadState, ThreadWaitKind};

#[cfg(test)]
#[path = "../../tests/headless/process_telemetry.rs"]
mod tests;

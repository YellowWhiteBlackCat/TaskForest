//! Platform-neutral snapshot models grouped by telemetry capability domain.

mod availability;
mod counter;
mod cpu;
mod disk;
mod gpu;
mod gpu_engine_rows;
mod memory;
mod network;
mod system;

pub use availability::{
    ObservationWireError, OptionalObservation, OptionalObservationState, ScalarAvailability,
    ScalarObservation, ScalarObservationGroup, ScalarObservationSlot,
};
pub use counter::{CounterDelta, CumulativeCounter};
pub use cpu::{
    CpuFrequencySource, CpuMetrics, CpuPerformancePolicy, CpuScalarObservations,
    CpuTemperatureSource, MAX_TRACKED_LOGICAL_CPUS, cpu_usage_pct_observation,
};
pub use disk::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, DiskScalarObservations,
    SmartAvailability,
};
pub use gpu::{
    GpuEngine, GpuEngineKind, GpuEngineMetric, GpuEngineMetricPoint, GpuGraphicsApi,
    GpuMetricField, GpuMetricProvenance, GpuMetrics, GpuScalarObservations, GpuThrottleReason,
};
pub use gpu_engine_rows::{GpuEngineRowsFailure, GpuEngineRowsSnapshot};
pub use memory::{
    MemoryCompositionObservations, MemoryCompressionObservations, MemoryMetrics,
    MemoryModuleObservations, MemoryOptionalObservations, MemoryScalarObservations,
    VirtualMemoryCommitObservations,
};
pub use network::{
    NetworkAdapterType, NetworkMetrics, NetworkScalarObservations, NetworkWirelessObservations,
};
pub use system::{
    CpuTelemetryObservation, GpuTelemetryObservation, HostRuntimeFacts, HostRuntimeObservation,
    MemoryTelemetryObservation, NetworkTelemetryObservation, ProviderRuntimeState,
    StorageTelemetryObservation, SystemObservationState, SystemSnapshot, SystemTelemetryDomains,
};

// Keep the established `core::metrics::*` storage paths wire/source compatible
// while physical ownership lives in the dedicated storage capability module.
pub use crate::core::storage::{
    StorageConnection, StorageDeviceKind, StorageIdentityStability, StorageInterconnect,
    StorageProtocol,
};

#[cfg(test)]
#[path = "../../tests/headless/metrics.rs"]
mod tests;

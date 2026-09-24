//! Platform-neutral snapshot models grouped by telemetry capability domain.

mod availability;
mod counter;
mod cpu;
mod cpu_throttle;
mod disk;
mod gpu;
mod gpu_engine_rows;
mod load;
mod memory;
mod msr_readout;
mod network;
mod psi;
mod rapl_power;
mod smbios_memory;
mod system;

pub use availability::{
    ObservationWireError, OptionalObservation, OptionalObservationState, ScalarAvailability,
    ScalarObservation, ScalarObservationGroup, ScalarObservationSlot,
};
pub use counter::{CounterDelta, CumulativeCounter};
pub use cpu::{
    CpuFrequencySource, CpuIdleState, CpuInterruptSnapshot, CpuMetrics, CpuPackageMetrics,
    CpuPerformancePolicy, CpuScalarObservations, CpuTemperatureSource, MAX_TRACKED_LOGICAL_CPUS,
    cpu_usage_pct_observation,
};
pub use cpu_throttle::{
    CpuThrottleCounterSample, CpuThrottleFailure, CpuThrottlePackageCounters, CpuThrottleSnapshot,
    aggregate_package_throttle_counters,
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
pub use load::SystemLoadAverage;
pub use memory::{
    MemoryCompositionObservations, MemoryCompressionObservations, MemoryMetrics,
    MemoryModuleObservations, MemoryOptionalObservations, MemoryScalarObservations,
    VirtualMemoryCommitObservations,
};
pub use msr_readout::{
    MsrPackageReadout, MsrReadoutFailure, MsrReadoutSnapshot, MsrThermalStatusReadout,
};
pub use network::{
    NetworkAdapterType, NetworkMetrics, NetworkScalarObservations, NetworkWirelessObservations,
};
pub use psi::{PressureWindow, ResourcePressure, SystemPressureSnapshot};
pub use rapl_power::{RaplPackageRow, RaplPowerFailure, RaplPowerSnapshot};
pub use smbios_memory::{
    DmiIdentityFacts, SmbiosMemoryFailure, SmbiosMemorySnapshot, SmbiosModuleRow,
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

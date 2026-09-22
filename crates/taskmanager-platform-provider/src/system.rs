//! System-domain observation and on-demand readout provider traits: the
//! periodic host/cpu/memory/storage/network/gpu/container refresh contracts
//! plus the frontend-paced request/response facets (per-engine GPU rows, NPU
//! inventory, SMBIOS memory, CPU package power, CPU MSR readouts, CPU
//! thermal-throttle counters).

use taskmanager_core::{
    ContainerRollup, CpuTelemetryObservation, CpuThrottleSnapshot, DeviceId, GpuEngineRowsSnapshot,
    GpuTelemetryObservation, HardwareInfo, HostRuntimeObservation, MemoryTelemetryObservation,
    MsrReadoutSnapshot, NetworkTelemetryObservation, NpuInventorySnapshot, RaplPowerSnapshot,
    SmbiosMemorySnapshot, StorageTelemetryObservation,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};

pub trait HostTelemetryProvider: Send + 'static {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<HostRuntimeObservation, ProviderFailure>;
}

pub trait CpuTelemetryProvider: Send + 'static {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<CpuTelemetryObservation, ProviderFailure>;
}

pub trait MemoryTelemetryProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<MemoryTelemetryObservation, ProviderFailure>;
}

pub trait StorageTelemetryProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StorageTelemetryObservation, ProviderFailure>;
}

pub trait NetworkTelemetryProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NetworkTelemetryObservation, ProviderFailure>;
}

pub trait GpuTelemetryProvider: Send + 'static {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<GpuTelemetryObservation, ProviderFailure>;
}

pub trait HardwareInventoryProvider: Send + 'static {
    fn refresh(&mut self) -> Result<CompositeSourceSnapshot<HardwareInfo>, ProviderFailure>;
}

pub trait ContainerRollupProvider: Send + 'static {
    fn refresh(&mut self, now_ms: u64) -> Result<ContainerRollup, ProviderFailure>;
}

/// On-demand per-engine GPU utilization reads (capability
/// `telemetry.gpu.engines`).
///
/// One call = ONE bounded helper invocation for `device_id`, never an internal
/// poll loop: request pacing belongs to the frontend. Implementations answer
/// with either real rows or a typed failure snapshot — never fabricated zeros.
pub trait GpuEngineRowsProvider: Send + 'static {
    fn read_engine_rows(
        &mut self,
        device_id: &DeviceId,
    ) -> Result<GpuEngineRowsSnapshot, ProviderFailure>;
}

/// On-demand NPU accelerator inventory reads (capability `accelerator.npu`).
///
/// One call = ONE bounded enumeration, never an internal poll loop: request
/// pacing belongs to the frontend. Implementations answer with a sorted
/// device list — an empty list on a host without an NPU is an honest success —
/// or a typed failure snapshot; utilization facts inside each device stay
/// typed and are never fabricated.
pub trait NpuInventoryProvider: Send + 'static {
    fn read_inventory(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NpuInventorySnapshot, ProviderFailure>;
}

/// On-demand SMBIOS memory inventory reads (capability
/// `telemetry.memory.smbios`).
///
/// One call = ONE bounded helper invocation, never an internal poll loop:
/// request pacing belongs to the frontend (the escalation discipline forbids
/// auto-triggering the OS-native prompt). Implementations answer with real
/// slot/module rows or a typed failure snapshot — never fabricated inventory.
pub trait SmbiosMemoryProvider: Send + 'static {
    fn read_memory_smbios(&mut self) -> Result<SmbiosMemorySnapshot, ProviderFailure>;
}

/// On-demand CPU package-power reads (capability
/// `telemetry.cpu.package_power`).
///
/// One call = ONE bounded RAPL helper sample (the helper's fixed window),
/// never an internal poll loop: request pacing belongs to the frontend.
/// Implementations answer with real per-package watt figures or a typed
/// failure snapshot — never a fabricated zero-watt reading.
pub trait RaplPowerProvider: Send + 'static {
    fn read_package_power(&mut self) -> Result<RaplPowerSnapshot, ProviderFailure>;
}

/// On-demand CPU MSR readouts (capability `telemetry.cpu.msr`).
///
/// One call = ONE bounded MSR helper invocation, never an internal poll
/// loop: request pacing belongs to the frontend (the escalation discipline
/// forbids auto-triggering the OS-native prompt). Implementations answer with
/// real per-node register rows or a typed failure snapshot — never a
/// fabricated zero for a register the CPU does not implement.
pub trait MsrReadoutProvider: Send + 'static {
    fn read_msr_readouts(&mut self) -> Result<MsrReadoutSnapshot, ProviderFailure>;
}

/// On-demand CPU thermal-throttle trigger-counter reads (capability
/// `telemetry.cpu.throttle`).
///
/// One call = ONE bounded read of the cumulative
/// `thermal_throttle/{core,package}_throttle_count` counters, aggregated for
/// the whole host by the shared core rule. Implementations answer with real
/// per-package counters — a counter the host does not expose stays absent on
/// its row — or a typed failure snapshot; never a fabricated zero.
pub trait CpuThrottleProvider: Send + 'static {
    fn read_cpu_throttle(&mut self) -> Result<CpuThrottleSnapshot, ProviderFailure>;
}

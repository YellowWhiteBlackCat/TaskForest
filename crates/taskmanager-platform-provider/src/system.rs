use taskmanager_core::{
    ContainerRollup, CpuTelemetryObservation, DeviceId, GpuEngineRowsSnapshot,
    GpuTelemetryObservation, HardwareInfo, HostRuntimeObservation, MemoryTelemetryObservation,
    NetworkTelemetryObservation, NpuInventorySnapshot, StorageTelemetryObservation,
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

//! Audited native Windows PDH performance counters for GPU engine and CPU frequency.

use crate::WindowsApiError;

#[cfg(windows)]
const MAX_PDH_BUFFER_BYTES: usize = 4 * 1024 * 1024;

#[cfg(windows)]
const MAX_PDH_ITEMS: usize = 65_536;

#[cfg(windows)]
const MAX_PDH_NAME_UTF16: usize = 512;

#[cfg(windows)]
const MAX_PROCESSOR_CORES: usize = 4096;

mod counters;
mod cpu;
mod gpu;

/// Breakdown per individual engine type (e.g. 3D, Copy, Video Decode, Compute, Neural).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuEngineDetail {
    pub engine_name: String,
    pub utilization_pct: f32,
}

/// An aggregated GPU utilization sample per adapter LUID.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuEngineSample {
    /// 64-bit adapter LUID (high << 32 | low).
    pub luid: u64,
    /// Total utilization percentage across engines for this adapter (0.0..100.0).
    pub utilization_pct: f32,
    /// Breakdown per engine type.
    pub engines: Vec<WindowsGpuEngineDetail>,
}

/// Dynamic processor frequency readings from PDH.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsCpuFrequencySample {
    /// Aggregate processor frequency across all cores in MHz.
    pub total_frequency_mhz: Option<u64>,
    /// Per-logical-core dynamic frequency in MHz.
    pub per_core_frequency_mhz: Vec<Option<u64>>,
}

/// Per-adapter video-memory usage from `\GPU Adapter Memory(*)` (WDDM 2.0+,
/// the same source Task Manager's GPU memory readouts use).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuAdapterMemorySample {
    /// Adapter instance name reported by PDH (e.g. "Intel(R) Arc(TM) Graphics").
    pub instance_name: String,
    /// 64-bit adapter LUID (high << 32 | low), when the instance name carries one.
    pub luid: Option<u64>,
    /// Current dedicated video-memory usage in bytes, when observed.
    pub dedicated_usage_bytes: Option<u64>,
    /// Current shared system-memory usage in bytes, when observed.
    pub shared_usage_bytes: Option<u64>,
}

/// One per-process GPU engine utilization row from `\GPU Engine(*)`, with the
/// pid, adapter LUID, and engine type parsed out of the PDH instance name and
/// sibling engine instances of the same type summed per (pid, LUID, type).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuEngineInstanceSample {
    pub pid: u32,
    /// 64-bit adapter LUID (high << 32 | low).
    pub luid: u64,
    /// Engine type display label parsed from `engtype_` (e.g. "3D", "Video Decode", "Neural").
    pub engine_type: String,
    pub utilization_pct: f32,
}

/// Per-process dedicated/shared GPU memory from `\GPU Process Memory(*)`
/// (WDDM 2.0+, Task Manager's own per-process GPU memory source), aggregated
/// per (pid, adapter LUID).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuProcessMemorySample {
    pub pid: u32,
    /// 64-bit adapter LUID (high << 32 | low).
    pub luid: u64,
    pub dedicated_bytes: u64,
    pub shared_bytes: u64,
}

/// Query active GPU engine utilization percentages grouped by adapter LUID.
#[must_use = "inspect GPU engine utilization query result"]
pub fn query_gpu_engine_utilization() -> Result<Vec<WindowsGpuEngineSample>, WindowsApiError> {
    #[cfg(windows)]
    {
        gpu::query_gpu_engine_utilization_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query per-process GPU engine utilization rows, one aggregated row per
/// (pid, adapter LUID, engine type).
#[must_use = "inspect GPU engine instance query result"]
pub fn query_gpu_engine_instances() -> Result<Vec<WindowsGpuEngineInstanceSample>, WindowsApiError>
{
    #[cfg(windows)]
    {
        gpu::query_gpu_engine_instances_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query per-process dedicated/shared GPU memory usage aggregated per
/// (pid, adapter LUID). A host whose WDDM lacks the counter set fails the
/// counter add, which keeps the same typed classification as
/// [`query_gpu_adapter_memory`].
#[must_use = "inspect GPU process memory query result"]
pub fn query_gpu_process_memory() -> Result<Vec<WindowsGpuProcessMemorySample>, WindowsApiError> {
    #[cfg(windows)]
    {
        gpu::query_gpu_process_memory_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query current dedicated/shared video-memory usage per GPU adapter via PDH.
#[must_use = "inspect GPU adapter memory query result"]
pub fn query_gpu_adapter_memory() -> Result<Vec<WindowsGpuAdapterMemorySample>, WindowsApiError> {
    #[cfg(windows)]
    {
        gpu::query_gpu_adapter_memory_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query dynamic processor frequencies from Windows performance counters.
#[must_use = "inspect dynamic processor frequency query result"]
pub fn query_cpu_dynamic_frequencies() -> Result<WindowsCpuFrequencySample, WindowsApiError> {
    #[cfg(windows)]
    {
        cpu::query_cpu_dynamic_frequencies_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_pdh.rs"]
mod tests;

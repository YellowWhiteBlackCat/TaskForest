use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

use super::gpu_engines::ProcessGpuEngines;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessGpuDevice {
    pub device_id: String,
    pub memory_bytes: Option<u64>,
    pub utilization_pct: Option<f32>,
    pub engine_time_ns: Option<u64>,
}

/// Per-process GPU telemetry.
///
/// `devices` carries the per-PCI-device rollup (collapsed across engines) while
/// `engines` carries the per-engine-class breakdown collected from
/// `/proc/<pid>/fdinfo/<fd>`. The two have independent collection states because
/// they are read through different procfs trees: the device rollup scans
/// `fdinfo/` directly, the engine breakdown enumerates `fd/` and keeps only the
/// descriptors that resolve to `/dev/dri/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProcessGpuSnapshot {
    pub state: DeviceState,
    pub devices: Vec<ProcessGpuDevice>,
    pub engines: ProcessGpuEngines,
}

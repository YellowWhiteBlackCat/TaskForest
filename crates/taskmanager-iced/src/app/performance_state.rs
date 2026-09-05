//! Iced-only Performance selection and bounded renderer history state.

use super::PerfDevice;
use crate::perf_history::ProcessPerfHistory;

pub(crate) struct PerformanceState {
    pub(crate) last_sampled_snapshot_ms: Option<u64>,
    pub(crate) selected_device: PerfDevice,
    pub(crate) sidebar_visible: bool,
    pub(crate) process_history: Option<ProcessPerfHistory>,
    pub(crate) gpu_engines_expanded: bool,
}

impl Default for PerformanceState {
    fn default() -> Self {
        Self {
            last_sampled_snapshot_ms: None,
            selected_device: PerfDevice::default(),
            sidebar_visible: true,
            process_history: None,
            gpu_engines_expanded: true,
        }
    }
}

//! Pure data-layer capacity fold for the per-partition panel (ARCH.md §4.0):
//! the typed partition observation reads live here so the render module only
//! paints the folded usage.

use crate::core::device_state::DeviceStatus;
use crate::core::metrics::DiskPartition;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PartitionUsage {
    Current { used: u64, free: u64, total: u64 },
    Unavailable(DeviceStatus),
}

pub(super) fn partition_usage(partition: &DiskPartition) -> PartitionUsage {
    match (
        partition.current_used_bytes(),
        partition.current_free_bytes(),
        partition.current_capacity_bytes(),
    ) {
        (Some(used), Some(free), Some(total)) => PartitionUsage::Current { used, free, total },
        _ => PartitionUsage::Unavailable(partition.device_state.status),
    }
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_perf_views_partition_stats_stats_tests.rs"]
mod tests;

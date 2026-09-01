//! Pure formatting for optional, platform-neutral memory detail rows.

use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};

pub(super) fn virtual_memory_commit_readout(
    memory: &MemoryMetrics,
    units: UnitPreferences,
) -> Option<String> {
    match (
        memory.current_committed_bytes(),
        memory.current_commit_limit_bytes(),
    ) {
        (Some(committed), Some(limit)) if limit > 0 => {
            Some(units.format_quantity_pair(committed, limit, QuantityFamily::Memory, false))
        }
        _ => None,
    }
}

pub(super) fn compressed_swap_readout(
    memory: &MemoryMetrics,
    units: UnitPreferences,
) -> Option<String> {
    match (
        memory.current_compressed_swap_used_bytes(),
        memory.current_compressed_swap_capacity_bytes(),
    ) {
        (Some(used), Some(capacity)) if capacity > 0 => {
            Some(units.format_quantity_pair(used, capacity, QuantityFamily::Memory, false))
        }
        _ => None,
    }
}

/// Format the compression depth as its own stat. Keeping it separate from the
/// used/capacity pair is a right-rail contract: the pair remains a normal
/// value row and the optional ratio gets its own bounded row instead of
/// creating one compound line that can evict the label at narrow widths.
pub(super) fn compression_ratio_readout(memory: &MemoryMetrics) -> Option<String> {
    memory
        .current_compressed_swap_ratio()
        .map(|ratio| format!("{ratio:.1}:1"))
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_memory_details_tests.rs"]
mod tests;

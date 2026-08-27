//! Pure formatting for optional, platform-neutral memory detail rows.

use crate::core::metrics::MemoryMetrics;
use crate::gpui_app::formatting::{DisplayUnits, UnitKind};

pub(super) fn virtual_memory_commit_readout(
    memory: &MemoryMetrics,
    units: DisplayUnits,
) -> Option<String> {
    match (
        memory.current_committed_bytes(),
        memory.current_commit_limit_bytes(),
    ) {
        (Some(committed), Some(limit)) if limit > 0 => {
            Some(units.format_pair(committed, limit, UnitKind::Memory, false))
        }
        _ => None,
    }
}

pub(super) fn compressed_swap_readout(
    memory: &MemoryMetrics,
    units: DisplayUnits,
) -> Option<String> {
    let readout = match (
        memory.current_compressed_swap_used_bytes(),
        memory.current_compressed_swap_capacity_bytes(),
    ) {
        (Some(used), Some(capacity)) if capacity > 0 => {
            units.format_pair(used, capacity, UnitKind::Memory, false)
        }
        _ => return None,
    };
    // The compression depth follows the used/capacity pair only when the
    // core guarded ratio is derivable (both mm_stat sizes current).
    match memory.current_compressed_swap_ratio() {
        Some(ratio) => Some(format!(
            "{readout} · {} {ratio:.1}:1",
            crate::i18n::t("mem.compression_ratio")
        )),
        None => Some(readout),
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_memory_details_tests.rs"]
mod tests;

//! Pure Memory-page observation projection.

use crate::gpui_app::formatting::{self, missing_value};
use taskmanager_application::i18n;
use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_shell::viewmodel::StatRow;

use super::memory_details::{
    compressed_swap_readout, compression_ratio_readout, virtual_memory_commit_readout,
};

pub(super) struct MemoryPageStats {
    pub rows: Vec<StatRow>,
    pub total_readout: String,
    pub has_swap: bool,
}

pub(super) fn memory_page_stats(memory: &MemoryMetrics, units: UnitPreferences) -> MemoryPageStats {
    let mut rows = vec![
        StatRow::text(
            i18n::t("mem.in_use"),
            match (
                memory.current_used_bytes(),
                memory.used_percentage_observed(),
            ) {
                (Some(used), Some(percentage)) => Some(format!(
                    "{} ({percentage:.0}%)",
                    units.format_quantity(used, QuantityFamily::Memory, false)
                )),
                (Some(used), None) => {
                    Some(units.format_quantity(used, QuantityFamily::Memory, false))
                }
                _ => None,
            },
        ),
        StatRow::text(
            i18n::t("mem.available"),
            memory
                .projected_available_bytes()
                .map(|value| units.format_quantity(value, QuantityFamily::Memory, false)),
        ),
        StatRow::text(
            i18n::t("mem.hardware_reserved"),
            memory
                .current_hardware_reserved_bytes()
                .map(|value| units.format_quantity(value, QuantityFamily::Memory, false)),
        ),
        StatRow::text(
            i18n::t("mem.cached"),
            memory
                .current_cached_bytes()
                .map(|value| units.format_quantity(value, QuantityFamily::Memory, false)),
        ),
        StatRow::pair(
            i18n::t("mem.swap"),
            match (
                memory.current_swap_used_bytes(),
                memory.current_swap_total_bytes(),
            ) {
                (Some(used), Some(total)) => {
                    Some(units.format_quantity_pair(used, total, QuantityFamily::Memory, false))
                }
                _ => None,
            },
        ),
        StatRow::text(
            i18n::t("common.speed"),
            memory
                .current_speed_mhz()
                .map(|speed| format!("{speed} MT/s")),
        ),
        StatRow::pair(
            i18n::t("mem.slots"),
            match (memory.current_slots_used(), memory.current_slots_total()) {
                (Some(used), Some(total)) => Some(format!("{used} / {total}")),
                _ => None,
            },
        ),
    ];
    if let Some(value) = memory.current_buffers_bytes() {
        rows.insert(
            4,
            StatRow::text(
                i18n::t("mem.buffers"),
                Some(units.format_quantity(value, QuantityFamily::Memory, false)),
            ),
        );
    }
    // ZFS hosts report the ARC as a reclaimable component; the row stays
    // hidden on every other host instead of rendering a fake zero.
    if let Some(arc) = memory.current_zfs_arc_bytes() {
        let swap_row = rows
            .iter()
            .position(|row| row.label() == i18n::t("mem.swap"));
        let insertion = swap_row.unwrap_or(rows.len());
        rows.insert(
            insertion,
            StatRow::text(
                i18n::t("mem.zfs_arc"),
                Some(units.format_quantity(arc, QuantityFamily::Memory, false)),
            ),
        );
    }
    if let Some(readout) = virtual_memory_commit_readout(memory, units) {
        rows.push(StatRow::text(i18n::t("mem.committed"), Some(readout)));
    }
    if let Some(readout) = compressed_swap_readout(memory, units) {
        rows.push(StatRow::text(i18n::t("mem.zram_swap"), Some(readout)));
    }
    if let Some(readout) = compression_ratio_readout(memory) {
        rows.push(StatRow::text(
            i18n::t("mem.compression_ratio"),
            Some(readout),
        ));
    }
    // The RAM the zram store actually consumes (`mm_stat` `mem_used_total`,
    // metadata included): a distinct fact from the swap-used view above, so
    // its own gated row — kept in lockstep with the iced row set.
    if let Some(ram) = memory.current_compressed_swap_memory_used_bytes() {
        rows.push(StatRow::text(
            i18n::t("mem.zram_ram_used"),
            Some(units.format_quantity(ram, QuantityFamily::Memory, false)),
        ));
    }
    if let Some(enabled) = memory.current_compressed_swap_cache_enabled() {
        rows.push(StatRow::text(
            i18n::t("mem.zswap"),
            Some(if enabled {
                i18n::t("common.enabled").into()
            } else {
                i18n::t("common.disabled").into()
            }),
        ));
    }
    if let Some(rate) = memory
        .current_used_rate_mib_per_sec()
        .filter(|rate| rate.abs() >= 0.05)
    {
        rows.push(StatRow::text(
            i18n::t("mem.usage_rate"),
            Some(formatting::format_signed_memory_rate_mib(units, rate)),
        ));
    }
    let swap_total = memory.current_swap_total_bytes();
    MemoryPageStats {
        rows,
        total_readout: optional_memory(memory.current_total_bytes(), units),
        has_swap: swap_total.is_some_and(|total| total > 0),
    }
}

pub(super) fn optional_memory(value: Option<u64>, units: UnitPreferences) -> String {
    value.map_or_else(missing_value, |value| {
        units.format_quantity(value, QuantityFamily::Memory, false)
    })
}

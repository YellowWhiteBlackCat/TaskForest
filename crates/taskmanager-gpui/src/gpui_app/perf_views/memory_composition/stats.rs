//! Pure data-layer folds for the memory-composition card (ARCH.md §8.1): the
//! typed memory observation reads (totals, swap, compressed swap, summary
//! tiles) live here so the render module only paints folded strings, shares,
//! and bars.

use taskmanager_shell::memory::MemSegmentKind;

use crate::gpui_app::formatting;
use taskmanager_application::i18n;
use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences, bytes_to_gib};

/// One colorless composition segment with its rendered width share already
/// folded (bytes / current total).
pub(super) struct SegmentShare {
    pub(super) kind: MemSegmentKind,
    pub(super) label: &'static str,
    pub(super) bytes: u64,
    pub(super) share: f32,
}

/// The breakdown math is shared (`taskmanager_shell::memory`) so every
/// frontend agrees on which segments exist; this fold attaches only the
/// rendered width fraction. The render wrapper maps kinds to theme colors.
pub(super) fn segment_shares(memory: &MemoryMetrics) -> Vec<SegmentShare> {
    let Some(total_bytes) = memory.current_total_bytes().filter(|total| *total > 0) else {
        return Vec::new();
    };
    let total_gib = bytes_to_gib(total_bytes);
    taskmanager_shell::memory::memory_segments(memory)
        .into_iter()
        .map(|seg| SegmentShare {
            share: (bytes_to_gib(seg.bytes) / total_gib) as f32,
            kind: seg.kind,
            label: seg.label,
            bytes: seg.bytes,
        })
        .collect()
}

/// Composition-card totals plus whether a swap bar should render at all.
pub(super) struct MemoryOverviewStats {
    pub(super) total_bytes: u64,
    pub(super) has_swap: bool,
}

pub(super) fn overview_stats(memory: &MemoryMetrics) -> MemoryOverviewStats {
    MemoryOverviewStats {
        total_bytes: memory.current_total_bytes().unwrap_or(0),
        has_swap: matches!(
            (
                memory.current_swap_used_bytes(),
                memory.current_swap_total_bytes()
            ),
            (Some(_), Some(total)) if total > 0
        ),
    }
}

/// Header labels for the composition card. Keep the typed reads in this
/// data-layer module so the render wrapper only consumes display-ready text.
pub(super) struct CompositionLabels {
    pub(super) used: String,
    pub(super) total: String,
    pub(super) pair: String,
}

pub(super) fn composition_labels(
    memory: &MemoryMetrics,
    units: UnitPreferences,
) -> CompositionLabels {
    let used = memory
        .current_used_bytes()
        .map_or_else(formatting::missing_value, |value| {
            units.format_quantity(value, QuantityFamily::Memory, false)
        });
    let total = memory
        .current_total_bytes()
        .map_or_else(formatting::missing_value, |value| {
            units.format_quantity(value, QuantityFamily::Memory, false)
        });
    let pair = match (memory.current_used_bytes(), memory.current_total_bytes()) {
        (Some(used), Some(total)) => {
            units.format_quantity_pair(used, total, QuantityFamily::Memory, false)
        }
        _ => formatting::missing_value(),
    };
    CompositionLabels { used, total, pair }
}

/// The three summary tiles' value/note strings (In use / Available / Swap).
pub(super) struct SummaryTiles {
    pub(super) used: String,
    pub(super) used_note: String,
    pub(super) available: String,
    pub(super) available_note: String,
    pub(super) swap: String,
    pub(super) swap_note: String,
}

pub(super) fn summary_tiles(memory: &MemoryMetrics, units: UnitPreferences) -> SummaryTiles {
    let used = memory
        .current_used_bytes()
        .map_or_else(formatting::missing_value, |value| {
            units.format_quantity(value, QuantityFamily::Memory, false)
        });
    let used_note = memory
        .used_percentage_observed()
        .map_or_else(formatting::missing_value, |value| format!("{value:.0}%"));
    // Kernel `MemAvailable` excludes the ZFS ARC; the tile reads the core
    // ARC-aware availability projection so a ZFS host doesn't look starved.
    let available = memory
        .projected_available_bytes()
        .map_or_else(formatting::missing_value, |value| {
            units.format_quantity(value, QuantityFamily::Memory, false)
        });
    let available_note =
        memory
            .current_total_bytes()
            .map_or_else(formatting::missing_value, |total| {
                format!(
                    "{} {}",
                    i18n::t("mem.of"),
                    units.format_quantity(total, QuantityFamily::Memory, false)
                )
            });
    let swap = match (
        memory.current_swap_used_bytes(),
        memory.current_swap_total_bytes(),
    ) {
        (Some(used), Some(total)) if total > 0 => {
            units.format_quantity(used, QuantityFamily::Memory, false)
        }
        _ => formatting::missing_value(),
    };
    let swap_note = memory
        .swap_percentage_observed()
        .map_or_else(formatting::missing_value, |value| format!("{value:.0}%"));

    SummaryTiles {
        used,
        used_note,
        available,
        available_note,
        swap,
        swap_note,
    }
}

/// Swap bar fold: the used share (0..=1) and the fully composed annotation
/// label, including the optional zram/zswap suffixes.
pub(super) struct SwapBarStats {
    pub(super) used_share: f32,
    pub(super) label: String,
}

pub(super) fn swap_bar_stats(
    memory: &MemoryMetrics,
    units: UnitPreferences,
) -> Option<SwapBarStats> {
    let used = memory.current_swap_used_bytes()?;
    let total_bytes = memory.current_swap_total_bytes()?.max(1);
    let total = bytes_to_gib(total_bytes);
    let used_share = (bytes_to_gib(used) / total).clamp(0.0, 1.0) as f32;
    let mut label = format!(
        "Swap  {}  ({:.0}%)",
        units.format_quantity_pair(used, total_bytes, QuantityFamily::Memory, false),
        used_share * 100.0,
    );
    if let Some(zram) = memory.current_compressed_swap_used_bytes()
        && zram > 0
    {
        label.push_str(&format!(
            "   ·   zram {}",
            units.format_quantity(zram, QuantityFamily::Memory, false)
        ));
    }
    // The RAM the store actually consumes (`mm_stat` `mem_used_total`):
    // distinct from the swap-used view above and the compressed size below.
    if let Some(ram) = memory
        .current_compressed_swap_memory_used_bytes()
        .filter(|v| *v > 0)
    {
        label.push_str(&format!(
            "   ·   {} {}",
            i18n::t("mem.zram_ram_used"),
            units.format_quantity(ram, QuantityFamily::Memory, false)
        ));
    }
    if let Some(ratio) = memory.current_compressed_swap_ratio() {
        label.push_str(&format!(
            "   ·   {} {ratio:.1}:1",
            i18n::t("mem.compression_ratio")
        ));
        if let (Some(original), Some(compressed)) = (
            memory.current_compressed_swap_original_bytes(),
            memory.current_compressed_swap_compressed_bytes(),
        ) {
            label.push_str(&format!(
                " · {} {} → {} {}",
                i18n::t("mem.compression_original"),
                units.format_quantity(original, QuantityFamily::Memory, false),
                i18n::t("mem.compression_compressed"),
                units.format_quantity(compressed, QuantityFamily::Memory, false),
            ));
        }
    }
    if memory.current_compressed_swap_cache_enabled() == Some(true) {
        label.push_str("   ·   zswap on");
    }
    Some(SwapBarStats { used_share, label })
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_perf_views_memory_composition_stats_tests.rs"]
mod tests;

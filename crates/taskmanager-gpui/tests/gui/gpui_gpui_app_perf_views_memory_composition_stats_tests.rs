use crate::core::metrics::MemoryMetrics;
use taskmanager_test_support::MemoryMetricsFixtureBuilder;

use super::{overview_stats, summary_tiles, swap_bar_stats};

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

fn measured_memory() -> MemoryMetrics {
    MemoryMetricsFixtureBuilder::new()
        .current_total_bytes(8 * GIB)
        .current_used_bytes(4 * GIB)
        .current_available_bytes(4 * GIB)
        .current_swap_total_bytes(2 * GIB)
        .current_swap_used_bytes(GIB)
        .build()
}

#[test]
fn summary_tiles_fold_measured_and_missing_memory_states() {
    let units = crate::gpui_app::formatting::DisplayUnits::default();
    let tiles = summary_tiles(&measured_memory(), units);
    assert_eq!(tiles.used, "4.00 GiB");
    assert_eq!(tiles.used_note, "50%");
    assert_eq!(tiles.available, "4.00 GiB");
    assert_eq!(
        tiles.available_note,
        format!("{} 8.00 GiB", crate::i18n::t("mem.of"))
    );
    assert_eq!(tiles.swap, "1.00 GiB");
    assert_eq!(tiles.swap_note, "50%");

    let missing = summary_tiles(&MemoryMetrics::default(), units);
    assert_eq!(missing.used, "—");
    assert_eq!(missing.used_note, "—");
    assert_eq!(missing.available, "—");
    assert_eq!(missing.available_note, "—");
    assert_eq!(missing.swap, "—");
    assert_eq!(missing.swap_note, "—");
}

#[test]
fn overview_stats_fold_totals_and_swap_presence() {
    let overview = overview_stats(&measured_memory());
    assert_eq!(overview.total_bytes, 8 * GIB);
    assert!(overview.has_swap);

    let missing = overview_stats(&MemoryMetrics::default());
    assert_eq!(missing.total_bytes, 0);
    assert!(!missing.has_swap);
}

#[test]
fn swap_bar_label_composes_zram_and_zswap_annotations() {
    let units = crate::gpui_app::formatting::DisplayUnits::default();
    let memory = MemoryMetricsFixtureBuilder::from_item(measured_memory())
        .compressed_swap_used_bytes(512 * MIB)
        .compressed_swap_cache_enabled(true)
        .build();

    let stats = swap_bar_stats(&memory, units).expect("measured swap renders");
    assert_eq!(stats.used_share, 0.5);
    assert_eq!(
        stats.label,
        "Swap  1.00 GiB / 2.00 GiB  (50%)   ·   zram 512 MiB   ·   zswap on"
    );

    let bare = swap_bar_stats(&measured_memory(), units).expect("measured swap renders");
    assert_eq!(bare.used_share, 0.5);
    assert_eq!(bare.label, "Swap  1.00 GiB / 2.00 GiB  (50%)");
}

#[test]
fn swap_bar_label_appends_the_guarded_zram_compression_depth() {
    let units = crate::gpui_app::formatting::DisplayUnits::default();
    let memory = MemoryMetricsFixtureBuilder::from_item(measured_memory())
        .compressed_swap_used_bytes(512 * MIB)
        .compressed_swap_original_bytes(3 * GIB)
        .compressed_swap_compressed_bytes(GIB)
        .compressed_swap_memory_used_bytes(256 * MIB)
        .build();

    // Exactly 3 GiB ÷ 1 GiB → a clean 3.0:1 with the orig→compressed pair,
    // plus the RAM the store actually consumes (`mm_stat` `mem_used_total`).
    let stats = swap_bar_stats(&memory, units).expect("measured swap renders");
    assert_eq!(
        stats.label,
        format!(
            "Swap  1.00 GiB / 2.00 GiB  (50%)   ·   zram 512 MiB   ·   {} 256 MiB   ·   {} 3.0:1 · {} 3.00 GiB → {} 1.00 GiB",
            crate::i18n::t("mem.zram_ram_used"),
            crate::i18n::t("mem.compression_ratio"),
            crate::i18n::t("mem.compression_original"),
            crate::i18n::t("mem.compression_compressed"),
        )
    );

    // A zero compressed size cannot yield a ratio and an empty store
    // consumes no RAM; both suffixes stay away.
    let zero = MemoryMetricsFixtureBuilder::from_item(measured_memory())
        .compressed_swap_used_bytes(512 * MIB)
        .compressed_swap_original_bytes(3 * GIB)
        .compressed_swap_compressed_bytes(0)
        .compressed_swap_memory_used_bytes(0)
        .build();
    assert_eq!(
        swap_bar_stats(&zero, units)
            .expect("measured swap renders")
            .label,
        "Swap  1.00 GiB / 2.00 GiB  (50%)   ·   zram 512 MiB"
    );
}

#[test]
fn available_tile_layers_the_zfs_arc_onto_kernel_availability() {
    let units = crate::gpui_app::formatting::DisplayUnits::default();
    let memory = MemoryMetricsFixtureBuilder::from_item(measured_memory())
        .zfs_arc_bytes(2 * GIB)
        .build();

    let tiles = summary_tiles(&memory, units);
    assert_eq!(tiles.available, "6.00 GiB");
    // The kernel fact underneath is untouched.
    assert_eq!(memory.current_available_bytes(), Some(4 * GIB));
}

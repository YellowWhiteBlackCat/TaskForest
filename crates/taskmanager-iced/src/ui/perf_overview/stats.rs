//! Memory statistics rows for the Iced Performance overview.

use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_shell::viewmodel::StatRow;

use crate::ui::format::memory_text_pref;

pub(crate) fn memory_stats_rows(
    memory: &MemoryMetrics,
    use_bytes: bool,
    use_base2: bool,
) -> Vec<StatRow> {
    let observed = super::projection::MemoryObservation::from(memory);
    let opt = |value: Option<u64>| value.map(|v| memory_text_pref(v, use_bytes, use_base2));
    let mut stats = vec![
        StatRow::text(t("mem.in_use"), opt(observed.used_bytes)),
        StatRow::text(t("mem.available"), opt(observed.projected_available_bytes)),
        StatRow::text(
            t("mem.hardware_reserved"),
            opt(observed.hardware_reserved_bytes),
        ),
        StatRow::text(t("mem.cached"), opt(observed.cached_bytes)),
        StatRow::pair(
            t("mem.swap"),
            match (observed.swap_used_bytes, observed.swap_total_bytes) {
                (Some(used), Some(total)) => Some(format!(
                    "{} / {}",
                    memory_text_pref(used, use_bytes, use_base2),
                    memory_text_pref(total, use_bytes, use_base2)
                )),
                _ => None,
            },
        ),
        StatRow::text(
            t("common.speed"),
            observed.speed_mhz.map(|value| format!("{value} MT/s")),
        ),
        StatRow::pair(
            t("mem.slots"),
            match (observed.slots_used, observed.slots_total) {
                (Some(used), Some(total)) => Some(format!("{used} / {total}")),
                _ => None,
            },
        ),
    ];
    if let Some(value) = observed.buffers_bytes {
        stats.insert(
            4,
            StatRow::text(
                t("mem.buffers"),
                Some(memory_text_pref(value, use_bytes, use_base2)),
            ),
        );
    }
    if let Some(arc) = observed.zfs_arc_bytes {
        let swap_row = stats
            .iter()
            .position(|row| row.label() == t("mem.swap"))
            .unwrap_or(stats.len());
        stats.insert(
            swap_row,
            StatRow::text(
                t("mem.zfs_arc"),
                Some(memory_text_pref(arc, use_bytes, use_base2)),
            ),
        );
    }
    if let (Some(committed), Some(limit)) = (observed.committed_bytes, observed.commit_limit_bytes)
        && limit > 0
    {
        stats.push(StatRow::pair(
            t("mem.committed"),
            Some(format!(
                "{} / {}",
                memory_text_pref(committed, use_bytes, use_base2),
                memory_text_pref(limit, use_bytes, use_base2)
            )),
        ));
    }
    if let (Some(used), Some(capacity)) = (
        observed.compressed_swap_used_bytes,
        observed.compressed_swap_capacity_bytes,
    ) && capacity > 0
    {
        let ratio = observed
            .compressed_swap_compression_ratio
            .map_or_else(String::new, |ratio| {
                format!(" · {} {ratio:.1}:1", t("mem.compression_ratio"))
            });
        stats.push(StatRow::pair(
            t("mem.zram_swap"),
            Some(format!(
                "{} / {}{ratio}",
                memory_text_pref(used, use_bytes, use_base2),
                memory_text_pref(capacity, use_bytes, use_base2)
            )),
        ));
        if let Some(ram) = observed.compressed_swap_memory_used_bytes {
            stats.push(StatRow::text(
                t("mem.zram_ram_used"),
                Some(memory_text_pref(ram, use_bytes, use_base2)),
            ));
        }
    }
    if let Some(on) = observed.compressed_swap_cache_enabled {
        let state = if on {
            t("common.enabled")
        } else {
            t("common.disabled")
        };
        stats.push(StatRow::text(t("mem.zswap"), Some(state.to_string())));
    }
    if let Some(rate) = observed
        .used_rate_mib_per_sec
        .filter(|rate| rate.abs() >= 0.05)
    {
        stats.push(StatRow::text(
            t("mem.usage_rate"),
            Some(signed_memory_rate_text(rate, use_bytes, use_base2)),
        ));
    }
    if let Some(rate) = memory.current_swap_in_bytes_per_sec() {
        stats.push(StatRow::text(
            t("mem.swap_in_rate"),
            Some(format!(
                "{}/s",
                memory_text_pref(rate, use_bytes, use_base2)
            )),
        ));
    }
    if let Some(rate) = memory.current_swap_out_bytes_per_sec() {
        stats.push(StatRow::text(
            t("mem.swap_out_rate"),
            Some(format!(
                "{}/s",
                memory_text_pref(rate, use_bytes, use_base2)
            )),
        ));
    }
    stats
}

pub(crate) fn signed_memory_rate_text(
    rate_mib_per_sec: f32,
    use_bytes: bool,
    use_base2: bool,
) -> String {
    let sign = if rate_mib_per_sec < 0.0 { "−" } else { "+" };
    let per_sec = (rate_mib_per_sec.abs() * 1024.0 * 1024.0).round() as u64;
    format!(
        "{sign}{}/s",
        memory_text_pref(per_sec, use_bytes, use_base2)
    )
}

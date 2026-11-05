//! The Performance stat rail's typed row keys and their pure folds.

use crate::pages::performance::metrics::memory_metrics;
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{bytes, missing_value};

/// The stat rail's typed row keys. A key is a semantic slot, not a localized
/// label: locales may rename labels, but the rail keeps one authority per
/// observation it rewrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatKey {
    MemoryUsed,
    MemoryAvailable,
    MemoryHardwareReserved,
    MemoryCached,
    MemoryBuffers,
    MemorySwap,
    MemorySpeed,
    MemorySlots,
    MemoryCommitted,
    MemoryZramSwap,
    MemoryCompressionRatio,
    MemoryZramRamUsed,
    MemoryZswap,
    MemoryUsageRate,
    MemorySwapInRate,
    MemorySwapOutRate,
}

/// One stat rail row's current value. Each key reads exactly one typed fact;
/// an absent observation prints the shared dash, never a fabricated zero.
pub(crate) fn stat_value(shell: &ShellApp, key: StatKey) -> String {
    let memory = memory_metrics(shell);
    let Some(memory) = memory else {
        return missing_value();
    };
    match key {
        StatKey::MemoryUsed => match (
            memory.current_used_bytes(),
            memory.used_percentage_observed(),
        ) {
            (Some(used), Some(percentage)) => {
                format!("{} ({percentage:.0}%)", bytes(used))
            }
            (Some(used), None) => bytes(used),
            _ => missing_value(),
        },
        StatKey::MemoryAvailable => memory
            .projected_available_bytes()
            .map_or_else(missing_value, bytes),
        StatKey::MemoryHardwareReserved => memory
            .current_hardware_reserved_bytes()
            .map_or_else(missing_value, bytes),
        StatKey::MemoryCached => memory
            .current_cached_bytes()
            .map_or_else(missing_value, bytes),
        StatKey::MemoryBuffers => memory
            .current_buffers_bytes()
            .map_or_else(missing_value, bytes),
        StatKey::MemorySwap => match (
            memory.current_swap_used_bytes(),
            memory.current_swap_total_bytes(),
        ) {
            (Some(used), Some(total)) => format!("{} / {}", bytes(used), bytes(total)),
            _ => missing_value(),
        },
        StatKey::MemorySpeed => memory
            .current_speed_mhz()
            .map_or_else(missing_value, |speed| format!("{speed} MT/s")),
        StatKey::MemorySlots => match (memory.current_slots_used(), memory.current_slots_total()) {
            (Some(used), Some(total)) => format!("{used} / {total}"),
            _ => missing_value(),
        },
        StatKey::MemoryCommitted => match (
            memory.current_committed_bytes(),
            memory.current_commit_limit_bytes(),
        ) {
            (Some(used), Some(total)) => format!("{} / {}", bytes(used), bytes(total)),
            _ => missing_value(),
        },
        StatKey::MemoryZramSwap => match (
            memory.current_compressed_swap_used_bytes(),
            memory.current_compressed_swap_capacity_bytes(),
        ) {
            (Some(used), Some(capacity)) if capacity > 0 => {
                format!("{} / {}", bytes(used), bytes(capacity))
            }
            _ => missing_value(),
        },
        StatKey::MemoryCompressionRatio => memory
            .current_compressed_swap_ratio()
            .map_or_else(missing_value, |ratio| format!("{ratio:.1}:1")),
        StatKey::MemoryZramRamUsed => memory
            .current_compressed_swap_memory_used_bytes()
            .map_or_else(missing_value, bytes),
        StatKey::MemoryZswap => {
            memory
                .current_compressed_swap_cache_enabled()
                .map_or_else(missing_value, |enabled| {
                    t(if enabled {
                        "common.enabled"
                    } else {
                        "common.disabled"
                    })
                    .to_owned()
                })
        }
        StatKey::MemoryUsageRate => memory
            .current_used_rate_mib_per_sec()
            .filter(|rate| rate.abs() >= 0.05)
            .map_or_else(missing_value, |rate| format!("{rate:+.1} MiB/s")),
        StatKey::MemorySwapInRate => memory
            .current_swap_in_bytes_per_sec()
            .map_or_else(missing_value, |rate| format!("{}/s", bytes(rate))),
        StatKey::MemorySwapOutRate => memory
            .current_swap_out_bytes_per_sec()
            .map_or_else(missing_value, |rate| format!("{}/s", bytes(rate))),
    }
}

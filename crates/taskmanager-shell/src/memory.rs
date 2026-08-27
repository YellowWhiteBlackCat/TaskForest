//! Shared memory-composition breakdown (ADR-020 single-source rule).
//!
//! The segment math — which categories exist and their byte counts — is
//! computed once here, from the typed [`MemoryMetrics`] accessors; each
//! frontend (gpui / ratatui TUI / iced) maps a segment's semantic `kind`
//! to its own theme color and renders the bar. This mirrors the
//! `bytes()` / `duration()` rule in [`crate::presentation`]: frontends never
//! re-derive the breakdown.
//!
//! Measured-zero is kept distinct from `None` (unavailable): a fully
//! measured-zero composition yields five segments, while an unknown
//! composition degrades to the two-segment in-use/available fallback. A
//! failed typed observation never replays a legacy `Option` value — the
//! `current_*` accessors return `None` when the observation is unavailable,
//! so the branches below fall through automatically.

use taskmanager_application::{MemoryMetrics, i18n::t};

/// Semantic role of one memory-composition segment. Each frontend maps this
/// to its own theme color (every frontend palette derives from the same
/// `taskmanager-theme` tokens, so the mapping is consistent): the primary
/// used hue for [`MemSegmentKind::Active`] / [`MemSegmentKind::InUse`], the
/// accent tint for [`MemSegmentKind::Inactive`], the disk/cache hue for
/// [`MemSegmentKind::Cache`], the dim/free hue for [`MemSegmentKind::Free`]
/// / [`MemSegmentKind::Available`], and the shade fill for
/// [`MemSegmentKind::Other`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemSegmentKind {
    /// Active / recently-used memory (five-segment meminfo path).
    Active,
    /// Inactive / early-reclaim candidate (five-segment path).
    Inactive,
    /// Page cache + buffers (five-segment path) or cached + buffers
    /// (three-segment path).
    Cache,
    /// ZFS adaptive replacement cache — a reclaimable component the kernel's
    /// `MemAvailable` does not count; rendered distinctly from the page
    /// cache on ZFS hosts (five-segment path).
    ZfsArc,
    /// Completely free physical memory (five-segment path).
    Free,
    /// Unaccounted-for reserved (`total − named`), five-segment path only.
    Other,
    /// In-use memory (three-/two-segment degraded path).
    InUse,
    /// Available memory (three-/two-segment degraded path).
    Available,
}

/// One colorless segment of the memory-composition bar: its semantic role,
/// byte count, and the resolved localized label.
#[derive(Clone, Copy, Debug)]
pub struct MemSegment {
    /// Semantic role → frontend theme color.
    pub kind: MemSegmentKind,
    /// Bytes in this segment (raw; the frontend divides by the total for the
    /// rendered width).
    pub bytes: u64,
    /// Resolved localized label (e.g. "Active", "Cache + Buffers").
    pub label: &'static str,
}

/// Build the memory-composition segments (two to six) from the typed
/// [`MemoryMetrics`] accessors, the same way every frontend does.
///
/// - **Five segments** (full): Active / Inactive / Cache+Buffers / Free /
///   Other-reserved, when the adapter exposes active, inactive, free,
///   reclaimable, and buffers. ZFS hosts gain a sixth, ZFS-ARC, between the
///   cache and free segments.
/// - **Three segments** (partial): In-use / Cached+Buffers / Available, when
///   cached and buffers are known but the active/inactive/free/reclaimable
///   detail is not.
/// - **Two segments** (minimal): In-use / Available, the always-available
///   fallback (Available carries the ARC-aware availability projection).
#[must_use]
pub fn memory_segments(memory: &MemoryMetrics) -> Vec<MemSegment> {
    if let (Some(active), Some(inactive), Some(free), Some(reclaimable)) = (
        memory.current_active_bytes(),
        memory.current_inactive_bytes(),
        memory.current_free_bytes(),
        memory.current_reclaimable_bytes(),
    ) {
        // Reclaimable memory remains a valid cache component even when the
        // provider did not expose a separate buffers counter. Missing
        // buffers are not a measured zero.
        let cache = memory
            .current_buffers_bytes()
            .map_or(reclaimable, |buffers| buffers.saturating_add(reclaimable));
        let Some(total) = memory.current_total_bytes() else {
            return Vec::new();
        };
        // The ZFS ARC is its own reclaimable segment when present (the
        // kernel's named page states never include it, so without this it
        // silently lands in "other / reserved"); absent hosts see no
        // segment rather than a zero.
        let zfs_arc = memory.current_zfs_arc_bytes();
        let named = active
            .saturating_add(inactive)
            .saturating_add(cache)
            .saturating_add(free)
            .saturating_add(zfs_arc.unwrap_or(0));
        let other = total.saturating_sub(named);
        let mut segments = vec![
            MemSegment {
                kind: MemSegmentKind::Active,
                bytes: active,
                label: t("mem.active"),
            },
            MemSegment {
                kind: MemSegmentKind::Inactive,
                bytes: inactive,
                label: t("mem.inactive"),
            },
            MemSegment {
                kind: MemSegmentKind::Cache,
                bytes: cache,
                label: t("mem.cache_buffers"),
            },
        ];
        if let Some(arc) = zfs_arc {
            segments.push(MemSegment {
                kind: MemSegmentKind::ZfsArc,
                bytes: arc,
                label: t("mem.zfs_arc"),
            });
        }
        segments.extend([
            MemSegment {
                kind: MemSegmentKind::Free,
                bytes: free,
                label: t("mem.free"),
            },
            MemSegment {
                kind: MemSegmentKind::Other,
                bytes: other,
                label: t("mem.other_reserved"),
            },
        ]);
        segments
    } else if let Some(cached) = memory.current_cached_bytes() {
        // Keep the known cached component when buffers are unavailable; do
        // not turn an absent optional fact into a believable zero.
        let cache = memory
            .current_buffers_bytes()
            .map_or(cached, |buffers| cached.saturating_add(buffers));
        let (Some(used), Some(total)) = (memory.current_used_bytes(), memory.current_total_bytes())
        else {
            return Vec::new();
        };
        let free = total.saturating_sub(used.saturating_add(cache));
        vec![
            MemSegment {
                kind: MemSegmentKind::InUse,
                bytes: used,
                label: t("mem.in_use"),
            },
            MemSegment {
                kind: MemSegmentKind::Cache,
                bytes: cache,
                label: t("mem.cached_buffers"),
            },
            MemSegment {
                kind: MemSegmentKind::Available,
                bytes: free,
                label: t("mem.available"),
            },
        ]
    } else {
        // Kernel `MemAvailable` excludes the ZFS ARC, so the minimal path
        // uses the core ARC-aware availability projection: on a ZFS host the
        // ARC no longer hides inside "in use". The kernel fact itself is
        // never redefined — see `MemoryMetrics::projected_available_bytes`.
        let (Some(used), Some(available)) = (
            memory.current_used_bytes(),
            memory.projected_available_bytes(),
        ) else {
            return Vec::new();
        };
        vec![
            MemSegment {
                kind: MemSegmentKind::InUse,
                bytes: used,
                label: t("mem.in_use"),
            },
            MemSegment {
                kind: MemSegmentKind::Available,
                bytes: available,
                label: t("mem.available"),
            },
        ]
    }
}

/// Swap breakdown for the secondary swap bar: used + total, plus optional
/// compressed-swap (zram) used bytes and a zswap-on flag.
#[derive(Clone, Copy, Debug)]
pub struct SwapBreakdown {
    /// Swap currently used (clamped to the total).
    pub used_bytes: u64,
    /// Total swap configured.
    pub total_bytes: u64,
    /// Compressed (zram) swap used, when a provider reports it.
    pub zram_bytes: Option<u64>,
    /// Whether compressed-swap caching (zswap) is enabled.
    pub zswap_on: bool,
    /// Uncompressed size of the data held in the zram store (`mm_stat`
    /// `orig_data_size`), for the compression readout.
    pub zram_original_bytes: Option<u64>,
    /// Size of that data after compression (`compr_data_size`).
    pub zram_compressed_bytes: Option<u64>,
    /// RAM the zram store consumes, metadata included (`mem_used_total`).
    pub zram_memory_used_bytes: Option<u64>,
    /// Original ÷ compressed from the core guarded pure rule.
    pub zram_compression_ratio: Option<f32>,
}

/// Resolve the swap breakdown for a memory snapshot, or `None` when no swap
/// is configured (`total` is zero or unavailable) so the caller can omit the
/// swap bar entirely.
#[must_use]
pub fn swap_breakdown(memory: &MemoryMetrics) -> Option<SwapBreakdown> {
    let total = memory.current_swap_total_bytes()?;
    if total == 0 {
        return None;
    }
    let used = memory.current_swap_used_bytes()?.min(total);
    Some(SwapBreakdown {
        used_bytes: used,
        total_bytes: total,
        zram_bytes: memory.current_compressed_swap_used_bytes(),
        zswap_on: memory.current_compressed_swap_cache_enabled() == Some(true),
        zram_original_bytes: memory.current_compressed_swap_original_bytes(),
        zram_compressed_bytes: memory.current_compressed_swap_compressed_bytes(),
        zram_memory_used_bytes: memory.current_compressed_swap_memory_used_bytes(),
        zram_compression_ratio: memory.current_compressed_swap_ratio(),
    })
}

#[cfg(test)]
#[path = "../tests/headless/shell_memory.rs"]
mod tests;

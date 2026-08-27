//! Provider-neutral boot timeline projection (BN-05).
//!
//! The Linux adapter already measures the systemd user critical chain
//! (`systemd-analyze --user critical-chain`) into
//! [`StartupCriticalChainNode`] entries. This module turns that raw chain
//! into a deterministic, bounded timeline every frontend can render as a
//! waterfall: a total measured span, per-unit windows, and honest handling of
//! nodes without activation/duration data (they are counted, never invented).
//!
//! It performs no I/O and has no platform dependency.

use serde::{Deserialize, Serialize};

use super::StartupCriticalChainNode;

/// One measured unit window inside the boot waterfall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootTimelineSegment {
    pub unit: String,
    /// Activation offset from boot, in milliseconds.
    pub start_ms: u64,
    /// Window end (`start + duration`), in milliseconds.
    pub end_ms: u64,
    pub duration_ms: u64,
}

/// Bounded, deterministic waterfall over a measured critical chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BootTimeline {
    /// Span of the longest measured window; `0` when nothing is timed.
    pub total_ms: u64,
    /// Measured segments sorted by start time (stable for equal starts).
    pub segments: Vec<BootTimelineSegment>,
    /// Nodes kept out of `segments` because the chain exceeded the segment
    /// cap (collapsed into a "+N more" row by the frontend).
    pub collapsed_count: usize,
    /// Nodes with no activation/duration data. They have no honest place on
    /// the time axis, so they are counted and listed, not placed.
    pub untimed_count: usize,
    /// The untimed unit names (bounded), so a frontend can show them without
    /// inventing a position.
    pub untimed_units: Vec<String>,
}

/// Per-unit comparison of the current boot's waterfall against a previous
/// boot's (roadmap #5). Fact-only: the signed difference of two measured
/// windows, never a causal claim about WHY a unit got slower or faster.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootSegmentDelta {
    pub unit: String,
    pub current_ms: u64,
    pub previous_ms: u64,
    /// `current - previous` as a signed value.
    pub delta_ms: i64,
}

/// Match the two timelines' timed segments by unit and report their deltas in
/// the CURRENT timeline's order. Units without a measured counterpart in
/// both boots are skipped: a first-appearance or vanished unit has no honest
/// comparison, not a fabricated ±.
pub fn segment_deltas(current: &BootTimeline, previous: &BootTimeline) -> Vec<BootSegmentDelta> {
    current
        .segments
        .iter()
        .filter_map(|segment| {
            let previous_segment = previous
                .segments
                .iter()
                .find(|candidate| candidate.unit == segment.unit)?;
            Some(BootSegmentDelta {
                unit: segment.unit.clone(),
                current_ms: segment.duration_ms,
                previous_ms: previous_segment.duration_ms,
                delta_ms: segment.duration_ms as i64 - previous_segment.duration_ms as i64,
            })
        })
        .collect()
}

/// Built-in waterfall row cap. Larger chains collapse deterministically.
pub const DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS: usize = 20;
/// Untimed unit names surfaced in [`BootTimeline::untimed_units`] (bounded
/// so the projection stays small even on a pathological chain).
pub const DEFAULT_BOOT_TIMELINE_MAX_UNTIMED: usize = 10;

impl BootTimeline {
    /// Project a critical chain into a bounded waterfall.
    ///
    /// - Nodes with both an activation offset and a duration become timed
    ///   segments (a missing duration with a valid offset is treated as a
    ///   zero-duration segment at that offset — the unit WAS activated, the
    ///   duration simply was not measured).
    /// - Nodes without an activation offset are untimed: counted and listed,
    ///   never placed on the axis.
    /// - When the timed set exceeds `max_segments`, only the earliest
    ///   `max_segments` are kept (stable order) and the rest are counted in
    ///   [`BootTimeline::collapsed_count`].
    #[must_use]
    pub fn from_critical_chain(
        chain: &[StartupCriticalChainNode],
        max_segments: usize,
        max_untimed: usize,
    ) -> Self {
        let mut timed = Vec::new();
        let mut untimed_count = 0_usize;
        let mut untimed_units = Vec::new();
        for node in chain {
            match node.activated_at_ms {
                Some(start_ms) => {
                    let duration_ms = node.duration_ms.unwrap_or(0);
                    let end_ms = start_ms.saturating_add(duration_ms);
                    timed.push(BootTimelineSegment {
                        unit: node.unit.clone(),
                        start_ms,
                        end_ms,
                        duration_ms,
                    });
                }
                None => {
                    untimed_count += 1;
                    if untimed_units.len() < max_untimed {
                        untimed_units.push(node.unit.clone());
                    }
                }
            }
        }
        timed.sort_by_key(|segment| (segment.start_ms, segment.unit.clone()));
        let total_ms = timed
            .iter()
            .map(|segment| segment.end_ms)
            .max()
            .unwrap_or(0);
        let collapsed_count = timed.len().saturating_sub(max_segments);
        timed.truncate(max_segments);
        Self {
            total_ms,
            segments: timed,
            collapsed_count,
            untimed_count,
            untimed_units,
        }
    }

    /// Normalized window fraction in `0.0..=1.0` for waterfall bar widths.
    /// A zero total yields `0.0` for every segment (nothing to scale by).
    #[must_use]
    pub fn fraction_of_total(&self, segment: &BootTimelineSegment) -> f32 {
        if self.total_ms == 0 {
            return 0.0;
        }
        let fraction = segment.duration_ms as f32 / self.total_ms as f32;
        fraction.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_startup_timeline_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/headless/core_core_startup_timeline_delta_tests.rs"]
mod delta_tests;

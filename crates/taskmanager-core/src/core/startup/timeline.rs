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

/// The boot phases a platform can report separately.
///
/// The order is the boot order: firmware hands over to the loader, the loader
/// hands over to the kernel, and the kernel hands over to user space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupPhaseKind {
    /// Pre-boot firmware time, from power-on to the boot loader.
    Firmware,
    /// Boot loader time.
    Loader,
    /// Kernel time, from the loader handoff to the init system.
    Kernel,
    /// User-space time, from the init system up to the user session.
    UserSpace,
}

/// One phase of a measured boot timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupPhase {
    /// Which boot phase this row is.
    pub kind: StartupPhaseKind,
    /// The measured duration in milliseconds.
    ///
    /// `None` = the platform did not measure this phase. It never means zero:
    /// a zero-width bar would claim the phase took no time at all.
    pub duration_ms: Option<u64>,
}

/// The measured phase split of one boot.
///
/// Every phase is optional. `None` means "this phase was not measured" — the
/// platform did not publish a duration for it — never zero and never a value
/// borrowed from a neighbouring phase. A frontend that draws phase bars must
/// render an explicit "not measured" row for the absent phases instead of a
/// zero-width bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StartupTimelineSnapshot {
    /// Pre-boot firmware time in milliseconds.
    pub firmware_ms: Option<u64>,
    /// Boot loader time in milliseconds.
    pub loader_ms: Option<u64>,
    /// Kernel time in milliseconds.
    pub kernel_ms: Option<u64>,
    /// User-space time in milliseconds.
    pub user_space_ms: Option<u64>,
}

impl StartupTimelineSnapshot {
    /// The phases in boot order with their measured duration.
    ///
    /// The order is fixed so every frontend walks the phases the same way.
    #[must_use]
    pub fn phases(&self) -> [StartupPhase; 4] {
        [
            StartupPhase {
                kind: StartupPhaseKind::Firmware,
                duration_ms: self.firmware_ms,
            },
            StartupPhase {
                kind: StartupPhaseKind::Loader,
                duration_ms: self.loader_ms,
            },
            StartupPhase {
                kind: StartupPhaseKind::Kernel,
                duration_ms: self.kernel_ms,
            },
            StartupPhase {
                kind: StartupPhaseKind::UserSpace,
                duration_ms: self.user_space_ms,
            },
        ]
    }

    /// The sum of the MEASURED phases only.
    ///
    /// An unmeasured phase contributes nothing: the result is the span the
    /// platform actually measured, never a span padded with invented zeros.
    #[must_use]
    pub fn measured_total_ms(&self) -> u64 {
        self.phases().iter().fold(0_u64, |total, phase| {
            total.saturating_add(phase.duration_ms.unwrap_or(0))
        })
    }

    /// Whether at least one phase carries a measured duration.
    #[must_use]
    pub fn has_measured_phase(&self) -> bool {
        self.phases()
            .iter()
            .any(|phase| phase.duration_ms.is_some())
    }
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

#[cfg(test)]
#[path = "../../../tests/headless/core_core_startup_phase_timeline_tests.rs"]
mod phase_tests;

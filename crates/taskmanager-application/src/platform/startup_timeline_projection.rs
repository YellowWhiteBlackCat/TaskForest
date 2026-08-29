//! Shared boot-timeline projection for all frontends (BN-05).
//!
//! The single rows decision every waterfall renderer needs lives here, beside
//! the startup-evidence projection it consumes. It performs no I/O and
//! depends on the core projection.

use taskmanager_core::StartupBootEvidenceSnapshot;
use taskmanager_core::core::startup::{
    BootTimeline, DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS, DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
};

/// Project the boot-timeline waterfall rows for a frontend, or `None` when
/// the block must stay silent.
///
/// - A typed critical-chain failure suppresses the whole waterfall (never
///   render stale bars over a failure).
/// - An empty-but-typed snapshot (no measured segments and no untimed nodes)
///   also stays silent: there is nothing honest to place on the time axis.
/// - Otherwise the bounded [`BootTimeline`] is returned: measured segments
///   sorted by activation, untimed nodes counted and listed (never placed),
///   and any overflow collapsed into `collapsed_count`.
#[must_use]
pub fn boot_timeline_rows(evidence: &StartupBootEvidenceSnapshot) -> Option<BootTimeline> {
    if evidence.critical_chain_failure.is_some() {
        return None;
    }
    let timeline = BootTimeline::from_critical_chain(
        &evidence.critical_chain,
        DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS,
        DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    );
    if timeline.segments.is_empty() && timeline.untimed_count == 0 {
        return None;
    }
    Some(timeline)
}

#[cfg(test)]
#[path = "../../tests/headless/platform/startup_timeline_projection.rs"]
mod tests;

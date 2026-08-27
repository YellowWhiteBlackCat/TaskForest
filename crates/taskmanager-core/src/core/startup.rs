//! Platform-neutral startup contracts grouped by change domain.

mod evidence;
mod identity;
mod inventory;
mod timeline;

pub use evidence::{
    StartupBootEvidenceSnapshot, StartupCriticalChainNode, StartupEvidenceFailure,
    StartupFailedUnit, StartupImpactEvidence, StartupImpactUnknownReason,
};
pub use identity::{StartupEntryId, StartupEntryLocator};
pub use inventory::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupScope, StartupSource,
};
pub use timeline::{
    BootSegmentDelta, BootTimeline, BootTimelineSegment, DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS,
    DEFAULT_BOOT_TIMELINE_MAX_UNTIMED, segment_deltas,
};

//! test-intent: behavior
//!
//! The per-phase boot split is an independent evidence source, so the
//! snapshot has to let a frontend tell three states apart: the analyzer
//! answered and measured a split, the analyzer answered and measured nothing,
//! and the analyzer could not be read at all. The last two both leave the
//! split absent — only the typed failure separates them, so that absence is
//! never read as "the boot took no time".

use super::*;
use crate::core::device_state::DeviceStatus;

#[test]
fn a_source_that_measured_nothing_publishes_no_split_and_no_failure() {
    let snapshot = StartupBootEvidenceSnapshot::default();

    assert!(
        snapshot.phase_timeline.is_none(),
        "an unmeasured phase split must not be published as an empty one"
    );
    assert!(
        snapshot.phase_timeline_failure.is_none(),
        "a source that was read carries no failure"
    );
}

#[test]
fn a_measured_split_is_published_with_only_the_phases_that_were_measured() {
    let snapshot = StartupBootEvidenceSnapshot {
        phase_timeline: Some(StartupTimelineSnapshot {
            firmware_ms: Some(1_200),
            kernel_ms: Some(5_600),
            ..StartupTimelineSnapshot::default()
        }),
        ..StartupBootEvidenceSnapshot::default()
    };

    let split = snapshot.phase_timeline.expect("a measured split");
    assert!(split.has_measured_phase());
    assert_eq!(split.firmware_ms, Some(1_200));
    assert_eq!(
        split.loader_ms, None,
        "a phase the analyzer did not measure stays absent"
    );
    assert_eq!(split.kernel_ms, Some(5_600));
}

#[test]
fn an_unreadable_source_publishes_no_split_and_keeps_its_typed_failure() {
    let snapshot = StartupBootEvidenceSnapshot {
        phase_timeline_state: DeviceState::default().transition(DeviceStatus::PermissionDenied, 1),
        phase_timeline_failure: Some(StartupEvidenceFailure::PermissionDenied),
        ..StartupBootEvidenceSnapshot::default()
    };

    assert!(
        snapshot.phase_timeline.is_none(),
        "an unavailable source must not publish a split"
    );
    assert_eq!(
        snapshot.phase_timeline_failure,
        Some(StartupEvidenceFailure::PermissionDenied)
    );
    assert_eq!(
        snapshot.phase_timeline_state.status,
        DeviceStatus::PermissionDenied
    );
}

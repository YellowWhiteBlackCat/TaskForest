//! Regression tests for the shared telemetry-frame lifecycle and tick commit.

use super::{FrameCommit, SystemProjectionStore, TelemetryFrameState};
use taskmanager_core::core::metrics::SystemSnapshot;

#[test]
fn telemetry_frame_lifecycle_is_collecting_until_a_snapshot_exists() {
    let mut projection = SystemProjectionStore::default();

    assert_eq!(
        projection.telemetry_frame_state(),
        TelemetryFrameState::Collecting
    );
    assert!(projection.telemetry_frame_state().is_collecting());

    projection.snapshot = Some(SystemSnapshot::default());

    assert_eq!(
        projection.telemetry_frame_state(),
        TelemetryFrameState::Ready
    );
    assert!(projection.telemetry_frame_state().is_ready());
}

#[test]
fn frame_commit_merges_partial_ticks_without_losing_a_commit() {
    assert_eq!(
        FrameCommit::Unchanged.merge(FrameCommit::Unchanged),
        FrameCommit::Unchanged
    );
    assert_eq!(
        FrameCommit::Unchanged.merge(FrameCommit::Committed),
        FrameCommit::Committed
    );
    assert_eq!(
        FrameCommit::Committed.merge(FrameCommit::Unchanged),
        FrameCommit::Committed
    );
}

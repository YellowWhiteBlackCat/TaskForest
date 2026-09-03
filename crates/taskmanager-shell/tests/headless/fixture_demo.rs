//! test-intent: behavior
//!
//! The shared demo builders must produce the same typed projection and a
//! useful bounded history window for every frontend that consumes them.

use super::*;
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus};

#[test]
fn direct_demo_contains_the_shared_product_projection() {
    let track = demo_direct_track();
    let projection = track.projection();

    assert!(projection.snapshot.is_some());
    assert!(projection.hardware.is_some());
    assert!(
        projection
            .processes
            .as_ref()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert!(
        projection
            .services
            .as_ref()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert!(
        projection
            .startup_entries
            .as_ref()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert!(
        projection
            .sessions
            .as_ref()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert!(track.visible_processes().len() >= 2);
    assert_eq!(
        projection.capability_status(&CapabilityId::TELEMETRY_GPU_ENGINES),
        Some(CapabilityStatus::PermissionRequired)
    );
}

#[test]
fn demo_telemetry_contains_measured_chart_windows() {
    let (store, _ingestor) = demo_telemetry();

    assert!(store.system_history.cpu_usage().samples().len() >= 2);
    assert!(store.system_history.memory_usage().samples().len() >= 2);
    assert!(store.system_history.storage_rate_total().samples().len() >= 2);
    assert!(store.system_history.network_rate_total().samples().len() >= 2);

    let disk = taskmanager_core::core::identity::DeviceId::new("disk:demo:nvme0");
    assert!(
        store
            .system_history
            .storage_rate(&disk)
            .is_some_and(|history| history.samples().len() >= 2)
    );
}

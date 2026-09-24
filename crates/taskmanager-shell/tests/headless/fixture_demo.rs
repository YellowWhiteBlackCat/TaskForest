//! test-intent: behavior
//!
//! The shared demo builders must produce the same typed projection and a
//! useful bounded history window for every frontend that consumes them.

use super::*;
use taskmanager_application::i18n;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus};

use crate::presentation::{msr_thermal_status_segments, msr_thermal_status_summary};

/// The demo/capture bootstrap seeds the real-time thermal-status readout
/// through the real request-session path, so the new row is visible in every
/// frontend's evidence frame. The seed is explicitly synthetic (demo mode
/// never invokes the privileged lane) and carries BOTH states in one frame:
/// CPU 0 asserted and CPU 1 unreadable (the honest dash). Both shell tracks
/// must fold to the identical summary and the identical per-node segments.
#[test]
fn demo_seeds_the_synthetic_thermal_status_readout() {
    i18n::set_language(i18n::Language::En);
    let expected = Some("CPU 0 Asserted | CPU 1 —");
    let expected_segments = vec!["CPU 0 Asserted", "CPU 1 —"];
    assert_eq!(
        msr_thermal_status_summary(demo_app().msr_readout_state()).as_deref(),
        expected,
        "the composed demo track must seed the asserted + unreadable readout"
    );
    assert_eq!(
        msr_thermal_status_segments(demo_app().msr_readout_state()),
        expected_segments,
        "the composed demo track must expose one paintable segment per node"
    );
    assert_eq!(
        msr_thermal_status_summary(demo_direct_track().msr_readout_state()).as_deref(),
        expected,
        "the direct demo track must fold the identical synthetic readout"
    );
    assert_eq!(
        msr_thermal_status_segments(demo_direct_track().msr_readout_state()),
        expected_segments,
        "the direct demo track must expose the identical per-node segments"
    );
}

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
        Some(CapabilityStatus::RequiresEscalation)
    );
}

#[test]
fn demo_telemetry_contains_measured_chart_windows() {
    let (store, _ingestor) = demo_telemetry();

    assert!(store.system_history.cpu_usage().samples().len() >= 2);
    assert!(store.system_history.memory_usage().samples().len() >= 2);
    assert!(store.system_history.storage_rate_total().samples().len() >= 2);
    assert!(store.system_history.network_rate_total().samples().len() >= 2);

    let disk = DeviceId::new("disk:demo:nvme0");
    assert!(
        store
            .system_history
            .storage_rate(&disk)
            .is_some_and(|history| history.samples().len() >= 2)
    );
}

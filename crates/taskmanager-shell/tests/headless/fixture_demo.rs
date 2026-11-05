//! test-intent: behavior
//!
//! The shared demo builders must produce the same typed projection and a
//! useful bounded history window for every frontend that consumes them.

use super::*;
use taskforest_contract::{CapabilityId, CapabilityStatus};
use taskforest_core::core::device_state::DeviceStatus;
use taskforest_core::core::identity::DeviceId;
use taskmanager_application::i18n;

use crate::presentation::{msr_thermal_status_segments, msr_thermal_status_summary};
use taskmanager_telemetry_store::live_graph::{LiveGraphHistory, MetricSeries};

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
fn demo_notice_uses_the_shared_locale_in_both_shell_tracks() {
    for (language, expected) in [
        (i18n::Language::En, "Demo snapshot · no host actions"),
        (i18n::Language::Zh, "演示快照 · 无主机操作"),
    ] {
        i18n::set_language(language);
        let composed = demo_app();
        assert_eq!(
            composed.feedback_notice().map(|notice| notice.text()),
            Some(expected),
            "composed demo feedback must use the active shared locale"
        );

        let direct = demo_direct_track();
        assert_eq!(
            direct.feedback_notice().map(|notice| notice.text()),
            Some(expected),
            "direct-track demo feedback must use the same active locale"
        );
    }
    i18n::set_language(i18n::Language::En);
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

    let gpu = projection
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.gpu.first())
        .expect("demo GPU snapshot");
    assert_eq!(
        gpu.device_state.status,
        DeviceStatus::Healthy,
        "available demo GPU metrics carry a healthy device-source state"
    );
    assert!(
        gpu.current_utilization_pct().is_some(),
        "the demo GPU status matches its measured utilization"
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

/// GPUI's standalone demo store and the composed track's shell history must
/// expose the same accepted waveform. Different frontends may use different
/// read adapters, but a different sample sequence is a second demo data
/// regime and would make sparkline/headline capture comparisons meaningless.
#[test]
fn both_demo_tracks_read_the_same_history_sequence() {
    let composed = demo_app();
    let (store, _ingestor) = demo_telemetry();
    for series in [
        MetricSeries::CpuUsagePercent,
        MetricSeries::MemoryUsagePercent,
        MetricSeries::DiskBytesPerSec,
        MetricSeries::NetworkBytesPerSec,
        MetricSeries::GpuUsagePercent,
    ] {
        let shell_window = composed.history.series(series);
        let standalone = LiveGraphHistory::from_store(std::sync::Arc::clone(&store), 600);
        let store_window = standalone.series(series);
        assert_eq!(
            shell_window.len(),
            25,
            "{series:?}: the shared demo history owns one seed frame and 24 adjacent frames"
        );
        assert_eq!(
            shell_window, store_window,
            "{series:?}: GPUI and Bevy must read identical accepted demo samples"
        );
    }
    let cpu = composed
        .history
        .series(MetricSeries::CpuUsagePercent)
        .into_iter()
        .map(|value| value.round() as i32)
        .collect::<Vec<_>>();
    assert_ne!(
        cpu.first().copied(),
        cpu.get(1).copied(),
        "the shared sequence is a real waveform, not repeated flat frames"
    );
    assert_eq!(
        cpu.last().copied(),
        Some(37),
        "the waveform tail returns to the current CPU fact"
    );
}

use super::{
    CpuFactsMarkerData, PerfDevice, append_device_marker, append_marker, append_process_marker,
    cpu_facts_marker_line, device_name, marker_line, page_name, performance_viewport_marker_line,
    process_context_menu_marker_line, target_marker_line,
};
use crate::ui::responsive::{
    PerformanceChartInventory, PerformancePageBudget, PerformanceVerticalRunway,
};
use iced::Size;
use taskmanager_application::AppPage;
use taskmanager_core::core::hardware::CoreBreakdown;
use taskmanager_core::core::process::ProcessLiveKey;

#[must_use]
pub(crate) fn device_marker_line(device: PerfDevice) -> String {
    target_marker_line("performance", device_name(device))
}

#[test]
fn marker_line_has_typed_capture_identity() {
    assert_eq!(
        marker_line("frame_ready", "demo", "performance"),
        "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=performance\n"
    );

    // The format is only half of the readiness contract: the external runner
    // reads the file the production app appends. Drive the real append path —
    // the page marker and the typed device target marker — and read back what
    // the capture script sees.
    let dir = crate::test_support::temp_dir("capture-marker");
    let path = dir.join("markers.txt");
    append_marker(
        &path,
        "frame_ready",
        "demo",
        page_name(AppPage::Applications),
    );
    append_device_marker(&path, PerfDevice::Gpu(0));
    let text = std::fs::read_to_string(&path).expect("the appended marker file");
    assert!(
        text.contains("ICED_CAPTURE_MARKER event=frame_ready mode=demo page=applications\n"),
        "the appended page marker must carry the live page: {text:?}"
    );
    assert!(
        text.contains(
            "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=gpu\n"
        ),
        "the appended target marker must carry the typed device token: {text:?}"
    );

    // A failed marker write is deliberately ignored: an unwritable evidence
    // path degrades to a missing marker the validator rejects, never to an
    // application failure.
    let unwritable = dir.join("missing-parent").join("markers.txt");
    append_marker(&unwritable, "frame_ready", "demo", "performance");
    append_device_marker(&unwritable, PerfDevice::Cpu);
    assert!(
        !unwritable.exists(),
        "an unwritable marker path must not fabricate evidence"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn process_marker_binds_capture_to_the_emitting_application() {
    let dir = crate::test_support::temp_dir("capture-process-marker");
    let path = dir.join("markers.txt");
    append_process_marker(&path, 47_211);
    let text = std::fs::read_to_string(&path).expect("the appended process marker");
    assert_eq!(
        text, "ICED_CAPTURE_MARKER event=process_ready pid=47211\n",
        "the runner must target the PID emitted by the actual Iced process"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn page_names_follow_the_shared_page_set() {
    let names: Vec<_> = AppPage::ALL.into_iter().map(page_name).collect();
    assert_eq!(
        names,
        [
            "performance",
            "applications",
            "services",
            "system",
            "startup",
            "users",
            "app-history"
        ]
    );
}

#[test]
fn device_markers_cover_every_performance_selector() {
    let devices = crate::app::PerfDevice::ALL;
    let names: Vec<_> = devices.into_iter().map(device_name).collect();
    assert_eq!(
        names,
        [
            "cpu", "memory", "disk", "network", "gpu", "npu", "battery", "fan"
        ]
    );
    assert_eq!(
        device_marker_line(crate::app::PerfDevice::Gpu(0)),
        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=gpu\n"
    );
    assert_eq!(
        target_marker_line("applications", "applications"),
        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=applications device=applications\n"
    );
}

#[test]
fn performance_viewport_marker_records_the_received_size_and_typed_budget() {
    let viewport = Size::new(1180.0, 780.0);
    let budget = PerformancePageBudget::for_perf_frame(viewport, true);
    assert_eq!(budget.vertical, PerformanceVerticalRunway::Charts);
    assert_eq!(budget.chart_inventory, PerformanceChartInventory::Full);
    assert_eq!(
        performance_viewport_marker_line("cpu", viewport, viewport, budget),
        "ICED_CAPTURE_MARKER event=viewport_ready mode=demo page=performance device=cpu width=1180.00 height=780.00 page_width=1180.00 page_height=780.00 rung=Charts inventory=Full\n"
    );
}

#[test]
fn process_context_menu_surface_marker_carries_the_frozen_live_identity() {
    assert_eq!(
        process_context_menu_marker_line(
            ProcessLiveKey::new(4321, 7).expect("fixture process identity is valid"),
        ),
        "ICED_CAPTURE_MARKER event=surface_ready mode=demo page=applications surface=process-context-menu pid=4321 start_token=7\n"
    );
}

#[test]
fn cpu_facts_marker_records_projected_hybrid_counts_and_cache_capacities() {
    let facts = CpuFactsMarkerData {
        core_breakdown: CoreBreakdown {
            p_cores: 6,
            e_cores: 8,
            lp_cores: 2,
        },
        caches_kib: [Some(64), Some(64), Some(1_024), Some(16_384)],
        base_frequency_mhz: Some(2_400),
        physical_cores: Some(16),
        logical_cores: Some(22),
        virtualization: Some("VT-x"),
        temperature_average_max_c: Some((55, 59)),
    };
    assert_eq!(
        cpu_facts_marker_line(facts),
        Some("ICED_CAPTURE_MARKER event=cpu_facts_ready mode=demo page=performance device=cpu-facts rail=stats-end core_breakdown=6/8/2 cache_kib=64/64/1024/16384 base_frequency_mhz=2400 core_counts=16/22 virtualization=VT-x core_temperature_c=55/59\n".to_string())
    );
    assert_eq!(
        cpu_facts_marker_line(CpuFactsMarkerData {
            core_breakdown: CoreBreakdown::default(),
            caches_kib: [Some(64), None, Some(1), Some(1)],
            ..facts
        }),
        None,
        "missing typed facts cannot emit a complete facts marker"
    );
}

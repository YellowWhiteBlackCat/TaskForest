use super::{PerfDevice, device_name, marker_line, page_name, target_marker_line};

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
}

#[test]
fn page_names_follow_the_shared_page_set() {
    let names: Vec<_> = taskmanager_application::AppPage::ALL
        .into_iter()
        .map(page_name)
        .collect();
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
        ["cpu", "memory", "disk", "network", "gpu", "battery", "fan"]
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

//! Environment-gated readiness markers for real Iced pixel capture.
//!
//! The marker path is intentionally outside the renderer-neutral shell. It is
//! only enabled by `scripts/capture-iced.sh`; normal launches do not perform
//! filesystem I/O for capture evidence.

use std::path::Path;

use taskmanager_application::AppPage;

use crate::app::PerfDevice;

/// Build one stable marker line for the independent Iced evidence runner.
#[must_use]
pub(crate) fn marker_line(event: &str, mode: &str, page: &str) -> String {
    format!("ICED_CAPTURE_MARKER event={event} mode={mode} page={page}\n")
}

/// Stable capture vocabulary for the Performance selector. This is separate
/// from [`page_name`] because the Iced runner captures one selected resource
/// within the Performance page.
#[must_use]
pub(crate) fn device_name(device: PerfDevice) -> &'static str {
    match device {
        PerfDevice::Cpu => "cpu",
        PerfDevice::Memory => "memory",
        PerfDevice::Disk(_) => "disk",
        PerfDevice::Network(_) => "network",
        PerfDevice::Gpu(_) => "gpu",
        PerfDevice::Battery(_) => "battery",
        PerfDevice::Fan(_) => "fan",
    }
}

/// Build the target marker for a canonical capture page/resource. Performance
/// uses the resource vocabulary; other pages use their stable page name as the
/// target token so the runner can prove it captured the requested surface.
#[must_use]
pub(crate) fn target_marker_line(page: &str, target: &str) -> String {
    format!("ICED_CAPTURE_MARKER event=target_ready mode=demo page={page} device={target}\n")
}

/// Return the canonical capture spelling for one shared page.
#[must_use]
pub(crate) fn page_name(page: AppPage) -> &'static str {
    match page {
        AppPage::Performance => "performance",
        AppPage::Applications => "applications",
        AppPage::Services => "services",
        AppPage::System => "system",
        AppPage::Startup => "startup",
        AppPage::Users => "users",
        AppPage::AppHistory => "app-history",
    }
}

/// Append a marker when capture mode is enabled. A failed marker write is
/// deliberately ignored; the external validator must reject a run without
/// the marker rather than letting the application fail because of evidence
/// plumbing.
pub(crate) fn append_marker(path: &Path, event: &str, mode: &str, page: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = file.write_all(marker_line(event, mode, page).as_bytes());
}

/// Append the selected Performance resource marker after the first frame is
/// presented. The validator requires this line so a valid Iced screenshot cannot
/// silently be a different device page than the requested scenario.
pub(crate) fn append_device_marker(path: &Path, device: PerfDevice) {
    append_target_marker(path, "performance", device_name(device));
}

/// Append a page/resource target marker after the first frame is presented.
pub(crate) fn append_target_marker(path: &Path, page: &str, target: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = file.write_all(target_marker_line(page, target).as_bytes());
}

#[cfg(test)]
#[path = "../tests/gui/capture_tests.rs"]
mod tests;

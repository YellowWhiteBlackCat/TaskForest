//! Environment-gated readiness markers for real Iced pixel capture.
//!
//! The marker path is intentionally outside the renderer-neutral shell. It is
//! only enabled by `scripts/capture-iced.sh`; normal launches do not perform
//! filesystem I/O for capture evidence.

use std::path::Path;

use iced::Size;
use taskforest_core::core::hardware::CoreBreakdown;
use taskforest_core::core::process::ProcessLiveKey;
use taskmanager_application::AppPage;

use crate::app::PerfDevice;
use crate::ui::responsive::PerformancePageBudget;

pub(crate) fn persisted_history_requested() -> bool {
    std::env::var("TM_ICED_CAPTURE_DEVICE").is_ok_and(|target| {
        matches!(
            target.as_str(),
            "history-replay" | "application-history-replay"
        )
    })
}

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
        PerfDevice::Npu(_) => "npu",
        PerfDevice::Battery(_) => "battery",
        PerfDevice::Fan(_) => "fan",
    }
}

/// Capture target token for the health modal. It is one renderer-local surface
/// with no page of its own, so its marker keeps the underlying Performance
/// page and names the surface as the target (the `service-details` shape).
pub(crate) const HEALTH_TARGET: &str = "health";

/// Build the target marker for a canonical capture page/resource. Performance
/// uses the resource vocabulary; other pages use their stable page name as the
/// target token so the runner can prove it captured the requested surface.
#[must_use]
pub(crate) fn target_marker_line(page: &str, target: &str) -> String {
    format!("ICED_CAPTURE_MARKER event=target_ready mode=demo page={page} device={target}\n")
}

/// Build a readiness line for the real process row identity mounted in the
/// Applications Popover surface.
#[must_use]
pub(crate) fn process_context_menu_marker_line(identity: ProcessLiveKey) -> String {
    format!(
        "ICED_CAPTURE_MARKER event=surface_ready mode=demo page=applications surface=process-context-menu pid={} start_token={}\n",
        identity.pid(),
        identity.start_token(),
    )
}

/// Build a readiness line from the logical client viewport received by the
/// app, including the typed Performance allocation derived from that frame.
#[must_use]
pub(crate) fn performance_viewport_marker_line(
    target: &str,
    viewport: Size,
    page_viewport: Size,
    budget: PerformancePageBudget,
) -> String {
    format!(
        "ICED_CAPTURE_MARKER event=viewport_ready mode=demo page=performance device={target} width={:.2} height={:.2} page_width={:.2} page_height={:.2} rung={:?} inventory={:?}\n",
        viewport.width,
        viewport.height,
        page_viewport.width,
        page_viewport.height,
        budget.vertical,
        budget.chart_inventory,
    )
}

/// Typed CPU facts captured by the dedicated CPU-facts scenario.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CpuFactsMarkerData<'a> {
    pub(crate) core_breakdown: CoreBreakdown,
    pub(crate) caches_kib: [Option<u64>; 4],
    pub(crate) base_frequency_mhz: Option<u64>,
    pub(crate) physical_cores: Option<usize>,
    pub(crate) logical_cores: Option<usize>,
    pub(crate) virtualization: Option<&'a str>,
    pub(crate) temperature_average_max_c: Option<(i32, i32)>,
}

/// Build readiness evidence from the typed CPU facts projected in the rail.
#[must_use]
pub(crate) fn cpu_facts_marker_line(data: CpuFactsMarkerData<'_>) -> Option<String> {
    let [Some(l1d), Some(l1i), Some(l2), Some(l3)] = data.caches_kib else {
        return None;
    };
    let base_frequency_mhz = data.base_frequency_mhz?;
    let physical_cores = data.physical_cores?;
    let logical_cores = data.logical_cores?;
    let virtualization = data.virtualization?;
    let (temperature_average_c, temperature_maximum_c) = data.temperature_average_max_c?;
    Some(format!(
        "ICED_CAPTURE_MARKER event=cpu_facts_ready mode=demo page=performance device=cpu-facts rail=stats-end core_breakdown={}/{}/{} cache_kib={l1d}/{l1i}/{l2}/{l3} base_frequency_mhz={base_frequency_mhz} core_counts={physical_cores}/{logical_cores} virtualization={virtualization} core_temperature_c={temperature_average_c}/{temperature_maximum_c}\n",
        data.core_breakdown.p_cores, data.core_breakdown.e_cores, data.core_breakdown.lp_cores,
    ))
}

/// Append typed CPU facts after the capture readiness gate has accepted them.
pub(crate) fn append_cpu_facts_marker(path: &Path, data: CpuFactsMarkerData<'_>) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let Some(line) = cpu_facts_marker_line(data) else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = file.write_all(line.as_bytes());
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

/// Append the current demo process identity before its first frame. The
/// Windows capture helper requires this PID to bind Windows.Graphics.Capture
/// to the exact product process that emitted the readiness markers.
pub(crate) fn append_process_marker(path: &Path, process_id: u32) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(
        file,
        "ICED_CAPTURE_MARKER event=process_ready pid={process_id}"
    );
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

/// Append a marker for a production overlay that must be mounted before capture.
pub(crate) fn append_process_context_menu_marker(path: &Path, identity: ProcessLiveKey) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = file.write_all(process_context_menu_marker_line(identity).as_bytes());
}

/// Append the actual received client viewport and its Performance budget.
pub(crate) fn append_performance_viewport_marker(
    path: &Path,
    target: &str,
    viewport: Size,
    page_viewport: Size,
    budget: PerformancePageBudget,
) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = file.write_all(
        performance_viewport_marker_line(target, viewport, page_viewport, budget).as_bytes(),
    );
}

#[cfg(test)]
#[path = "../tests/gui/capture_tests.rs"]
mod tests;

//! Headless Wayland visual capture routing and targets for the Bevy frontend.

use crate::app::Page;
use crate::pages::performance::PerformanceDeviceTarget;
use bevy::window::WindowResolution;

pub(crate) fn capture_page_name(page: Page) -> &'static str {
    if let Some(target) = capture_scenario_target() {
        return target;
    }
    match page {
        Page::Processes => "applications",
        Page::Performance => "performance",
        Page::Services => "services",
        Page::System => "system",
        Page::Startup => "startup",
        Page::Sessions => "users",
        Page::Alerts => "alerts",
        Page::Settings => "settings",
        Page::AppHistory => "app-history",
        Page::Containers => "containers",
    }
}

pub(crate) fn capture_scenario_target() -> Option<&'static str> {
    let raw = std::env::var("TM_BEVY_CAPTURE_PAGE").ok()?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "service-logs" => Some("service-logs"),
        "perf-memory" => Some("perf-memory"),
        "perf-disk" => Some("perf-disk"),
        "perf-network" => Some("perf-network"),
        "perf-gpu" => Some("perf-gpu"),
        "perf-battery" => Some("perf-battery"),
        "process-force-kill" => Some("process-force-kill"),
        "process-tree-confirm" => Some("process-tree-confirm"),
        "process-batch-confirm" => Some("process-batch-confirm"),
        "smart-self-test-confirm" => Some("smart-self-test-confirm"),
        "process-properties-performance" => Some("process-properties-performance"),
        "process-memory-pss-swap" => Some("process-memory-pss-swap"),
        "process-network-details" => Some("process-network-details"),
        "process-gpu-details" => Some("process-gpu-details"),
        "process-resource-limits" => Some("process-resource-limits"),
        "process-isolation" => Some("process-isolation"),
        "startup-impact" => Some("startup-impact"),
        "startup-failure-evidence" => Some("startup-failure-evidence"),
        "startup-boot-markers" => Some("startup-boot-markers"),
        "services-search-highlight" => Some("services-search-highlight"),
        "apps-search-highlight" => Some("apps-search-highlight"),
        "apps-group-expanded" => Some("apps-group-expanded"),
        "telemetry-paused" => Some("telemetry-paused"),
        "sidebar-hidden" => Some("sidebar-hidden"),
        "about" => Some("about"),
        "system-about" => Some("system-about"),
        "system-dashboard" => Some("system-dashboard"),
        "system-hardware" => Some("system-hardware"),
        "system-npu" => Some("system-npu"),
        "sensor-center" => Some("sensor-center"),
        "storage-health" => Some("storage-health"),
        "active-alert" => Some("active-alert"),
        "alert-rules-manager" => Some("alert-rules-manager"),
        "smart-missing-tool" => Some("smart-missing-tool"),
        "smart-permission" => Some("smart-permission"),
        "partition-disk-usage" => Some("partition-disk-usage"),
        "partition-live-usage" => Some("partition-live-usage"),
        "gpu-engine-inventory" => Some("gpu-engine-inventory"),
        "intel-gpu-telemetry" => Some("intel-gpu-telemetry"),
        "history-replay" => Some("history-replay"),
        "history-60m" => Some("history-60m"),
        "apps-identity-matrix" => Some("apps-identity-matrix"),
        "event-center" => Some("event-center"),
        "settings-permission-center" => Some("settings-permission-center"),
        _ => None,
    }
}

pub(crate) fn capture_perf_device_target() -> Option<PerformanceDeviceTarget> {
    let raw = std::env::var("TM_BEVY_CAPTURE_PAGE").ok()?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "perf-memory" => Some(PerformanceDeviceTarget::Memory),
        "perf-disk"
        | "storage-health"
        | "smart-missing-tool"
        | "smart-permission"
        | "partition-disk-usage"
        | "partition-live-usage" => Some(PerformanceDeviceTarget::Disk("disk:demo:nvme0".into())),
        "perf-network" => Some(PerformanceDeviceTarget::Network(
            "network:demo:wlan0".into(),
        )),
        "perf-gpu" | "gpu-engine-inventory" | "intel-gpu-telemetry" => {
            Some(PerformanceDeviceTarget::Gpu("gpu:demo:0".into()))
        }
        "perf-battery" | "sensor-center" => {
            Some(PerformanceDeviceTarget::Battery("battery:demo:0".into()))
        }
        _ => None,
    }
}

pub(crate) fn capture_wants_service_logs() -> bool {
    std::env::var("TM_BEVY_CAPTURE_PAGE").is_ok_and(|value| value.trim() == "service-logs")
}

pub(crate) fn capture_page() -> Option<Page> {
    let value = std::env::var("TM_BEVY_CAPTURE_PAGE").ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "applications"
        | "processes"
        | "process-force-kill"
        | "process-tree-confirm"
        | "process-batch-confirm"
        | "process-properties-performance"
        | "process-memory-pss-swap"
        | "process-network-details"
        | "process-gpu-details"
        | "process-resource-limits"
        | "process-isolation"
        | "apps-search-highlight"
        | "apps-group-expanded"
        | "apps-identity-matrix" => Some(Page::Processes),
        "performance"
        | "perf-memory"
        | "perf-disk"
        | "perf-network"
        | "perf-gpu"
        | "perf-battery"
        | "smart-self-test-confirm"
        | "telemetry-paused"
        | "sidebar-hidden"
        | "sensor-center"
        | "storage-health"
        | "smart-missing-tool"
        | "smart-permission"
        | "partition-disk-usage"
        | "partition-live-usage"
        | "gpu-engine-inventory"
        | "intel-gpu-telemetry"
        | "history-replay"
        | "history-60m" => Some(Page::Performance),
        "services" | "service-logs" | "services-search-highlight" => Some(Page::Services),
        "startup" | "startup-impact" | "startup-failure-evidence" | "startup-boot-markers" => {
            Some(Page::Startup)
        }
        "system" | "system-dashboard" | "system-hardware" | "system-npu" | "about"
        | "system-about" => Some(Page::System),
        "alerts" | "active-alert" | "alert-rules-manager" | "event-center" => Some(Page::Alerts),
        "users" | "sessions" => Some(Page::Sessions),
        "settings" | "settings-permission-center" => Some(Page::Settings),
        "app-history" | "history" => Some(Page::AppHistory),
        "containers" => Some(Page::Containers),
        _ => None,
    }
}

pub(crate) fn capture_window_resolution() -> WindowResolution {
    let raw = std::env::var("TM_BEVY_WINDOW_SIZE").unwrap_or_default();
    let (width, height) = raw
        .split_once('x')
        .and_then(|(width, height)| Some((width.parse::<u32>().ok()?, height.parse::<u32>().ok()?)))
        .filter(|(width, height)| *width >= 720 && *height >= 480)
        .unwrap_or((1180, 780));
    WindowResolution::new(width, height)
}

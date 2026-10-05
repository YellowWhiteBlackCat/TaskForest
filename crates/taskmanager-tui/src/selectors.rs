//! Renderer-local resource, focus, and process-layout selectors.

use taskmanager_application::system_timeline::{
    SystemHistoryWindow, SystemPageSection, TimelineMetric,
};
use taskmanager_application::{AppPage, DirectoryUsageRequest, PlatformEffect, i18n::t};
use taskmanager_core::core::directory_usage::{
    DirectoryScanBounds, DirectoryScanSpec, DirectoryScanStatus,
};
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::GpuMetrics;
use taskmanager_core::core::sensors::SensorQuantity;
use taskmanager_platform_contract::CapabilityId;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use crate::TuiApp;
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsAction, present_gpu_engine_rows,
};
use taskmanager_shell::{ShellApp, gpu_chart_metric_gate};

/// Frontend-local selector for the Performance page's resource detail model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfDevice {
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
    Npu,
    Battery,
    Fan,
}

impl PerfDevice {
    pub const ALL: [PerfDevice; 7] = [
        PerfDevice::Cpu,
        PerfDevice::Memory,
        PerfDevice::Disk,
        PerfDevice::Network,
        PerfDevice::Gpu,
        PerfDevice::Battery,
        PerfDevice::Fan,
    ];

    #[must_use]
    pub const fn label_key(self) -> &'static str {
        match self {
            PerfDevice::Cpu => "common.cpu",
            PerfDevice::Memory => "common.memory",
            PerfDevice::Disk => "common.disk",
            PerfDevice::Network => "sidebar.network",
            PerfDevice::Gpu => "common.gpu",
            PerfDevice::Npu => "npu.title",
            PerfDevice::Battery => "common.battery",
            PerfDevice::Fan => "common.fan",
        }
    }

    #[must_use]
    pub fn from_digit(character: char) -> Option<Self> {
        let one_based = character.to_digit(10)? as usize;
        Self::ALL.get(one_based.checked_sub(1)?).copied()
    }
}

/// The keyboard focus target on the Applications page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FocusPanel {
    #[default]
    Table,
    Details,
}

impl FocusPanel {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Table => Self::Details,
            Self::Details => Self::Table,
        }
    }
}

impl TuiApp {
    /// Select one Performance resource and reset resource-local viewport
    /// intent so a scroll position from a dense CPU topology cannot leak into
    /// the next resource visit.
    pub(crate) fn select_perf_device(&mut self, device: PerfDevice) {
        self.perf_device = device;
        self.performance_device_key = None;
        self.cpu_core_scroll = 0;
        self.cpu_detail_scroll = 0;
        self.gpu_engine_scroll = 0;
        self.chart_cursor = None;
    }

    /// Move the CPU per-core viewport by logical grid rows. Paint clamps this
    /// intent against the current topology and terminal geometry.
    pub(crate) fn scroll_cpu_cores(&mut self, delta: isize) {
        if delta >= 0 {
            self.cpu_core_scroll = self.cpu_core_scroll.saturating_add(delta as usize);
        } else {
            self.cpu_core_scroll = self.cpu_core_scroll.saturating_sub(delta.unsigned_abs());
        }
    }

    /// Move the CPU details rail by terminal lines. The renderer clamps this
    /// intent against the wrapped row height, so a changing provider payload
    /// can never leave the rail beyond its last line.
    pub(crate) fn scroll_cpu_details(&mut self, delta: isize) {
        if delta >= 0 {
            self.cpu_detail_scroll = self.cpu_detail_scroll.saturating_add(delta as usize);
        } else {
            self.cpu_detail_scroll = self.cpu_detail_scroll.saturating_sub(delta.unsigned_abs());
        }
    }

    /// Move the standard GPU engine viewport. Compact paint omits that region,
    /// so this intent can never displace the primary chart or fact strip.
    pub(crate) fn scroll_gpu_engines(&mut self, delta: isize) {
        if delta >= 0 {
            self.gpu_engine_scroll = self.gpu_engine_scroll.saturating_add(delta as usize);
        } else {
            self.gpu_engine_scroll = self.gpu_engine_scroll.saturating_sub(delta.unsigned_abs());
        }
    }

    pub(crate) fn select_system_section(&mut self, section: SystemPageSection) {
        self.system_section = section;
        self.system_scroll = 0;
    }
    pub(crate) fn select_system_history_window_digit(&mut self, digit: char) -> bool {
        let window = digit
            .to_digit(10)
            .and_then(|value| value.checked_sub(1))
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| SystemHistoryWindow::ALL.get(index))
            .copied();
        if let Some(window) = window {
            self.system_history_window = window;
            self.system_scroll = 0;
            true
        } else {
            false
        }
    }

    /// Move the System section viewport. This is page navigation, not
    /// selection state: all typed facts remain in one fixed order.
    pub(crate) fn scroll_system(&mut self, delta: isize) {
        if delta >= 0 {
            self.system_scroll = self.system_scroll.saturating_add(delta as usize);
        } else {
            self.system_scroll = self.system_scroll.saturating_sub(delta.unsigned_abs());
        }
        if self.system_section == SystemPageSection::Dashboard {
            self.system_scroll = self
                .system_scroll
                .min(TimelineMetric::ALL.len().saturating_sub(1));
        }
    }

    /// Performance resources allowed by preferences and currently available facts.
    #[must_use]
    pub fn visible_perf_devices(&self) -> Vec<PerfDevice> {
        let mut devices = Vec::new();
        for entry in self
            .sidebar_entries()
            .into_iter()
            .filter(|entry| entry.visible)
        {
            if !devices.contains(&entry.device) {
                devices.push(entry.device);
            }
        }
        if !devices.contains(&PerfDevice::Fan)
            && self.projection().sensors.as_ref().is_some_and(|sensors| {
                sensors
                    .readings
                    .iter()
                    .any(|reading| reading.quantity() == &SensorQuantity::Temperature)
            })
        {
            devices.push(PerfDevice::Fan);
        }
        devices
    }

    #[must_use]
    pub fn select_perf_device_digit(&self, character: char) -> Option<PerfDevice> {
        let one_based = character.to_digit(10)? as usize;
        self.visible_perf_devices()
            .get(one_based.checked_sub(1)?)
            .copied()
    }

    /// Toggle the directory-usage scan lifecycle on the
    /// Performance page's Disk device. An idle or terminal slot starts a
    /// bounded scan of the first mounted partition (or `/` when none is
    /// reported); a `Scanning` slot cancels the active scan — mirroring
    /// GPUI's one-pill-per-partition start plus the conditional cancel pill,
    /// collapsed into a single keyboard toggle. The typed request crosses the
    /// shared seam: [`ShellApp::request_directory_usage`] wraps it in the
    /// `PlatformEffect::DirectoryUsage` variant the runtime routes through
    /// `queue_effect` — the exact same application lane every on-demand
    /// effect uses (G-03; no frontend-owned `PlatformClient` bypass).
    /// Progress and results fold back into the shared
    /// `SystemProjectionStore::directory_usage` slot the Disk panel renders.
    pub(crate) fn toggle_directory_scan(&mut self) -> Option<PlatformEffect> {
        if self.page() != AppPage::Performance || self.perf_device != PerfDevice::Disk {
            return None;
        }
        // Cancel path: an active (Scanning) scan toggles to Cancel, mirroring
        // GPUI's conditional cancel pill (only rendered while Scanning). The
        // scan state is the shared `SystemProjectionStore` slot (latest-wins).
        if let Some(snapshot) = self.shell.projection().directory_usage.as_ref()
            && snapshot.status == DirectoryScanStatus::Scanning
        {
            let scan_id = snapshot.scan_id;
            let root = snapshot.root.clone();
            self.report_notice(
                FeedbackSource::Control,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                t("tui.status.scan_cancelling").replacen("{}", &root, 1),
            );
            return Some(ShellApp::request_directory_usage(
                DirectoryUsageRequest::Cancel(scan_id),
            ));
        }
        // Start path: scan the first mounted partition (or `/`), mirroring
        // GPUI's default bounds (the UI never customizes depth/entry caps).
        let disk = self.sidebar_disks().first().copied()?;
        let root = disk
            .partitions
            .iter()
            .find(|p| !p.mount_point.is_empty())
            .map(|partition| partition.mount_point.clone())
            .unwrap_or_else(|| "/".to_string());
        let spec = DirectoryScanSpec {
            root: root.clone(),
            bounds: DirectoryScanBounds::default(),
        };
        self.report_notice(
            FeedbackSource::Control,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            t("tui.status.scan_started").replacen("{}", &root, 1),
        );
        Some(ShellApp::request_directory_usage(
            DirectoryUsageRequest::StartScan(spec),
        ))
    }

    /// Toggle the per-engine GPU utilization session (`e` on the
    /// Performance·GPU page): enable submits ONE bounded engine-rows request
    /// for the painted GPU identity (the OS-native prompt fires at most on this
    /// user-initiated request — the escalation discipline forbids
    /// auto-triggering), disable stops the TUI's re-request cadence. The typed
    /// answer lands in the shared request session, which is also the sole row
    /// payload authority. A closed session never displays stale rows as live.
    pub(crate) fn toggle_gpu_engine_rows(&mut self) -> Option<PlatformEffect> {
        if self.page() != AppPage::Performance || self.perf_device != PerfDevice::Gpu {
            return None;
        }
        let device_id = self.gpu_engine_rows_device_id()?;
        let action = present_gpu_engine_rows(
            self.shell.gpu_engine_rows_state(),
            &device_id,
            self.projection()
                .capability_status(&CapabilityId::TELEMETRY_GPU_ENGINES),
        )
        .action();
        match action {
            GpuEngineRowsAction::Disable => {
                self.shell.close_gpu_engine_rows_request();
                self.report_notice(
                    FeedbackSource::Control,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    t("tui.status.gpu_engines_stopped"),
                );
                None
            }
            GpuEngineRowsAction::Enable
            | GpuEngineRowsAction::Reauthorize
            | GpuEngineRowsAction::Recheck => {
                self.report_notice(
                    FeedbackSource::Control,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    t("tui.status.gpu_engines_requested"),
                );
                Some(ShellApp::request_gpu_engine_rows(device_id))
            }
            GpuEngineRowsAction::None => None,
        }
    }

    /// The device identity for the engine-rows request: the selected GPU or
    /// first visible GPU in the persisted order (the PMU helper reads the
    /// integrated engine block). `None` when no GPU exists — the toggle is an
    /// honest no-op rather than a request about nothing.
    pub(crate) fn gpu_engine_rows_device_id(&self) -> Option<DeviceId> {
        let gpu = self.sidebar_gpus().first().copied()?;
        let id = gpu.device_id.trim();
        (!id.is_empty()).then(|| DeviceId::new(id.to_owned()))
    }

    /// Cycle the GPU headline chart's metric family with `g` on the
    /// Performance·GPU page (ADR-034 stage 2). The selection, its
    /// availability gate, and the fixed vocabulary order live in the shared
    /// shell contract — this only routes the key and reports the resulting
    /// family in the status bar. No-op off the page, on another device, or
    /// when the viewed GPU reports no available family.
    pub(crate) fn cycle_gpu_chart_metric(&mut self) {
        if self.page() != AppPage::Performance || self.perf_device != PerfDevice::Gpu {
            return;
        }
        let gate = gpu_chart_metric_gate(self.viewed_gpu());
        if self.shell.cycle_gpu_chart_metric(&gate) {
            let selected = self.shell.gpu_chart_metric_selected();
            self.report_notice(
                FeedbackSource::Control,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                t("tui.status.gpu_series").replacen("{}", t(selected.label_key()), 1),
            );
        }
    }

    /// The GPU row the shared chart-metric selection is bound to: the selected
    /// device or first visible device in the persisted order (the panel's headline
    /// device — the same one the engine-rows session binds to). `None`
    /// everywhere else; the shell fold then leaves the selection untouched.
    pub(crate) fn viewed_gpu(&self) -> Option<&GpuMetrics> {
        if self.page() == AppPage::Performance && self.perf_device == PerfDevice::Gpu {
            self.sidebar_gpus().first().copied()
        } else {
            None
        }
    }
}

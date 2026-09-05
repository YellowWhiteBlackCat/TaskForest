//! Renderer-local resource, focus, and process-layout selectors.

use taskmanager_application::{AppPage, DirectoryUsageRequest, PlatformEffect, i18n::t};
use taskmanager_core::core::directory_usage::{
    DirectoryScanBounds, DirectoryScanSpec, DirectoryScanStatus,
};
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::GpuMetrics;
use taskmanager_platform_contract::CapabilityId;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use crate::TuiApp;

/// Frontend-local selector for the Performance page's resource detail model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfDevice {
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
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
        self.cpu_core_scroll = 0;
        self.gpu_engine_scroll = 0;
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

    /// Move the standard GPU engine viewport. Compact paint omits that region,
    /// so this intent can never displace the primary chart or fact strip.
    pub(crate) fn scroll_gpu_engines(&mut self, delta: isize) {
        if delta >= 0 {
            self.gpu_engine_scroll = self.gpu_engine_scroll.saturating_add(delta as usize);
        } else {
            self.gpu_engine_scroll = self.gpu_engine_scroll.saturating_sub(delta.unsigned_abs());
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
    }

    /// Performance resources allowed by preferences and currently available facts.
    #[must_use]
    pub fn visible_perf_devices(&self) -> Vec<PerfDevice> {
        let show = &self.prefs.show;
        let snapshot = self.shell.projection().snapshot.as_ref();
        let mut devices = Vec::new();
        if show[0] {
            devices.push(PerfDevice::Cpu);
        }
        if show[1] {
            devices.push(PerfDevice::Memory);
        }
        if show[2] && snapshot.is_some_and(|snapshot| !snapshot.disks.is_empty()) {
            devices.push(PerfDevice::Disk);
        }
        if show[3] && snapshot.is_some_and(|snapshot| !snapshot.networks.is_empty()) {
            devices.push(PerfDevice::Network);
        }
        if show[9] && snapshot.is_some_and(|snapshot| !snapshot.gpu.is_empty()) {
            devices.push(PerfDevice::Gpu);
        }
        if self
            .shell
            .projection()
            .power_supplies
            .as_ref()
            .is_some_and(|power| !power.batteries.is_empty())
        {
            devices.push(PerfDevice::Battery);
        }
        if self
            .shell
            .projection()
            .sensors
            .as_ref()
            .is_some_and(|sensors| {
                sensors.readings.iter().any(|reading| {
                    reading.quantity() == &taskmanager_core::core::sensors::SensorQuantity::FanSpeed
                })
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
            return Some(taskmanager_shell::ShellApp::request_directory_usage(
                DirectoryUsageRequest::Cancel(scan_id),
            ));
        }
        // Start path: scan the first mounted partition (or `/`), mirroring
        // GPUI's default bounds (the UI never customizes depth/entry caps).
        let root = self
            .projection()
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.disks.first())
            .and_then(|disk| disk.partitions.iter().find(|p| !p.mount_point.is_empty()))
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
        Some(taskmanager_shell::ShellApp::request_directory_usage(
            DirectoryUsageRequest::StartScan(spec),
        ))
    }

    /// Toggle the per-engine GPU utilization session (`e` on the
    /// Performance·GPU page): enable submits ONE bounded engine-rows request
    /// for the first GPU's device (the OS-native prompt fires at most on this
    /// user-initiated request — the escalation discipline forbids
    /// auto-triggering), disable stops the TUI's re-request cadence. The typed
    /// answer lands in the shared request session, which is also the sole row
    /// payload authority. A closed session never displays stale rows as live.
    pub(crate) fn toggle_gpu_engine_rows(&mut self) -> Option<PlatformEffect> {
        if self.page() != AppPage::Performance || self.perf_device != PerfDevice::Gpu {
            return None;
        }
        let device_id = self.gpu_engine_rows_device_id()?;
        let action = taskmanager_shell::presentation::gpu_engine_rows::present_gpu_engine_rows(
            self.shell.gpu_engine_rows_state(),
            &device_id,
            self.projection()
                .capability_status(&CapabilityId::TELEMETRY_GPU_ENGINES),
        )
        .action();
        match action {
            taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::Disable => {
                self.shell.close_gpu_engine_rows_request();
                self.report_notice(
                    FeedbackSource::Control,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    t("tui.status.gpu_engines_stopped"),
                );
                None
            }
            taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::Enable
            | taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::Reauthorize
            | taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::Recheck => {
                self.report_notice(
                    FeedbackSource::Control,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    t("tui.status.gpu_engines_requested"),
                );
                Some(taskmanager_shell::ShellApp::request_gpu_engine_rows(
                    device_id,
                ))
            }
            taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::None => None,
        }
    }

    /// The device identity for the engine-rows request: the first GPU's stable
    /// native identity from the live snapshot (the PMU helper reads the
    /// integrated engine block). `None` when no GPU exists — the toggle is an
    /// honest no-op rather than a request about nothing.
    pub(crate) fn gpu_engine_rows_device_id(&self) -> Option<DeviceId> {
        let gpu = self.projection().snapshot.as_ref()?.gpu.first()?;
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
        let gate = taskmanager_shell::gpu_chart_metric_gate(self.viewed_gpu());
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

    /// The GPU row the shared chart-metric selection is bound to: the first
    /// device of the Performance·GPU page's snapshot (the panel's headline
    /// device — the same one the engine-rows session binds to). `None`
    /// everywhere else; the shell fold then leaves the selection untouched.
    pub(crate) fn viewed_gpu(&self) -> Option<&GpuMetrics> {
        if self.page() == AppPage::Performance && self.perf_device == PerfDevice::Gpu {
            self.projection()
                .snapshot
                .as_ref()
                .and_then(|s| s.gpu.first())
        } else {
            None
        }
    }
}

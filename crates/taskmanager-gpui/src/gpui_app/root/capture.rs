//! Capture-only state preparation and structured evidence markers.
//!
//! The production UI remains driven by live collectors. When
//! `TM_CAPTURE_EVIDENCE=1` is present, this module emits deterministic markers
//! after live telemetry and UI-list updates reach `RootView`. Strict scenario
//! tokens can also prepare otherwise hard-to-reproduce presentation states;
//! capture preparation never invokes a destructive action.

use crate::gpui_app::dashboard::{DashboardPanel, DashboardState, EventCenterState, SystemSection};
use crate::gpui_app::process_insights::process_insights_capture_fixture;
use crate::gpui_app::timeline::HistoryWindow;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::group_aggregate::aggregate_apps_typed;
use taskmanager_core::core::process::{
    ProcessBatchAction, ProcessBatchIntent, ProcessCategory, ProcessItem, ProcessLiveKey,
    process_category,
};
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_core::core::{AlertEvent, ServiceId};
use taskmanager_telemetry_store::{
    CorrelatedSystemTelemetryHistory, CorrelatedSystemTelemetryIngestor,
};

use super::{ProcessDetailsSection, TopPage};

mod dashboard_history;
mod fixtures;
mod gpu_history;
mod marker;
mod process_fixtures;
mod scenarios;
mod state;
mod system_health;
use fixtures::{
    dynamic_power_fixture, dynamic_sensor_fixture, npu_inventory_fixture, prepare_active_alert,
    prepare_gpu_engine_inventory, prepare_hotplug, prepare_intel_gpu, prepare_missing_tool_disk,
    prepare_partition_disk, prepare_permission_disk,
};
use marker::{emit_marker, emit_theme_marker};
use process_fixtures::{
    prepare_apps_group_expanded, prepare_apps_identity_matrix, prepare_apps_search_highlight,
    prepare_apps_zero_gray, prepare_diagnostic_process, prepare_process_batch,
    prepare_process_histories, prepare_process_insights, prepare_process_memory_pss_swap,
    prepare_process_tree, prepare_startup_boot_markers, prepare_startup_failure_evidence,
    prepare_startup_impact,
};
pub use scenarios::CaptureScenario;
pub(super) use state::{
    CaptureDataReadiness, CaptureEvidence, CaptureMode, CaptureProcessAction,
    CaptureScenarioProgress, HistoryReplayOpenState, SystemHealthCaptureOutcome,
    SystemNpuCaptureState,
};
#[cfg(target_os = "linux")]
pub(super) use state::{WindowCaptureChain, WindowCaptureSchedule};

impl CaptureEvidence {
    pub(crate) const fn is_enabled(&self) -> bool {
        self.mode.enabled()
    }

    pub(crate) const fn telemetry_ready(&self) -> bool {
        self.data_readiness.telemetry_ready()
    }

    pub(crate) const fn ui_data_ready(&self) -> bool {
        self.data_readiness.ui_data_ready()
    }

    pub(crate) const fn scenario_ready(&self) -> bool {
        self.scenario_progress.ready()
    }

    fn mark_scenario_ready(&mut self) {
        if !self.scenario_ready() {
            self.scenario_progress = CaptureScenarioProgress::Ready;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    fn mark_telemetry_ready(&mut self) {
        if !self.telemetry_ready() {
            self.data_readiness = CaptureDataReadiness::TelemetryReady;
            emit_marker("telemetry_ready", self.scenario);
        }
    }

    fn mark_ui_data_ready(&mut self) {
        if !self.ui_data_ready() {
            self.data_readiness = CaptureDataReadiness::UiDataReady;
            emit_marker("ui_data_ready", self.scenario);
        }
    }

    pub fn mark_theme(&self, theme: &taskmanager_theme::Theme) {
        if self.is_enabled() {
            emit_theme_marker(self.scenario, theme);
        }
    }

    pub fn on_snapshot(&mut self, snapshot: &mut SystemSnapshot) {
        if !self.is_enabled() {
            return;
        }

        self.snapshot_count = self.snapshot_count.saturating_add(1);
        match self.scenario {
            Some(CaptureScenario::SmartMissingTool) => prepare_missing_tool_disk(snapshot),
            Some(CaptureScenario::SmartPermission) => prepare_permission_disk(snapshot),
            Some(CaptureScenario::DeviceHotplug) => prepare_hotplug(snapshot, self.snapshot_count),
            Some(CaptureScenario::GpuEngineInventory) => prepare_gpu_engine_inventory(snapshot),
            Some(CaptureScenario::PartitionDiskUsage) => prepare_partition_disk(snapshot),
            Some(CaptureScenario::IntelGpuTelemetry) => prepare_intel_gpu(snapshot),
            Some(CaptureScenario::ActiveAlert) => prepare_active_alert(snapshot),
            _ => {}
        }

        self.mark_telemetry_ready();
        let snapshot_scenario_ready = matches!(
            self.scenario,
            Some(
                CaptureScenario::SmartMissingTool
                    | CaptureScenario::SmartPermission
                    | CaptureScenario::PartitionDiskUsage
                    | CaptureScenario::IntelGpuTelemetry
                    | CaptureScenario::ActiveAlert
            )
        ) || (self.scenario == Some(CaptureScenario::DeviceHotplug)
            && self.snapshot_count >= 2)
            || (self.scenario == Some(CaptureScenario::PartitionLiveUsage)
                && snapshot.disks.iter().any(|disk| disk.partitions.len() >= 2));
        if snapshot_scenario_ready {
            self.mark_scenario_ready();
        }
    }

    /// Keep the stable selected-device key while the hot-plug fixture is in
    /// its disconnected settle window. The normal refresh path re-seeds an
    /// index selection from the current snapshot; doing that after the disk
    /// list is intentionally cleared would erase the key that the reconnect
    /// card needs to render and that the recovery frame needs to resolve.
    pub fn preserve_hotplug_selection(&self) -> bool {
        self.scenario == Some(CaptureScenario::DeviceHotplug) && self.scenario_ready()
    }

    /// Whether the Apps search-highlight fixture has reached the point where
    /// the RootView should select the Apps page and install its query.
    pub fn apps_search_highlight_requested(&self) -> bool {
        self.scenario == Some(CaptureScenario::AppsSearchHighlight) && self.scenario_ready()
    }

    /// Whether the Services search-highlight fixture has reached the point
    /// where the RootView should select the Services page and install its
    /// query.
    pub fn services_search_highlight_requested(&self) -> bool {
        self.scenario == Some(CaptureScenario::ServicesSearchHighlight) && self.scenario_ready()
    }

    /// Record that a real background process-list update reached RootView and,
    /// for the force-kill scenario, return a typed intent for the first live
    /// process. Returning an intent only prepares the dialog; execution remains
    /// exclusively behind the dialog's confirm button. `processes_observed_at_ms`
    /// is the accepted-snapshot timestamp behind `processes`.
    pub fn on_processes_update(
        &mut self,
        processes_updated: bool,
        processes_observed_at_ms: u64,
        processes: &mut Vec<ProcessItem>,
    ) -> Option<CaptureProcessAction> {
        if !self.is_enabled() || !processes_updated {
            return None;
        }
        self.mark_ui_data_ready();
        if self.scenario == Some(CaptureScenario::AppsSearchHighlight) {
            prepare_apps_search_highlight(processes);
            self.mark_scenario_ready();
            return None;
        }
        if self.scenario == Some(CaptureScenario::ProcessPropertiesPerformance)
            && self.scenario_ready()
        {
            if let Some(identity) = self.scenario_process_identity
                && let Some(process) = processes
                    .iter_mut()
                    .find(|process| ProcessLiveKey::from_process(process) == Some(identity))
            {
                prepare_process_histories(process);
            }
            return None;
        }
        // The strict insights fixture uses a synthetic identity. Keep that
        // identity in every refreshed process list until the screenshot is
        // taken; otherwise the normal 2 s process refresh removes it and the
        // Properties dialog correctly auto-closes before pixel capture.
        if self.scenario_ready()
            && self
                .scenario
                .is_some_and(CaptureScenario::is_process_insights)
        {
            let identity = prepare_process_insights(processes)?;
            debug_assert_eq!(self.scenario_process_identity, Some(identity));
            return None;
        }
        if self.scenario == Some(CaptureScenario::ProcessMemoryPssSwap) && self.scenario_ready() {
            prepare_process_memory_pss_swap(processes);
            return None;
        }
        if self.scenario == Some(CaptureScenario::AppsZeroGray) && self.scenario_ready() {
            prepare_apps_zero_gray(processes);
            return None;
        }
        if self.scenario == Some(CaptureScenario::AppsGroupExpanded) && self.scenario_ready() {
            prepare_apps_group_expanded(processes);
            return None;
        }
        if self.scenario == Some(CaptureScenario::AppsIdentityMatrix) && self.scenario_ready() {
            prepare_apps_identity_matrix(processes);
            return None;
        }
        if self.scenario_ready() {
            return None;
        }

        if self.scenario == Some(CaptureScenario::ProcessSelection) {
            let application_processes: Vec<&ProcessItem> = processes
                .iter()
                .filter(|process| process_category(process) == ProcessCategory::Application)
                .collect();
            let process = application_processes
                .iter()
                .copied()
                .find(|process| process.pid > 1 && process.name == "taskmanager")
                .or_else(|| {
                    application_processes
                        .iter()
                        .copied()
                        .filter(|process| {
                            process.pid > 1
                                && !process.name.starts_with("worker/")
                                && !process.name.starts_with("kworker")
                                && !process.name.starts_with('[')
                        })
                        .max_by(|left, right| {
                            left.current_cpu_percentage()
                                .partial_cmp(&right.current_cpu_percentage())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                })
                .or_else(|| {
                    application_processes
                        .iter()
                        .copied()
                        .find(|process| process.pid > 1)
                })?;
            let selected_identity = ProcessLiveKey::from_process(process)?;
            let root_identity =
                aggregate_apps_typed(&application_processes, processes_observed_at_ms)
                    .into_iter()
                    .find(|group| group.member_identities().contains(&selected_identity))?
                    .main_identity()?;
            self.scenario_process_identity = Some(root_identity);
            self.mark_scenario_ready();
            return Some(CaptureProcessAction::ApplicationSelection(root_identity));
        }

        if self.scenario == Some(CaptureScenario::ProcessMemoryPssSwap) {
            prepare_process_memory_pss_swap(processes);
            self.mark_scenario_ready();
            return None;
        }

        if self.scenario == Some(CaptureScenario::AppsZeroGray) {
            prepare_apps_zero_gray(processes);
            self.mark_scenario_ready();
            return None;
        }

        if self.scenario == Some(CaptureScenario::AppsGroupExpanded) {
            prepare_apps_group_expanded(processes);
            return None;
        }

        if self.scenario == Some(CaptureScenario::AppsIdentityMatrix) {
            prepare_apps_identity_matrix(processes);
            return None;
        }

        if self
            .scenario
            .is_some_and(CaptureScenario::is_process_insights)
        {
            if !self.telemetry_ready() {
                return None;
            }
            let identity = prepare_process_insights(processes)?;
            self.scenario_process_identity = Some(identity);
            return Some(CaptureProcessAction::Insights {
                identity,
                state: process_insights_capture_fixture(),
            });
        }

        if self.scenario == Some(CaptureScenario::ProcessTreeConfirm) {
            prepare_process_tree(processes);
            self.mark_scenario_ready();
            let identity = processes
                .iter()
                .find(|process| process.pid == 90_000)
                .and_then(ProcessLiveKey::from_process)?;
            let intent = ProcessBatchIntent::freeze_tree(
                processes,
                identity,
                ProcessBatchAction::EndProcessTree,
            );
            return (!intent.targets.is_empty()).then_some(CaptureProcessAction::Batch(intent));
        }

        if self.scenario == Some(CaptureScenario::ProcessBatchConfirm) {
            prepare_process_batch(processes);
            let intent = ProcessBatchIntent::freeze(
                processes,
                [91_001, 91_002, 91_003].into_iter().filter_map(|pid| {
                    processes
                        .iter()
                        .find(|process| process.pid == pid)
                        .and_then(ProcessLiveKey::from_process)
                }),
                ProcessBatchAction::Suspend,
            );
            return Some(CaptureProcessAction::Batch(intent));
        }

        if self.scenario == Some(CaptureScenario::DiagnosticPreview) {
            prepare_diagnostic_process(processes);
            return None;
        }

        let process = processes
            .iter()
            .find(|process| process.pid > 1 && process.name == "taskmanager")
            .or_else(|| {
                processes.iter().find(|process| {
                    process.pid > 1
                        && !process.name.starts_with("worker/")
                        && !process.name.starts_with("kworker")
                        && !process.name.starts_with('[')
                })
            })
            .or_else(|| processes.iter().find(|process| process.pid > 1))?;
        if self.scenario == Some(CaptureScenario::ProcessPropertiesPerformance) {
            let identity = ProcessLiveKey::from_process(process)?;
            if let Some(process) = processes
                .iter_mut()
                .find(|process| ProcessLiveKey::from_process(process) == Some(identity))
            {
                prepare_process_histories(process);
            }
            self.scenario_process_identity = Some(identity);
            self.mark_scenario_ready();
            return Some(CaptureProcessAction::Properties(
                identity,
                ProcessDetailsSection::Performance,
            ));
        }
        if self.scenario != Some(CaptureScenario::ProcessForceKill) {
            return None;
        }
        self.mark_scenario_ready();
        ProcessLiveKey::from_process(process).and_then(|identity| {
            let intent =
                ProcessBatchIntent::freeze(processes, [identity], ProcessBatchAction::Kill);
            (!intent.targets.is_empty()).then_some(CaptureProcessAction::Batch(intent))
        })
    }

    pub fn on_services_update(
        &mut self,
        services_updated: bool,
        services: &mut Vec<ServiceItem>,
    ) -> Option<ServiceId> {
        if !self.is_enabled() || !services_updated {
            return None;
        }
        if self.scenario == Some(CaptureScenario::ServicesSearchHighlight) {
            let fixture_id = ServiceId::new("fixture.service:p-4000".to_owned());
            services.retain(|service| service.id != fixture_id);
            services.push(ServiceItem::from_inventory(
                fixture_id,
                "p-4000",
                ServiceStatus::Active,
                "cap P-core (CPU 0-3) max frequency to 4000 MHz",
                "loaded",
                "active",
                "running",
            ));
            self.mark_scenario_ready();
            return None;
        }
        if self.scenario != Some(CaptureScenario::ServiceDetailsLogs) || self.scenario_ready() {
            return None;
        }
        if services.is_empty() {
            services.push(ServiceItem::from_inventory(
                ServiceId::new("fixture.service:taskmanager-capture.service"),
                "taskmanager-capture.service",
                ServiceStatus::Active,
                "Task Manager screenshot evidence service",
                "loaded",
                "active",
                "running",
            ));
        }
        Some(services[0].id.clone())
    }

    pub fn on_startup_update(
        &mut self,
        startup_updated: bool,
        entries: &mut Vec<StartupEntry>,
        evidence: &mut Option<taskmanager_core::core::startup::StartupBootEvidenceSnapshot>,
    ) -> bool {
        if !self.is_enabled()
            || !startup_updated
            || !matches!(
                self.scenario,
                Some(
                    CaptureScenario::StartupImpact
                        | CaptureScenario::StartupFailureEvidence
                        | CaptureScenario::StartupBootMarkers,
                )
            )
            || self.scenario_ready()
        {
            return false;
        }
        match self.scenario {
            Some(CaptureScenario::StartupBootMarkers) => {
                prepare_startup_boot_markers(entries, evidence, &mut self.startup_boot_baseline);
            }
            Some(CaptureScenario::StartupFailureEvidence) => {
                prepare_startup_failure_evidence(
                    entries,
                    evidence,
                    &mut self.startup_boot_baseline,
                );
            }
            _ => prepare_startup_impact(entries),
        }
        true
    }

    /// Keep a startup evidence fixture authoritative until the screenshot is
    /// taken. The live worker may publish a later systemd result after the
    /// inventory update that first triggered the scenario; without this
    /// re-application the readiness marker could describe a frame that the
    /// next platform event silently replaced.
    pub fn restore_startup_fixture(
        &mut self,
        entries: &mut Vec<StartupEntry>,
        evidence: &mut Option<taskmanager_core::core::startup::StartupBootEvidenceSnapshot>,
    ) {
        match self.scenario {
            Some(CaptureScenario::StartupFailureEvidence) => {
                prepare_startup_failure_evidence(
                    entries,
                    evidence,
                    &mut self.startup_boot_baseline,
                );
            }
            Some(CaptureScenario::StartupBootMarkers) => {
                prepare_startup_boot_markers(entries, evidence, &mut self.startup_boot_baseline);
            }
            _ => {}
        }
    }

    pub fn startup_boot_baseline(&self) -> Option<&taskmanager_core::core::startup::BootTimeline> {
        self.startup_boot_baseline.as_ref()
    }

    /// Take the capture-only alert history handoff after the dashboard state
    /// has requested the Event Center. No renderer-local history is retained.
    pub fn take_event_history_fixture(&mut self) -> Option<Vec<AlertEvent>> {
        self.event_history_fixture.take()
    }

    pub fn system_hardware_npu_fixture(
        &self,
    ) -> Option<taskmanager_core::core::NpuInventorySnapshot> {
        self.system_hardware_fixture_requested()
            .then(npu_inventory_fixture)
    }

    pub fn mark_system_npu_fixture_ready(&mut self, installed: bool) {
        if self.scenario == Some(CaptureScenario::SystemNpu)
            && installed
            && self.system_npu_state == SystemNpuCaptureState::AwaitingFixture
        {
            self.system_npu_state = SystemNpuCaptureState::AwaitingLayout;
        }
    }

    pub fn system_npu_layout_requested(&self) -> bool {
        self.scenario == Some(CaptureScenario::SystemNpu)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
            && self.system_npu_state == SystemNpuCaptureState::AwaitingLayout
    }

    /// Atomically claim one post-layout scroll attempt. Repeated renders before
    /// the next frame cannot queue duplicate callbacks.
    pub fn schedule_system_npu_scroll(&mut self) -> bool {
        if !self.system_npu_layout_requested() {
            return false;
        }
        self.system_npu_state = SystemNpuCaptureState::ScrollScheduled;
        true
    }

    pub fn mark_system_npu_scroll_applied(&mut self, graphics_visible: bool) {
        if self.scenario != Some(CaptureScenario::SystemNpu)
            || self.system_npu_state != SystemNpuCaptureState::ScrollScheduled
        {
            return;
        }
        if graphics_visible {
            self.system_npu_state = SystemNpuCaptureState::Ready;
            self.mark_scenario_ready();
        } else {
            self.system_npu_state = SystemNpuCaptureState::AwaitingLayout;
        }
    }

    pub fn seed_gpu_engine_inventory_history(
        &mut self,
        history: &CorrelatedSystemTelemetryHistory,
        ingestor: &CorrelatedSystemTelemetryIngestor,
        anchor_timestamp_ms: u64,
    ) -> bool {
        if self.scenario != Some(CaptureScenario::GpuEngineInventory)
            || !self.telemetry_ready()
            || !self.ui_data_ready()
            || self.scenario_ready()
        {
            return false;
        }
        let ready = gpu_history::seed(history, ingestor, anchor_timestamp_ms);
        if ready {
            self.mark_scenario_ready();
        }
        ready
    }

    /// Prepare one of the dashboard-owned controlled states. The special marker
    /// is deliberately delayed until both live telemetry and a real UI-list
    /// update reached RootView; callers invoke this after each kind of update.
    pub fn on_dashboard_state(
        &mut self,
        dashboard: &mut DashboardState,
        history: &CorrelatedSystemTelemetryHistory,
        ingestor: &CorrelatedSystemTelemetryIngestor,
        anchor_timestamp_ms: u64,
    ) -> Option<DashboardPanel> {
        if !self.is_enabled()
            || !self.telemetry_ready()
            || !self.ui_data_ready()
            || self.scenario_ready()
        {
            return None;
        }
        let (handled, panel) = match self.scenario {
            Some(CaptureScenario::SystemDashboard) => {
                dashboard.section = SystemSection::Dashboard;
                dashboard.history_window = HistoryWindow::FifteenMinutes;
                (
                    dashboard_history::seed(history, ingestor, anchor_timestamp_ms),
                    None,
                )
            }
            Some(CaptureScenario::SystemHardware) => {
                dashboard.section = SystemSection::Hardware;
                (true, None)
            }
            Some(CaptureScenario::SystemNpu) => {
                dashboard.section = SystemSection::Hardware;
                // Readiness belongs to the post-layout scroll state above.
                (false, None)
            }
            Some(CaptureScenario::HistorySixtyMinutes) => {
                dashboard.section = SystemSection::Dashboard;
                dashboard.history_window = HistoryWindow::SixtyMinutes;
                (
                    dashboard_history::seed(history, ingestor, anchor_timestamp_ms),
                    None,
                )
            }
            Some(CaptureScenario::AlertRulesManager) => {
                dashboard.section = SystemSection::Dashboard;
                (true, Some(DashboardPanel::AlertRules))
            }
            Some(CaptureScenario::EventCenter) => {
                dashboard.section = SystemSection::Dashboard;
                self.event_history_fixture = Some(EventCenterState::capture_event_fixture());
                (true, Some(DashboardPanel::Events))
            }
            Some(CaptureScenario::SavedViewPresets) => {
                dashboard.section = SystemSection::Dashboard;
                dashboard.add_capture_saved_view();
                (true, Some(DashboardPanel::SavedViews))
            }
            _ => (false, None),
        };
        if handled {
            self.mark_scenario_ready();
        }
        panel
    }

    /// Prepare deterministic runtime power/fan pages after the normal live
    /// telemetry and process-list readiness gates. The fixture remains inside
    /// the dynamic capability projection; it never mutates static hardware
    /// inventory and does not perform provider I/O.
    pub fn dynamic_device_fixture_requested(&self) -> bool {
        self.scenario == Some(CaptureScenario::BatteryFanPerformance)
    }

    /// Mark a live dynamic-device capture only after the provider supplied the
    /// requested real capability. Unlike the deterministic fixture path above,
    /// this method never inserts or rewrites a Battery/Fan observation.
    pub fn on_live_dynamic_device_state(
        &mut self,
        page: &mut TopPage,
        power_supplies: &taskmanager_core::core::PowerSupplySnapshot,
    ) -> bool {
        if !self.is_enabled()
            || !self.telemetry_ready()
            || !self.ui_data_ready()
            || self.scenario_ready()
        {
            return false;
        }
        let target_ready = match self.scenario {
            Some(CaptureScenario::BatteryLivePerformance) => !power_supplies.batteries.is_empty(),
            _ => false,
        };
        if !target_ready {
            return false;
        }
        *page = TopPage::Performance;
        self.mark_scenario_ready();
        true
    }

    pub fn process_memory_pss_swap_requested(&self) -> bool {
        self.scenario == Some(CaptureScenario::ProcessMemoryPssSwap) && self.scenario_ready()
    }

    pub fn on_dynamic_device_state(
        &mut self,
        page: &mut TopPage,
        power_supplies: &mut taskmanager_core::core::PowerSupplySnapshot,
        sensors: &mut taskmanager_core::core::SensorCenterSnapshot,
    ) -> bool {
        if !self.is_enabled()
            || !self.telemetry_ready()
            || !self.ui_data_ready()
            || !self.dynamic_device_fixture_requested()
            || self.scenario_ready()
        {
            return false;
        }
        *page = TopPage::Performance;
        *power_supplies = dynamic_power_fixture();
        *sensors = dynamic_sensor_fixture();
        self.mark_scenario_ready();
        true
    }
}

#[cfg(feature = "test-support")]
impl super::RootView {
    /// Seed deterministic dashboard evidence through the production telemetry authority.
    pub fn seed_dashboard_capture_history(&self, newest_timestamp_ms: u64) -> bool {
        dashboard_history::seed(
            &self.telemetry.system_history,
            &self.telemetry_ingestor,
            newest_timestamp_ms,
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_app/root/capture/tests.rs"]
mod tests;

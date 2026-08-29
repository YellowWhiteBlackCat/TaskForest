//! Strict capture scenario tokens and readiness-marker gates.

use super::{CaptureEvidence, emit_marker};
use std::path::PathBuf;
use taskmanager_core::core::startup::{StartupEntry, StartupImpactEvidence};

use taskmanager_core::core::setup::SetupScriptInfo;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureScenario {
    SmartMissingTool,
    SmartPermission,
    DeviceHotplug,
    GpuEngineInventory,
    IntelGpuTelemetry,
    ProcessForceKill,
    ProcessSelection,
    ProcessMemoryPssSwap,
    ProcessPropertiesPerformance,
    ProcessTreeConfirm,
    ProcessBatchConfirm,
    ServiceDetailsLogs,
    StartupImpact,
    StartupFailureEvidence,
    StartupBootMarkers,
    DiagnosticPreview,
    DiagnosticFailure,
    ActiveAlert,
    SystemDashboard,
    SystemHardware,
    SystemNpu,
    HistorySixtyMinutes,
    AlertRulesManager,
    EventCenter,
    SavedViewPresets,
    ProcessNetworkDetails,
    ProcessGpuDetails,
    ProcessResourceLimits,
    ProcessIsolation,
    StorageHealth,
    SmartSelfTestConfirm,
    SensorCenter,
    BatteryFanPerformance,
    BatteryLivePerformance,
    PartitionDiskUsage,
    PartitionLiveUsage,
    KeyboardFocus,
    VerticalNav,
    SettingsSwitchFocus,
    SettingsZeroGray,
    AppsSearchHighlight,
    ServicesSearchHighlight,
    AppsZeroGray,
    AppsGroupExpanded,
    AppsIdentityMatrix,
    SidebarHidden,
    SidebarEdit,
    TelemetryPaused,
    SystemAbout,
    About,
    FirstRun,
    HistoryReplay,
    ApplicationHistoryReplay,
}

impl CaptureScenario {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "smart-missing-tool" => Some(Self::SmartMissingTool),
            "smart-permission" => Some(Self::SmartPermission),
            "device-hotplug" => Some(Self::DeviceHotplug),
            "gpu-engine-inventory" => Some(Self::GpuEngineInventory),
            "intel-gpu-telemetry" => Some(Self::IntelGpuTelemetry),
            "process-force-kill" => Some(Self::ProcessForceKill),
            "process-selection" => Some(Self::ProcessSelection),
            "process-memory-pss-swap" => Some(Self::ProcessMemoryPssSwap),
            "process-properties-performance" => Some(Self::ProcessPropertiesPerformance),
            "process-tree-confirm" => Some(Self::ProcessTreeConfirm),
            "process-batch-confirm" => Some(Self::ProcessBatchConfirm),
            "service-details-logs" => Some(Self::ServiceDetailsLogs),
            "startup-impact" => Some(Self::StartupImpact),
            "startup-failure-evidence" => Some(Self::StartupFailureEvidence),
            "startup-boot-markers" => Some(Self::StartupBootMarkers),
            "diagnostic-preview" => Some(Self::DiagnosticPreview),
            "diagnostic-failure" => Some(Self::DiagnosticFailure),
            "active-alert" => Some(Self::ActiveAlert),
            "system-dashboard" => Some(Self::SystemDashboard),
            "system-hardware" => Some(Self::SystemHardware),
            "system-npu" => Some(Self::SystemNpu),
            "history-60m" => Some(Self::HistorySixtyMinutes),
            "alert-rules-manager" => Some(Self::AlertRulesManager),
            "event-center" => Some(Self::EventCenter),
            "saved-view-presets" => Some(Self::SavedViewPresets),
            "process-network-details" => Some(Self::ProcessNetworkDetails),
            "process-gpu-details" => Some(Self::ProcessGpuDetails),
            "process-resource-limits" => Some(Self::ProcessResourceLimits),
            "process-isolation" => Some(Self::ProcessIsolation),
            "storage-health" => Some(Self::StorageHealth),
            "smart-self-test-confirm" => Some(Self::SmartSelfTestConfirm),
            "sensor-center" => Some(Self::SensorCenter),
            "battery-fan-performance" => Some(Self::BatteryFanPerformance),
            "battery-live-performance" => Some(Self::BatteryLivePerformance),
            "partition-disk-usage" => Some(Self::PartitionDiskUsage),
            "partition-live-usage" => Some(Self::PartitionLiveUsage),
            "keyboard-focus" => Some(Self::KeyboardFocus),
            "vertical-nav" => Some(Self::VerticalNav),
            "settings-switch-focus" => Some(Self::SettingsSwitchFocus),
            "settings-zero-gray" => Some(Self::SettingsZeroGray),
            "apps-search-highlight" => Some(Self::AppsSearchHighlight),
            "services-search-highlight" => Some(Self::ServicesSearchHighlight),
            "apps-zero-gray" => Some(Self::AppsZeroGray),
            "apps-group-expanded" => Some(Self::AppsGroupExpanded),
            "apps-identity-matrix" => Some(Self::AppsIdentityMatrix),
            "sidebar-hidden" => Some(Self::SidebarHidden),
            "sidebar-edit" => Some(Self::SidebarEdit),
            "telemetry-paused" => Some(Self::TelemetryPaused),
            "system-about" => Some(Self::SystemAbout),
            "about" => Some(Self::About),
            "first-run" => Some(Self::FirstRun),
            "history-replay" => Some(Self::HistoryReplay),
            "application-history-replay" => Some(Self::ApplicationHistoryReplay),
            _ => None,
        }
    }

    pub fn token(self) -> &'static str {
        match self {
            Self::SmartMissingTool => "smart-missing-tool",
            Self::SmartPermission => "smart-permission",
            Self::DeviceHotplug => "device-hotplug",
            Self::GpuEngineInventory => "gpu-engine-inventory",
            Self::IntelGpuTelemetry => "intel-gpu-telemetry",
            Self::ProcessForceKill => "process-force-kill",
            Self::ProcessSelection => "process-selection",
            Self::ProcessMemoryPssSwap => "process-memory-pss-swap",
            Self::ProcessPropertiesPerformance => "process-properties-performance",
            Self::ProcessTreeConfirm => "process-tree-confirm",
            Self::ProcessBatchConfirm => "process-batch-confirm",
            Self::ServiceDetailsLogs => "service-details-logs",
            Self::StartupImpact => "startup-impact",
            Self::StartupFailureEvidence => "startup-failure-evidence",
            Self::StartupBootMarkers => "startup-boot-markers",
            Self::DiagnosticPreview => "diagnostic-preview",
            Self::DiagnosticFailure => "diagnostic-failure",
            Self::ActiveAlert => "active-alert",
            Self::SystemDashboard => "system-dashboard",
            Self::SystemHardware => "system-hardware",
            Self::SystemNpu => "system-npu",
            Self::HistorySixtyMinutes => "history-60m",
            Self::AlertRulesManager => "alert-rules-manager",
            Self::EventCenter => "event-center",
            Self::SavedViewPresets => "saved-view-presets",
            Self::ProcessNetworkDetails => "process-network-details",
            Self::ProcessGpuDetails => "process-gpu-details",
            Self::ProcessResourceLimits => "process-resource-limits",
            Self::ProcessIsolation => "process-isolation",
            Self::StorageHealth => "storage-health",
            Self::SmartSelfTestConfirm => "smart-self-test-confirm",
            Self::SensorCenter => "sensor-center",
            Self::BatteryFanPerformance => "battery-fan-performance",
            Self::BatteryLivePerformance => "battery-live-performance",
            Self::PartitionDiskUsage => "partition-disk-usage",
            Self::PartitionLiveUsage => "partition-live-usage",
            Self::KeyboardFocus => "keyboard-focus",
            Self::VerticalNav => "vertical-nav",
            Self::SettingsSwitchFocus => "settings-switch-focus",
            Self::SettingsZeroGray => "settings-zero-gray",
            Self::AppsSearchHighlight => "apps-search-highlight",
            Self::ServicesSearchHighlight => "services-search-highlight",
            Self::AppsZeroGray => "apps-zero-gray",
            Self::AppsGroupExpanded => "apps-group-expanded",
            Self::AppsIdentityMatrix => "apps-identity-matrix",
            Self::SidebarHidden => "sidebar-hidden",
            Self::SidebarEdit => "sidebar-edit",
            Self::TelemetryPaused => "telemetry-paused",
            Self::SystemAbout => "system-about",
            Self::About => "about",
            Self::FirstRun => "first-run",
            Self::HistoryReplay => "history-replay",
            Self::ApplicationHistoryReplay => "application-history-replay",
        }
    }

    pub(super) fn is_process_insights(self) -> bool {
        matches!(
            self,
            Self::ProcessNetworkDetails
                | Self::ProcessGpuDetails
                | Self::ProcessResourceLimits
                | Self::ProcessIsolation
        )
    }
}

impl CaptureEvidence {
    pub fn from_environment() -> Self {
        let scenario = std::env::var("TM_CAPTURE_SCENARIO")
            .ok()
            .as_deref()
            .and_then(CaptureScenario::parse);
        let explicitly_enabled = std::env::var("TM_CAPTURE_EVIDENCE")
            .ok()
            .is_some_and(|value| !value.is_empty() && value != "0");
        Self {
            enabled: explicitly_enabled || scenario.is_some(),
            scenario,
            ..Self::default()
        }
    }

    /// Strict health scenarios use deterministic snapshots and never start a
    /// platform worker, keeping capture preparation free of provider I/O.
    pub fn system_health_fixture_requested(&self) -> bool {
        matches!(
            self.scenario,
            Some(
                CaptureScenario::StorageHealth
                    | CaptureScenario::SmartSelfTestConfirm
                    | CaptureScenario::SensorCenter
            )
        )
    }

    pub fn system_hardware_fixture_requested(&self) -> bool {
        let scenario_needs_fixture = match self.scenario {
            Some(CaptureScenario::SystemHardware) => !self.scenario_ready,
            Some(CaptureScenario::SystemNpu) => self.system_npu_state.needs_fixture(),
            _ => false,
        };
        self.enabled && scenario_needs_fixture && self.telemetry_ready && self.ui_data_ready
    }

    /// A strict keyboard capture waits for real telemetry and list data before
    /// asking RootView to focus the rendered Applications search field.
    pub fn keyboard_focus_requested(&self) -> bool {
        self.scenario == Some(CaptureScenario::KeyboardFocus)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_keyboard_focus_ready(&mut self) {
        if self.keyboard_focus_requested() {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict vertical-navigation evidence waits for live telemetry and the
    /// process list before switching the real root shell to its left-side tab
    /// rail. The capture runner then binds the screenshot to the same page and
    /// window as every other scenario.
    pub fn vertical_nav_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::VerticalNav)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_vertical_nav_ready(&mut self, vertical: bool) {
        if self.vertical_nav_requested() && vertical {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict Settings evidence waits for the normal telemetry/UI readiness
    /// markers before opening the dialog and focusing its CPU visibility switch.
    pub fn settings_switch_focus_requested(&self) -> bool {
        self.settings_switch_focus_enabled()
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn settings_switch_focus_enabled(&self) -> bool {
        self.scenario == Some(CaptureScenario::SettingsSwitchFocus)
    }

    pub fn mark_settings_switch_focus_ready(&mut self) {
        if self.settings_switch_focus_requested() {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict Settings evidence for the Apps zero-value preference waits for
    /// real telemetry/list readiness before scrolling to and focusing the new
    /// lower-row control.
    pub fn settings_zero_gray_requested(&self) -> bool {
        self.settings_zero_gray_enabled()
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn settings_zero_gray_enabled(&self) -> bool {
        self.scenario == Some(CaptureScenario::SettingsZeroGray)
    }

    /// Strict Apps evidence prepares measured zero resource values while the
    /// real process table remains the only rendered source. The scenario only
    /// changes the capture fixture and preference seed; production providers
    /// never learn about this presentation-only token.
    pub fn apps_zero_gray_enabled(&self) -> bool {
        self.scenario == Some(CaptureScenario::AppsZeroGray)
    }

    /// Strict Apps evidence prepares a bounded multi-process application group
    /// only after the live telemetry and process-list readiness markers. The
    /// RootView owns the Group-by-app projection and acknowledges the exact
    /// expanded state through [`Self::mark_apps_group_expanded_ready`].
    pub fn apps_group_expanded_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::AppsGroupExpanded)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_apps_group_expanded_ready(&mut self, expanded: bool) {
        if self.apps_group_expanded_requested() && expanded {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict Apps identity evidence prepares a bounded, high-signal matrix
    /// containing PWA, Snap, and AppImage-shaped process facts. The marker is
    /// emitted only after the RootView projection observes all three validated
    /// application assets; the fixture itself never claims those processes are
    /// present on the host.
    pub fn apps_identity_matrix_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::AppsIdentityMatrix)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_apps_identity_matrix_ready(&mut self, ready: bool) {
        if self.apps_identity_matrix_requested() && ready {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict F9 evidence waits for both live telemetry and a real process-list
    /// update before projecting the hidden Performance device navigator. The
    /// state change remains in RootView; this method only owns the readiness
    /// contract and marker.
    pub fn sidebar_hidden_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::SidebarHidden)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_sidebar_hidden_ready(&mut self, hidden: bool) {
        if self.sidebar_hidden_requested() && hidden {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict sidebar-edit evidence waits for the normal live snapshot/process
    /// readiness before enabling the concrete-device edit projection.
    pub fn sidebar_edit_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::SidebarEdit)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_sidebar_edit_ready(&mut self, editing: bool) {
        if self.sidebar_edit_requested() && editing {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict pause evidence waits for live telemetry/list readiness before
    /// projecting the held-Ctrl paused state. The capture path sets only the
    /// transient application policy bit; the real modifier lifecycle is
    /// proven separately by the GPUI behavior test.
    pub fn telemetry_paused_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::TelemetryPaused)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_telemetry_paused_ready(&mut self, paused: bool) {
        if self.telemetry_paused_requested() && paused {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict System Information evidence waits for the same live telemetry
    /// and process-list readiness as the other modal scenarios. Opening the
    /// modal is a RootView projection; it does not trigger another provider
    /// request.
    pub fn system_about_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::SystemAbout)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_system_about_ready(&mut self, open: bool) {
        if self.system_about_requested() && open {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict About evidence waits for the same live read-model readiness as
    /// System Information, then opens the independent build-metadata modal.
    /// No host facts or shell action are fabricated by this presentation-only
    /// scenario.
    pub fn about_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::About)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_about_ready(&mut self, open: bool) {
        if self.about_requested() && open {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict First Run evidence prepares only the dialog projection after the
    /// normal live telemetry/UI readiness gates. It never invokes setup,
    /// pkexec, a helper, or a restart; the resulting marker is therefore a
    /// pixel-fixture receipt, not a functional setup receipt.
    pub fn first_run_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::FirstRun)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn first_run_fixture_enabled(&self) -> bool {
        self.scenario == Some(CaptureScenario::FirstRun)
    }

    pub fn mark_first_run_ready(&mut self, open: bool) {
        if self.first_run_requested() && open {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// The fixture mirrors the fixed descriptor emitted by the Linux provider,
    /// but stays in the capture layer so a screenshot cannot accidentally
    /// prove that the installed asset or privileged helper exists.
    #[must_use]
    pub fn first_run_fixture_info() -> SetupScriptInfo {
        SetupScriptInfo {
            path: PathBuf::from("/usr/share/taskforest/setup/99-taskforest.rules"),
            run_command: "pkexec /usr/libexec/taskforest-setup-helper install".to_owned(),
            revert_command: "pkexec /usr/libexec/taskforest-setup-helper revert".to_owned(),
        }
    }

    pub fn mark_settings_zero_gray_ready(&mut self) {
        if self.settings_zero_gray_requested() {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    pub fn mark_process_batch_ready(&mut self, confirmation_ready: bool, selected_count: usize) {
        if self.scenario == Some(CaptureScenario::ProcessBatchConfirm)
            && confirmation_ready
            && selected_count >= 2
            && !self.scenario_ready
        {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    pub fn mark_service_details_ready(&mut self, details_ready: bool) {
        if self.scenario == Some(CaptureScenario::ServiceDetailsLogs)
            && details_ready
            && !self.scenario_ready
        {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    pub fn mark_process_insights_ready(&mut self, dialog_ready: bool) {
        if self
            .scenario
            .is_some_and(CaptureScenario::is_process_insights)
            && self.telemetry_ready
            && self.ui_data_ready
            && dialog_ready
            && !self.scenario_ready
        {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    pub fn mark_startup_impact_ready(&mut self, page_ready: bool, entries: &[StartupEntry]) {
        let evidence_ready = entries.iter().any(|entry| {
            matches!(
                entry.impact_evidence,
                StartupImpactEvidence::Measured { .. }
            )
        }) && entries
            .iter()
            .any(|entry| matches!(entry.impact_evidence, StartupImpactEvidence::Unknown { .. }));
        if self.scenario == Some(CaptureScenario::StartupImpact)
            && page_ready
            && evidence_ready
            && !self.scenario_ready
        {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    pub fn mark_startup_failure_evidence_ready(
        &mut self,
        page_ready: bool,
        evidence: Option<&taskmanager_core::core::startup::StartupBootEvidenceSnapshot>,
    ) {
        if self.scenario == Some(CaptureScenario::StartupFailureEvidence)
            && page_ready
            && evidence.is_some_and(|evidence| {
                evidence.failed_units.len() >= 3 && evidence.critical_chain.len() >= 2
            })
            && !self.scenario_ready
        {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Boot-markers evidence is ready when the Startup page is active AND the
    /// waterfall evidence plus its comparison baseline are both seeded (the
    /// chips need both boots to compare).
    pub fn mark_startup_boot_markers_ready(&mut self, page_ready: bool, seeded: bool) {
        if self.scenario == Some(CaptureScenario::StartupBootMarkers)
            && page_ready
            && seeded
            && !self.scenario_ready
        {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Strict replay evidence: after the normal readiness markers, open the
    /// Performance page's history-replay panel exactly once. The panel's
    /// data flows through the REAL query over the capture-private history
    /// directory the script pre-seeded with JSONL fixtures — this token only
    /// flips the panel open, it never fabricates rows.
    pub fn history_replay_open_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::HistoryReplay)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
            && !self.history_replay_opened
    }

    pub fn note_history_replay_opened(&mut self) {
        self.history_replay_opened = true;
    }

    /// Replay readiness is re-checked every tick until the async load lands:
    /// the panel is visible, no load in flight, and rows actually arrived.
    pub fn mark_history_replay_ready(&mut self, loaded: bool) {
        if self.scenario == Some(CaptureScenario::HistoryReplay) && loaded && !self.scenario_ready {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Application-history capture is ready only after the durable replay
    /// projection itself reaches `Ready` with at least one joined identity.
    /// Page selection or an active reader alone must never certify evidence.
    pub fn mark_application_history_replay_ready(
        &mut self,
        page_ready: bool,
        status: taskmanager_application::ApplicationHistoryStatus,
        row_count: usize,
    ) {
        if self.scenario == Some(CaptureScenario::ApplicationHistoryReplay)
            && self.telemetry_ready
            && self.ui_data_ready
            && page_ready
            && status == taskmanager_application::ApplicationHistoryStatus::Ready
            && row_count > 0
            && !self.scenario_ready
        {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Request the privacy preview only after real telemetry and list readiness.
    /// The caller must prepare the preview and then acknowledge success; this
    /// method never confirms or writes the bundle.
    pub fn diagnostic_preview_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::DiagnosticPreview)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_diagnostic_preview_ready(&mut self, preview_ready: bool) {
        if self.diagnostic_preview_requested() && preview_ready {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }

    /// Request a controlled typed failure only after the normal readiness
    /// markers. The caller assigns UI state directly; no worker is submitted.
    pub fn diagnostic_failure_requested(&self) -> bool {
        self.enabled
            && self.scenario == Some(CaptureScenario::DiagnosticFailure)
            && self.telemetry_ready
            && self.ui_data_ready
            && !self.scenario_ready
    }

    pub fn mark_diagnostic_failure_ready(&mut self, failure_ready: bool) {
        if self.diagnostic_failure_requested() && failure_ready {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
    }
}

impl crate::gpui_app::root::RootView {
    /// Capture certification follows the accepted replay completion directly;
    /// it must not wait for an unrelated platform batch to happen afterward.
    pub(in crate::gpui_app) fn sync_history_capture_readiness(&mut self) {
        let performance_loaded = self.history_replay_visible()
            && !self.history_replay_state().is_loading()
            && !self.history_replay_state().rows().is_empty();
        let application_history = self
            .history_runtime
            .replay()
            .application_history_projection(self.history_runtime.application_history_capability());
        self.capture_evidence
            .mark_history_replay_ready(performance_loaded);
        self.capture_evidence.mark_application_history_replay_ready(
            self.page == crate::gpui_app::root::TopPage::AppHistory,
            application_history.status,
            application_history.rows.len(),
        );
    }
}

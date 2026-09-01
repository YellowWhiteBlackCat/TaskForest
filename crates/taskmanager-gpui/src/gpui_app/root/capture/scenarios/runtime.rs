//! Capture coordinator transitions and scenario preparation.

use super::super::{
    CaptureEvidence, CaptureMode, HistoryReplayOpenState, WindowCaptureChain, WindowCaptureSchedule,
};
use super::CaptureScenario;
use std::path::PathBuf;
use taskmanager_core::core::setup::SetupScriptInfo;
use taskmanager_core::core::startup::{StartupEntry, StartupImpactEvidence};

impl CaptureEvidence {
    /// Inventory fixtures must be able to start from an accepted platform
    /// batch even when the live provider reports no new service/startup
    /// inventory. Capture-only preparation still runs after the normal batch
    /// fold; this predicate only asks the caller to give the scenario one
    /// opportunity to install its typed fixture.
    pub(crate) fn service_inventory_capture_requested(&self) -> bool {
        self.is_enabled()
            && !self.scenario_ready()
            && matches!(
                self.scenario,
                Some(
                    CaptureScenario::ServiceDetailsLogs | CaptureScenario::ServicesSearchHighlight,
                )
            )
    }

    /// Startup capture has the same provider-independent trigger as service
    /// capture. The fixture is still folded into the canonical startup
    /// projection and readiness remains gated by the page/evidence checks.
    pub(crate) fn startup_inventory_capture_requested(&self) -> bool {
        self.is_enabled()
            && !self.scenario_ready()
            && matches!(
                self.scenario,
                Some(
                    CaptureScenario::StartupImpact
                        | CaptureScenario::StartupFailureEvidence
                        | CaptureScenario::StartupBootMarkers
                )
            )
    }

    pub fn from_environment() -> Self {
        let scenario = std::env::var("TM_CAPTURE_SCENARIO")
            .ok()
            .as_deref()
            .and_then(CaptureScenario::parse);
        let explicitly_enabled = std::env::var("TM_CAPTURE_EVIDENCE")
            .ok()
            .is_some_and(|value| !value.is_empty() && value != "0");
        let window_capture_chain = if std::env::var("TM_CAPTURE_WINDOW_CHAIN")
            .ok()
            .is_some_and(|value| !value.is_empty() && value != "0")
        {
            WindowCaptureChain::Active
        } else {
            WindowCaptureChain::default()
        };
        let window_capture_schedule = if window_capture_chain.active() {
            WindowCaptureSchedule::AwaitingFrame
        } else {
            WindowCaptureSchedule::Inactive
        };
        Self {
            mode: if explicitly_enabled || scenario.is_some() || window_capture_chain.active() {
                CaptureMode::Enabled
            } else {
                CaptureMode::default()
            },
            scenario,
            window_capture_schedule,
            window_capture_chain,
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
            Some(CaptureScenario::SystemHardware) => !self.scenario_ready(),
            Some(CaptureScenario::SystemNpu) => self.system_npu_state.needs_fixture(),
            _ => false,
        };
        self.is_enabled()
            && scenario_needs_fixture
            && self.telemetry_ready()
            && self.ui_data_ready()
    }

    /// A strict keyboard capture waits for real telemetry and list data before
    /// asking RootView to focus the rendered Applications search field.
    pub fn keyboard_focus_requested(&self) -> bool {
        self.scenario == Some(CaptureScenario::KeyboardFocus)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_keyboard_focus_ready(&mut self) {
        if self.keyboard_focus_requested() {
            self.mark_scenario_ready();
        }
    }

    /// Strict vertical-navigation evidence waits for live telemetry and the
    /// process list before switching the real root shell to its left-side tab
    /// rail. The capture runner then binds the screenshot to the same page and
    /// window as every other scenario.
    pub fn vertical_nav_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::VerticalNav)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_vertical_nav_ready(&mut self, vertical: bool) {
        if self.vertical_nav_requested() && vertical {
            self.mark_scenario_ready();
        }
    }

    /// Strict Settings evidence waits for the normal telemetry/UI readiness
    /// markers before opening the dialog and focusing its CPU visibility switch.
    pub fn settings_switch_focus_requested(&self) -> bool {
        self.settings_switch_focus_enabled()
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn settings_switch_focus_enabled(&self) -> bool {
        self.scenario == Some(CaptureScenario::SettingsSwitchFocus)
    }

    pub fn mark_settings_switch_focus_ready(&mut self) {
        if self.settings_switch_focus_requested() {
            self.mark_scenario_ready();
        }
    }

    /// Strict Settings evidence for the Apps zero-value preference waits for
    /// real telemetry/list readiness before scrolling to and focusing the new
    /// lower-row control.
    pub fn settings_zero_gray_requested(&self) -> bool {
        self.settings_zero_gray_enabled()
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn settings_zero_gray_enabled(&self) -> bool {
        self.scenario == Some(CaptureScenario::SettingsZeroGray)
    }

    /// Strict Settings evidence scrolls to the central optional-hardware
    /// permission group after the normal live data markers. It never submits a
    /// request or changes a capability; the capture only proves the group is
    /// reachable in the current Settings layout.
    pub fn settings_permission_center_requested(&self) -> bool {
        self.settings_permission_center_enabled()
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn settings_permission_center_enabled(&self) -> bool {
        self.scenario == Some(CaptureScenario::SettingsPermissionCenter)
    }

    pub fn mark_settings_permission_center_ready(&mut self, visible: bool) {
        if self.settings_permission_center_requested() && visible {
            self.mark_scenario_ready();
        }
    }

    /// Strict current-window evidence submits the real GPUI toolbar action
    /// only after live telemetry and list data have reached the root. The
    /// native provider completion, rather than request submission, certifies
    /// this scenario.
    pub(crate) fn window_capture_requested(&self) -> bool {
        self.is_enabled()
            && self.window_capture_chain.active()
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
            && self.window_capture_schedule == WindowCaptureSchedule::AwaitingFrame
    }

    pub(crate) fn window_capture_output(&self) -> Option<PathBuf> {
        self.window_capture_schedule
            .active()
            .then(|| std::env::var_os("TM_CAPTURE_WINDOW_OUTPUT"))
            .flatten()
            .map(PathBuf::from)
    }

    pub(crate) fn mark_window_capture_submitted(&mut self) {
        if self.window_capture_schedule == WindowCaptureSchedule::ReadyToSubmit {
            self.window_capture_schedule = WindowCaptureSchedule::Submitted;
        }
    }

    pub(crate) fn schedule_window_capture_frame(&mut self) -> bool {
        if self.window_capture_requested() {
            self.window_capture_schedule = WindowCaptureSchedule::FrameScheduled;
            true
        } else {
            false
        }
    }

    pub(crate) fn schedule_window_capture_submission(&mut self) -> bool {
        if self.window_capture_schedule == WindowCaptureSchedule::FrameScheduled {
            self.window_capture_schedule = WindowCaptureSchedule::Settling;
            true
        } else {
            false
        }
    }

    pub(crate) fn mark_window_capture_settled(&mut self) {
        if self.window_capture_schedule == WindowCaptureSchedule::Settling {
            self.window_capture_schedule = WindowCaptureSchedule::ReadyToSubmit;
        }
    }

    pub(crate) fn window_capture_settling(&self) -> bool {
        self.window_capture_schedule == WindowCaptureSchedule::Settling
    }

    pub(crate) fn window_capture_submission_requested(&self) -> bool {
        self.window_capture_schedule == WindowCaptureSchedule::ReadyToSubmit
    }

    pub(crate) fn mark_window_capture_failed(&mut self) {
        if self.window_capture_schedule == WindowCaptureSchedule::ReadyToSubmit {
            self.window_capture_schedule = WindowCaptureSchedule::Failed;
        }
    }

    pub(crate) fn mark_window_capture_ready(&mut self) {
        if self.window_capture_chain.active()
            && self.window_capture_schedule == WindowCaptureSchedule::Submitted
            && !self.scenario_ready()
        {
            self.mark_scenario_ready();
        }
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
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::AppsGroupExpanded)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_apps_group_expanded_ready(&mut self, expanded: bool) {
        if self.apps_group_expanded_requested() && expanded {
            self.mark_scenario_ready();
        }
    }

    /// Strict Apps identity evidence prepares a bounded, high-signal matrix
    /// containing PWA, Snap, and AppImage-shaped process facts. The marker is
    /// emitted only after the RootView projection observes all three validated
    /// application assets; the fixture itself never claims those processes are
    /// present on the host.
    pub fn apps_identity_matrix_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::AppsIdentityMatrix)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_apps_identity_matrix_ready(&mut self, ready: bool) {
        if self.apps_identity_matrix_requested() && ready {
            self.mark_scenario_ready();
        }
    }

    /// Strict F9 evidence waits for both live telemetry and a real process-list
    /// update before projecting the hidden Performance device navigator. The
    /// state change remains in RootView; this method only owns the readiness
    /// contract and marker.
    pub fn sidebar_hidden_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::SidebarHidden)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_sidebar_hidden_ready(&mut self, hidden: bool) {
        if self.sidebar_hidden_requested() && hidden {
            self.mark_scenario_ready();
        }
    }

    /// Strict sidebar-edit evidence waits for the normal live snapshot/process
    /// readiness before enabling the concrete-device edit projection.
    pub fn sidebar_edit_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::SidebarEdit)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_sidebar_edit_ready(&mut self, editing: bool) {
        if self.sidebar_edit_requested() && editing {
            self.mark_scenario_ready();
        }
    }

    /// Strict pause evidence waits for live telemetry/list readiness before
    /// projecting the held-Ctrl paused state. The capture path sets only the
    /// transient application policy bit; the real modifier lifecycle is
    /// proven separately by the GPUI behavior test.
    pub fn telemetry_paused_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::TelemetryPaused)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_telemetry_paused_ready(&mut self, paused: bool) {
        if self.telemetry_paused_requested() && paused {
            self.mark_scenario_ready();
        }
    }

    /// Strict System Information evidence waits for the same live telemetry
    /// and process-list readiness as the other modal scenarios. Opening the
    /// modal is a RootView projection; it does not trigger another provider
    /// request.
    pub fn system_about_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::SystemAbout)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_system_about_ready(&mut self, open: bool) {
        if self.system_about_requested() && open {
            self.mark_scenario_ready();
        }
    }

    /// Strict About evidence waits for the same live read-model readiness as
    /// System Information, then opens the independent build-metadata modal.
    /// No host facts or shell action are fabricated by this presentation-only
    /// scenario.
    pub fn about_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::About)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_about_ready(&mut self, open: bool) {
        if self.about_requested() && open {
            self.mark_scenario_ready();
        }
    }

    /// Strict First Run evidence prepares only the dialog projection after the
    /// normal live telemetry/UI readiness gates. It never invokes setup,
    /// pkexec, a helper, or a restart; the resulting marker is therefore a
    /// pixel-fixture receipt, not a functional setup receipt.
    pub fn first_run_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::FirstRun)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn first_run_fixture_enabled(&self) -> bool {
        self.scenario == Some(CaptureScenario::FirstRun)
    }

    pub fn mark_first_run_ready(&mut self, open: bool) {
        if self.first_run_requested() && open {
            self.mark_scenario_ready();
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
            self.mark_scenario_ready();
        }
    }

    pub fn mark_process_batch_ready(&mut self, confirmation_ready: bool, selected_count: usize) {
        if self.scenario == Some(CaptureScenario::ProcessBatchConfirm)
            && confirmation_ready
            && selected_count >= 2
            && !self.scenario_ready()
        {
            self.mark_scenario_ready();
        }
    }

    pub fn mark_service_details_ready(&mut self, details_ready: bool) {
        if self.scenario == Some(CaptureScenario::ServiceDetailsLogs)
            && details_ready
            && !self.scenario_ready()
        {
            self.mark_scenario_ready();
        }
    }

    pub fn mark_process_insights_ready(&mut self, dialog_ready: bool) {
        if self
            .scenario
            .is_some_and(CaptureScenario::is_process_insights)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && dialog_ready
            && !self.scenario_ready()
        {
            self.mark_scenario_ready();
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
            && !self.scenario_ready()
        {
            self.mark_scenario_ready();
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
            && !self.scenario_ready()
        {
            self.mark_scenario_ready();
        }
    }

    /// Boot-markers evidence is ready when the Startup page is active AND the
    /// waterfall evidence plus its comparison baseline are both seeded (the
    /// chips need both boots to compare).
    pub fn mark_startup_boot_markers_ready(&mut self, page_ready: bool, seeded: bool) {
        if self.scenario == Some(CaptureScenario::StartupBootMarkers)
            && page_ready
            && seeded
            && !self.scenario_ready()
        {
            self.mark_scenario_ready();
        }
    }

    /// Strict replay evidence: after the normal readiness markers, open the
    /// Performance page's history-replay panel exactly once. The panel's
    /// data flows through the REAL query over the capture-private history
    /// directory the script pre-seeded with JSONL fixtures — this token only
    /// flips the panel open, it never fabricates rows.
    pub fn history_replay_open_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::HistoryReplay)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
            && self.history_replay_open_state == HistoryReplayOpenState::Closed
    }

    pub fn note_history_replay_opened(&mut self) {
        self.history_replay_open_state = HistoryReplayOpenState::Opened;
    }

    /// Replay readiness is re-checked every tick until the async load lands:
    /// the panel is visible, no load in flight, and rows actually arrived.
    pub fn mark_history_replay_ready(&mut self, loaded: bool) {
        if self.scenario == Some(CaptureScenario::HistoryReplay) && loaded && !self.scenario_ready()
        {
            self.mark_scenario_ready();
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
            && self.telemetry_ready()
            && self.ui_data_ready()
            && page_ready
            && status == taskmanager_application::ApplicationHistoryStatus::Ready
            && row_count > 0
            && !self.scenario_ready()
        {
            self.mark_scenario_ready();
        }
    }

    /// Request the privacy preview only after real telemetry and list readiness.
    /// The caller must prepare the preview and then acknowledge success; this
    /// method never confirms or writes the bundle.
    pub fn diagnostic_preview_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::DiagnosticPreview)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_diagnostic_preview_ready(&mut self, preview_ready: bool) {
        if self.diagnostic_preview_requested() && preview_ready {
            self.mark_scenario_ready();
        }
    }

    /// Request a controlled typed failure only after the normal readiness
    /// markers. The caller assigns UI state directly; no worker is submitted.
    pub fn diagnostic_failure_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::DiagnosticFailure)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
    }

    pub fn mark_diagnostic_failure_ready(&mut self, failure_ready: bool) {
        if self.diagnostic_failure_requested() && failure_ready {
            self.mark_scenario_ready();
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

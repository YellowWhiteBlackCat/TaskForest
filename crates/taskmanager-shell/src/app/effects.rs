//! Effect-queue status display copy and telemetry refresh-policy accessors
//! for the shell (ADR-027).
//!
//! Owns the status-line strings produced when platform effects are queued or
//! complete ([`ShellApp::report_effect_queued`], the submission/outcome
//! reporters), the reducer [`UiEffect`] application
//! ([`ShellApp::apply_ui_effect`]), and the frontend-local telemetry
//! refresh-policy surface ([`ShellApp::paused`],
//! [`ShellApp::telemetry_refresh_due`], the interval accessors, and the
//! Ctrl-held mirror). Collocating the display copy with the policy accessors
//! keeps every frontend's status line and refresh cadence in one audited
//! place.
use super::{
    FeedbackLifecycle, FeedbackSeverity, FeedbackSource, PAGE_STEP, ShellApp, SystemProjectionStore,
};
use taskmanager_application::{
    AppPage, FocusDirection, PlatformEffect, ProcessInsightsSubmissionError, SelectionDirection,
    ServiceUpdate, SessionControlOutcome, StartupControlOutcome, TelemetryInterval,
    TelemetryRefreshPolicyChange, UiEffect,
};
use taskmanager_platform_contract::{EventPortError, OperationFailure, SubmissionError};

impl SystemProjectionStore {
    /// Accept a correlated session-control outcome. Returns the outcome only
    /// when the request id matched a pending submission; the frontend formats
    /// point-of-action feedback from the accepted value.
    pub(super) fn apply_session_control_outcome(
        &mut self,
        outcome: SessionControlOutcome,
    ) -> Option<SessionControlOutcome> {
        if !self.session_control_requests.accept(outcome.request_id) {
            return None;
        }
        self.session_control_feedback = Some(outcome.clone());
        Some(outcome)
    }

    /// Accept a correlated startup-control outcome; returns it only on a
    /// request-id match.
    pub(super) fn apply_startup_control_outcome(
        &mut self,
        outcome: StartupControlOutcome,
    ) -> Option<StartupControlOutcome> {
        if !self.startup_control_requests.accept(outcome.request_id) {
            return None;
        }
        Some(outcome)
    }

    /// Fold one service update. Control outcomes are accepted through the
    /// latest-wins service tracker and returned for frontend feedback; log
    /// snapshots/streams pass through unchanged for the frontend's open-stream
    /// view state; dependency publications are acknowledged but not retained.
    pub(super) fn apply_service_update(&mut self, update: ServiceUpdate) -> Option<ServiceUpdate> {
        match update {
            ServiceUpdate::Action(outcome) => {
                if !self.service_control_requests.accept(
                    outcome.request_id,
                    &outcome.service_id,
                    outcome.action,
                ) {
                    return None;
                }
                Some(ServiceUpdate::Action(outcome))
            }
            ServiceUpdate::Logs(_)
            | ServiceUpdate::LogStream { .. }
            | ServiceUpdate::Dependencies { .. }
            | ServiceUpdate::DependenciesUnavailable { .. } => Some(update),
        }
    }
}

impl ShellApp {
    #[must_use]
    pub const fn paused(&self) -> bool {
        self.telemetry_refresh_policy.is_paused()
    }

    #[must_use]
    pub fn telemetry_refresh_due(&self, elapsed_since_last: std::time::Duration) -> bool {
        self.telemetry_refresh_policy
            .should_submit(Some(elapsed_since_last))
    }

    pub fn report_submission_error(&mut self, error: &SubmissionError) {
        self.report_notice(
            FeedbackSource::Platform,
            FeedbackSeverity::Error,
            FeedbackLifecycle::UntilReplaced,
            format!("Capability {}: {:?}", error.capability, error.kind),
        );
    }

    pub fn report_operation_failure(&mut self, error: &OperationFailure) {
        self.report_notice(
            FeedbackSource::Platform,
            FeedbackSeverity::Error,
            FeedbackLifecycle::UntilReplaced,
            format!("Capability {}: {:?}", error.capability, error.kind),
        );
    }

    pub fn report_event_port_error(&mut self, error: EventPortError) {
        self.report_notice(
            FeedbackSource::Platform,
            FeedbackSeverity::Error,
            FeedbackLifecycle::UntilReplaced,
            format!("Platform runtime: {error:?}"),
        );
    }

    /// The shell-side submission-error report for the whole-facet insight
    /// request (identity missing / revision exhausted) — distinct from the
    /// per-facet queue errors the shared `queue_effect` loop reports.
    pub fn report_process_insights_submission_error(
        &mut self,
        error: ProcessInsightsSubmissionError,
    ) {
        self.report_notice(
            FeedbackSource::Platform,
            FeedbackSeverity::Error,
            FeedbackLifecycle::UntilReplaced,
            format!("Process insights unavailable: {error:?}"),
        );
    }

    /// Mirror the live keyboard Ctrl state into the telemetry refresh policy.
    /// While Ctrl is held the policy pauses telemetry refresh (via
    /// [`taskmanager_application::TelemetryRefreshPolicy::is_paused`]) so the user can inspect a frozen
    /// frame — mirrors GPUI's hold-Ctrl pause. Called by frontends from their
    /// modifier-changed event; no-op when the state is unchanged.
    #[must_use]
    pub const fn control_held(&self) -> bool {
        self.telemetry_refresh_policy.is_control_held()
    }

    pub fn set_control_held(&mut self, held: bool) {
        if self.telemetry_refresh_policy.is_control_held() == held {
            return;
        }
        self.telemetry_refresh_policy
            .apply(TelemetryRefreshPolicyChange::SetControlHeld(held));
    }

    /// Set the automatic telemetry refresh interval (mirrors GPUI's Settings
    /// refresh slider, which drives `TelemetryRefreshPolicy`). The frontend
    /// persists the chosen milliseconds through its own config store; the
    /// policy keeps the clamped duration as the single cadence authority.
    pub fn set_telemetry_interval(&mut self, interval: TelemetryInterval) {
        self.telemetry_refresh_policy
            .apply(TelemetryRefreshPolicyChange::SetInterval(interval));
    }

    /// The currently configured telemetry interval (the Settings refresh
    /// chooser reads it back so the pill matches the effective cadence even
    /// when a persisted value was clamped).
    #[must_use]
    pub fn telemetry_interval(&self) -> TelemetryInterval {
        self.telemetry_refresh_policy.interval()
    }

    pub fn report_effect_queued(&mut self, effect: &PlatformEffect) {
        let text = match effect {
            PlatformEffect::Refresh(_) => "Refresh queued".into(),
            PlatformEffect::EndTask(target) => {
                format!("End task queued for {} ({})", target.name, target.pid)
            }
            PlatformEffect::ProcessSignal { target, signal } => {
                format!(
                    "Signal {signal:?} queued for {} ({})",
                    target.name, target.pid
                )
            }
            PlatformEffect::ExecuteBatch(intent) => {
                format!("Process batch {:?} queued", intent.action)
            }
            PlatformEffect::ServiceControl(target) => {
                format!("Service {:?} queued", target.action)
            }
            PlatformEffect::SessionControl(target) => {
                format!("Session {} {:?} queued", target.session_id, target.action)
            }
            PlatformEffect::StartupControl(request) => format!(
                "Startup {} queued",
                if request.enabled { "enable" } else { "disable" }
            ),
            PlatformEffect::RevealResource(request) => {
                format!("Opening file location for {}", request.target.name)
            }
            PlatformEffect::OpenUrl(request) => format!("Opening {}", request.url),
            PlatformEffect::ProcessInsights(target) => {
                format!(
                    "Process insights queued for {} ({})",
                    target.name, target.pid
                )
            }
            PlatformEffect::ProcessNetworkEscalation => {
                "Per-process network capture escalation queued".into()
            }
            PlatformEffect::ServiceLogStream(_) => "Service log stream queued".into(),
            PlatformEffect::DesktopNotification(request) => {
                format!("Desktop notification queued: {}", request.title)
            }
            PlatformEffect::DirectoryUsage(request) => match request {
                taskmanager_application::DirectoryUsageRequest::StartScan(spec) => {
                    format!("Directory usage scan queued for {}", spec.root)
                }
                taskmanager_application::DirectoryUsageRequest::Cancel(scan_id) => {
                    format!("Directory usage scan {} cancel queued", scan_id.get())
                }
            },
            PlatformEffect::GpuEngineRows(request) => {
                format!("GPU engine rows queued for {}", request.device_id.as_str())
            }
            PlatformEffect::NpuInventory(_) => "NPU accelerator inventory queued".into(),
            PlatformEffect::SmartControl(request) => match request {
                taskmanager_application::SmartControlRequest::StartSelfTest(_) => {
                    "SMART self-test queued".into()
                }
                taskmanager_application::SmartControlRequest::StopTracking(_) => {
                    "SMART tracking stop queued".into()
                }
            },
            PlatformEffect::ServiceDependencies(request) => format!(
                "Service dependencies queued for {}",
                request.service_id.as_str()
            ),
            PlatformEffect::ServiceLogSnapshot(request) => format!(
                "Service log snapshot queued for {}",
                request.service_id.as_str()
            ),
            PlatformEffect::ProcessAffinity(request) => format!(
                "Process affinity read queued for {} ({})",
                request.target.name, request.target.pid
            ),
            PlatformEffect::ProcessAffinityControl(request) => format!(
                "Process affinity set queued for {} ({})",
                request.target.name, request.target.pid
            ),
            PlatformEffect::CommandLaunch(request) => {
                format!("Command launch queued: {}", request.command)
            }
            PlatformEffect::SetupScript(request) => {
                format!("Setup script {:?} queued", request.action)
            }
            PlatformEffect::ResourceGroupControl(request) => format!(
                "Process resource limits queued for {} ({})",
                request.target.name, request.target.pid
            ),
        };
        self.report_notice(
            FeedbackSource::Control,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            text,
        );
    }

    /// Apply one reducer-emitted [`UiEffect`] to the shell state (moved here
    /// from the parent module so `app.rs` stays under the source-line
    /// ceiling; behavior unchanged).
    pub(super) fn apply_ui_effect(&mut self, effect: UiEffect) {
        match effect {
            UiEffect::FocusSearch => {
                // Ctrl+F focuses the process search field, which lives on the
                // Applications page — switch there first so the field is
                // visible (mirrors GPUI; applies to every frontend via the
                // shared reducer).
                self.application.active_page = AppPage::Applications;
                self.open_search();
            }
            UiEffect::MoveFocus(FocusDirection::Next | FocusDirection::Previous) => {
                self.toggle_search_focus();
            }
            UiEffect::MoveSelection(SelectionDirection::PageUp) => {
                self.move_selection(-(PAGE_STEP as isize));
            }
            UiEffect::MoveSelection(SelectionDirection::PageDown) => {
                self.move_selection(PAGE_STEP as isize);
            }
            UiEffect::MoveSelection(SelectionDirection::Previous) => {
                self.move_selection(-1);
            }
            UiEffect::MoveSelection(SelectionDirection::Next) => {
                self.move_selection(1);
            }
            UiEffect::MoveSelection(SelectionDirection::First) => {
                self.move_selection_to_first();
            }
            UiEffect::MoveSelection(SelectionDirection::Last) => {
                self.move_selection_to_last();
            }
            UiEffect::PageChanged(_) => {
                self.selected = 0;
                self.reset_input_mode();
                self.sync_application_selection();
            }
            UiEffect::ShowSystemAbout => {
                self.report_notice(
                    FeedbackSource::Navigation,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    "System information is available on the graphical System page",
                );
            }
            UiEffect::ToggleTelemetryPause => {
                let paused = !self.telemetry_refresh_policy.is_paused();
                self.telemetry_refresh_policy
                    .apply(TelemetryRefreshPolicyChange::SetPaused(paused));
                let text = if paused {
                    "Telemetry updates paused"
                } else {
                    "Telemetry updates resumed"
                };
                self.report_notice(
                    FeedbackSource::Navigation,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    text,
                );
            }
            // The TUI's Performance page is a single terminal layout and has
            // no GPUI-style device navigator. Keep the shared effect honest by
            // treating this frontend-local projection as inert rather than
            // inventing a second sidebar state.
            UiEffect::ToggleSidebar => {}
        }
    }
}

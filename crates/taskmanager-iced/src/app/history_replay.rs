//! Iced adapter for the application history-replay lifecycle.
//!
//! The session lifecycle (which history session is live, which window it
//! replays, whether a request was accepted) belongs to
//! [`taskmanager_app_host::HistoryFrontendRuntime`]. This module owns only
//! the Iced surface state: whether the replay panel is open and the
//! request-keyed row cache the renderer draws from.

use std::rc::Rc;

use taskforest_core::core::history::{HistorySeriesKey, HistoryWindow};
use taskmanager_app_host::{
    HistoryFrontendConnectOutcome, HistoryFrontendRuntime, HistoryReplayClient,
};
use taskmanager_application::{
    ApplicationHistoryProjection, HistoryReplayError, HistoryReplayRequestId,
};

use super::IcedApp;
use taskmanager_app_host::HistoryFrontendConnector;
use taskmanager_app_host::HistoryFrontendConnectorStartError;
use taskmanager_application::AppPage;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryReplayRow {
    pub(crate) key: HistorySeriesKey,
    pub(crate) samples: Rc<[f32]>,
    pub(crate) peak_value: Option<f64>,
    pub(crate) peak_measured_at_ms: Option<u64>,
    pub(crate) observed: usize,
    pub(crate) gaps: usize,
    pub(crate) clock_jumps: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum IcedHistoryPresentation {
    #[default]
    Live,
    Replay,
}

#[derive(Debug, Default)]
pub(crate) struct IcedHistoryReplay {
    runtime: HistoryFrontendRuntime,
    boot_candidate: Option<HistoryReplayClient>,
    projected_request: Option<HistoryReplayRequestId>,
    rows: Vec<HistoryReplayRow>,
    presentation: IcedHistoryPresentation,
}

impl IcedHistoryReplay {
    pub(crate) fn new(candidate: Option<HistoryReplayClient>) -> Self {
        Self {
            runtime: HistoryFrontendRuntime::default(),
            boot_candidate: candidate,
            projected_request: None,
            rows: Vec::new(),
            presentation: IcedHistoryPresentation::Live,
        }
    }

    pub(crate) const fn is_open(&self) -> bool {
        matches!(self.presentation, IcedHistoryPresentation::Replay)
    }

    pub(crate) const fn is_loading(&self) -> bool {
        self.runtime.controller().is_loading()
    }

    pub(crate) const fn window(&self) -> HistoryWindow {
        self.runtime.window()
    }

    pub(crate) fn rows(&self) -> &[HistoryReplayRow] {
        &self.rows
    }

    pub(crate) fn failure(&self) -> Option<&HistoryReplayError> {
        self.runtime.controller().failure()
    }

    pub(crate) fn rows_window(&self) -> Option<HistoryWindow> {
        self.runtime.controller().rows_window()
    }

    pub(crate) fn loaded_at_ms(&self) -> Option<u64> {
        self.runtime.controller().loaded_at_ms()
    }

    fn close(&mut self) {
        self.presentation = IcedHistoryPresentation::Live;
    }

    /// Resolve the boot state before the native connector is installed.
    ///
    /// A frontend that already owns a replay client adopts it as the live
    /// session when history is requested; without one the runtime stays
    /// honestly unavailable until the native connector arrives.
    fn resolve_boot(&mut self, requested: bool) {
        if requested
            && !self.runtime.connector_installed()
            && let Some(candidate) = self.boot_candidate.take()
        {
            self.runtime.adopt_replay_client(candidate);
        }
        self.runtime.request(requested);
    }

    fn sync_rows_projection(&mut self) {
        let request = self.runtime.controller().rows_request_id();
        if request == self.projected_request {
            return;
        }
        let projection = self.runtime.performance_history_projection();
        self.rows = projection
            .rows
            .iter()
            .map(|row| HistoryReplayRow {
                key: row.key.clone(),
                samples: Rc::from(row.gap_aware_samples().as_ref()),
                peak_value: row.peak_value,
                peak_measured_at_ms: row.peak_measured_at_ms,
                observed: row.observed,
                gaps: row.gaps,
                clock_jumps: row.clock_jumps,
            })
            .collect();
        self.projected_request = request;
    }

    fn application_history_projection(&self) -> ApplicationHistoryProjection {
        self.runtime.application_history_projection()
    }
}

impl IcedApp {
    pub(crate) fn install_history_frontend_connector(
        &mut self,
        connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        self.history_runtime.runtime.install_connector(connector);
        self.sync_history_persistence_sink();
    }

    pub(crate) fn request_history_frontend(&mut self, enabled: bool) {
        self.history_runtime.runtime.request(enabled);
        if !enabled {
            self.history_runtime.close();
            self.history_runtime.sync_rows_projection();
        }
        self.sync_history_persistence_sink();
    }

    pub(crate) fn history_replay_entry_available(&self) -> bool {
        self.history_runtime.runtime.is_active()
    }

    /// Whether persistence was requested for this run but the replay runtime
    /// could not start — the startup-unavailable notice state (GPUI
    /// `history_replay_startup_unavailable` parity). A run that never
    /// requested persistence shows nothing, never a dead notice.
    pub(crate) fn history_replay_startup_unavailable(&self) -> bool {
        self.history_runtime.runtime.startup_unavailable()
    }

    pub(crate) fn activate_history_replay_for_boot(&mut self) {
        let requested = self.configuration.draft().history_persistence;
        self.history_runtime.resolve_boot(requested);
        self.sync_history_persistence_sink();
        if self.history_replay_entry_available() {
            self.history_runtime.runtime.open_replay();
            self.history_runtime.sync_rows_projection();
        }
    }

    pub(super) fn toggle_history_replay(&mut self) {
        if self.history_runtime.is_open() {
            self.history_runtime.close();
        } else if self.history_replay_entry_available() {
            self.history_runtime.presentation = IcedHistoryPresentation::Replay;
            self.history_runtime.runtime.open_replay();
            self.history_runtime.sync_rows_projection();
        }
    }

    pub(super) fn select_history_replay_window(&mut self, window: HistoryWindow) {
        self.history_runtime.runtime.select_window(window);
        self.history_runtime.sync_rows_projection();
    }

    pub(super) fn refresh_history_replay(&mut self) {
        self.history_runtime.runtime.refresh_replay();
        self.history_runtime.sync_rows_projection();
    }

    pub(super) fn drain_history_replay_completions(&mut self) {
        let connector_changed =
            self.history_runtime.runtime.drain_connector() != HistoryFrontendConnectOutcome::Idle;
        if connector_changed {
            self.sync_history_persistence_sink();
            self.history_runtime.sync_rows_projection();
        }
        if self.is_demo()
            && crate::capture::persisted_history_requested()
            && self.shell.page() == AppPage::Performance
            && self.history_replay_entry_available()
            && !self.history_runtime.is_open()
        {
            self.toggle_history_replay();
        }

        if self.history_runtime.runtime.drain_replay() {
            self.history_runtime.sync_rows_projection();
        }
    }

    pub(crate) const fn history_replay_state(&self) -> &IcedHistoryReplay {
        &self.history_runtime
    }

    pub(crate) fn application_history_projection(&self) -> ApplicationHistoryProjection {
        self.history_runtime.application_history_projection()
    }

    fn sync_history_persistence_sink(&mut self) {
        self.shell
            .set_history_persistence_sink(self.history_runtime.runtime.record_sink());
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/history_replay_tests.rs"]
mod tests;

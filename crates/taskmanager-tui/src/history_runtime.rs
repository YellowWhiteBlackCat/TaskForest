//! Read-only persistent-history adapter for the terminal frontend.
//!
//! The session lifecycle (which history session is live, which window it
//! replays, whether a request was accepted) belongs to
//! [`taskmanager_app_host::HistoryFrontendRuntime`]. This module owns only
//! the terminal's own replay surface state: whether the replay panel is open
//! and how far its rows are scrolled.

use taskforest_core::core::history::HistoryWindow;
use taskmanager_app_host::{
    HistoryFrontendConnector, HistoryFrontendConnectorStartError, HistoryFrontendRuntime,
};
use taskmanager_application::{
    ApplicationHistoryProjection, ApplicationHistoryUnavailableReason, PerformanceHistoryProjection,
};

use crate::TuiApp;
use taskmanager_application::AppPage;

#[derive(Default)]
enum PerformancePresentation {
    #[default]
    Live,
    Replay {
        row_scroll: usize,
    },
}

pub(crate) struct TuiHistoryRuntime {
    runtime: HistoryFrontendRuntime,
    performance: PerformancePresentation,
}

impl Default for TuiHistoryRuntime {
    fn default() -> Self {
        Self {
            runtime: HistoryFrontendRuntime::default(),
            performance: PerformancePresentation::Live,
        }
    }
}

impl TuiHistoryRuntime {
    pub(crate) fn install_connector(
        &mut self,
        connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        self.runtime.install_connector(connector);
    }

    pub(crate) fn request(&mut self, enabled: bool) {
        self.runtime.request(enabled);
        if !enabled {
            self.performance = PerformancePresentation::Live;
        }
    }

    pub(crate) fn projection(&self) -> ApplicationHistoryProjection {
        self.runtime.application_history_projection()
    }

    pub(crate) const fn is_open(&self) -> bool {
        matches!(self.performance, PerformancePresentation::Replay { .. })
    }

    pub(crate) fn available(&self) -> bool {
        self.runtime.is_active()
    }

    pub(crate) fn performance_projection(&self) -> PerformanceHistoryProjection {
        self.runtime.performance_history_projection()
    }

    pub(crate) fn open(&mut self) -> bool {
        if !self.available() {
            return false;
        }
        self.performance = PerformancePresentation::Replay { row_scroll: 0 };
        self.runtime.open_replay();
        true
    }

    pub(crate) fn close(&mut self) {
        self.performance = PerformancePresentation::Live;
    }

    pub(crate) fn refresh(&mut self) {
        self.runtime.refresh_replay();
    }

    pub(crate) fn row_scroll(&self) -> usize {
        match self.performance {
            PerformancePresentation::Replay { row_scroll } => row_scroll,
            PerformancePresentation::Live => 0,
        }
    }

    pub(crate) fn scroll_rows(&mut self, delta: isize) {
        let maximum = self.performance_projection().rows.len().saturating_sub(1);
        if let PerformancePresentation::Replay { row_scroll } = &mut self.performance {
            *row_scroll = row_scroll.saturating_add_signed(delta).min(maximum);
        }
    }

    pub(crate) const fn window(&self) -> HistoryWindow {
        self.runtime.window()
    }

    pub(crate) fn select_window(&mut self, window: HistoryWindow) -> bool {
        self.runtime.select_window(window)
    }

    /// Whether the history runtime's state advanced since the last drain.
    ///
    /// The answer belongs to the runtime owner
    /// ([`taskmanager_app_host::HistoryFrontendRuntime::drain_changes`]): the
    /// connector's outcome and the replay's completions are one question, not
    /// two answers each frontend has to combine.
    pub(crate) fn drain(&mut self) -> bool {
        self.runtime.drain_changes()
    }

    pub(crate) fn unavailable_reason(&self) -> Option<ApplicationHistoryUnavailableReason> {
        self.runtime.unavailable_reason()
    }

    pub(crate) fn record_sink(
        &self,
    ) -> Option<std::sync::Arc<dyn taskforest_core::core::history::HistoryRecordSink>> {
        self.runtime.record_sink()
    }
}

impl TuiApp {
    pub(crate) fn install_history_frontend_connector(
        &mut self,
        connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        self.history_runtime.install_connector(connector);
        self.sync_history_persistence_sink();
    }

    pub(crate) fn application_history_projection(&self) -> ApplicationHistoryProjection {
        self.history_runtime.projection()
    }

    pub(crate) fn application_history_unavailable_reason(
        &self,
    ) -> Option<ApplicationHistoryUnavailableReason> {
        self.history_runtime.unavailable_reason()
    }

    pub(crate) fn select_application_history_window(&mut self, window: HistoryWindow) -> bool {
        self.history_runtime.select_window(window)
    }

    pub fn history_replay_open(&self) -> bool {
        self.history_runtime.is_open()
    }

    pub fn open_history_replay(&mut self) {
        let _ = self.history_runtime.open();
    }

    pub(crate) fn history_replay_available(&self) -> bool {
        self.history_runtime.available()
    }
    pub(crate) fn performance_history_projection(&self) -> PerformanceHistoryProjection {
        self.history_runtime.performance_projection()
    }
    pub(crate) fn refresh_history_replay(&mut self) {
        self.history_runtime.refresh();
    }
    pub(crate) fn history_replay_row_scroll(&self) -> usize {
        self.history_runtime.row_scroll()
    }
    pub(crate) fn scroll_history_replay_rows(&mut self, delta: isize) {
        self.history_runtime.scroll_rows(delta);
    }
    pub(crate) fn toggle_history_replay(&mut self) {
        if self.history_replay_open() {
            self.close_history_replay();
        } else {
            self.open_history_replay();
        }
    }

    pub fn close_history_replay(&mut self) {
        self.history_runtime.close();
    }

    pub fn select_history_replay_window(&mut self, window: HistoryWindow) -> bool {
        self.history_runtime.select_window(window)
    }

    pub fn history_replay_window(&self) -> HistoryWindow {
        self.history_runtime.window()
    }

    pub(crate) fn drain_history_replay_completions(&mut self) -> bool {
        let changed = self.history_runtime.drain();
        if changed {
            self.sync_history_persistence_sink();
            if self.history_replay_available()
                && crate::demo::persisted_history_capture_requested()
                && self.page() == AppPage::Performance
                && !self.history_replay_open()
            {
                self.open_history_replay();
            }
        }
        changed
    }

    pub(crate) fn request_history_frontend(&mut self, enabled: bool) {
        self.history_runtime.request(enabled);
        self.sync_history_persistence_sink();
    }

    fn sync_history_persistence_sink(&mut self) {
        self.shell
            .set_history_persistence_sink(self.history_runtime.record_sink());
    }
}

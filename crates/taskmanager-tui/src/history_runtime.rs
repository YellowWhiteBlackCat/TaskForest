//! Read-only persistent-history adapter for the terminal frontend.

use taskmanager_app_host::{
    HistoryFrontendConnectRequestId, HistoryFrontendConnector, HistoryFrontendConnectorStartError,
};
use taskmanager_application::{
    ApplicationHistoryCapability, ApplicationHistoryProjection,
    ApplicationHistoryUnavailableReason, HistoryReplayCompletionDisposition,
    HistoryReplayController, HistoryReplayRequest, PerformanceHistoryProjection,
};
use taskmanager_core::core::history::HistoryWindow;

use crate::TuiApp;
use taskmanager_app_host::HistoryFrontendSession;
use taskmanager_application::AppPage;
use taskmanager_core::core::history::HistoryRecordSink;

enum HistoryResources {
    Disabled,
    Connecting(HistoryFrontendConnectRequestId),
    Unavailable(ApplicationHistoryUnavailableReason),
    Active(HistoryFrontendSession),
}

#[derive(Default)]
enum PerformancePresentation {
    #[default]
    Live,
    Replay {
        row_scroll: usize,
    },
}

pub(crate) struct TuiHistoryRuntime {
    resources: HistoryResources,
    controller: HistoryReplayController,
    connector: Option<HistoryFrontendConnector>,
    requested: bool,
    performance: PerformancePresentation,
}

impl Default for TuiHistoryRuntime {
    fn default() -> Self {
        Self {
            resources: HistoryResources::Disabled,
            controller: HistoryReplayController::default(),
            connector: None,
            requested: false,
            performance: PerformancePresentation::Live,
        }
    }
}

impl TuiHistoryRuntime {
    pub(crate) fn install_connector(
        &mut self,
        result: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        match result {
            Ok(connector) => {
                self.connector = Some(connector);
                self.request(self.requested);
            }
            Err(error) if self.requested => {
                self.resources = HistoryResources::Unavailable(error.into())
            }
            Err(_) => self.resources = HistoryResources::Disabled,
        }
    }

    pub(crate) fn request(&mut self, enabled: bool) {
        self.requested = enabled;
        if !enabled {
            self.resources = HistoryResources::Disabled;
            self.controller.close();
            self.performance = PerformancePresentation::Live;
            return;
        }
        if matches!(
            self.resources,
            HistoryResources::Connecting(_) | HistoryResources::Active(_)
        ) {
            return;
        }
        let Some(connector) = self.connector.as_mut() else {
            self.resources = HistoryResources::Unavailable(
                ApplicationHistoryUnavailableReason::ConnectorStopped,
            );
            return;
        };
        self.resources = match connector.try_connect() {
            Ok(request) => HistoryResources::Connecting(request),
            Err(error) => HistoryResources::Unavailable(error.into()),
        };
    }

    pub(crate) fn projection(&self) -> ApplicationHistoryProjection {
        self.controller
            .application_history_projection(self.capability())
    }

    pub(crate) const fn is_open(&self) -> bool {
        matches!(self.performance, PerformancePresentation::Replay { .. })
    }

    pub(crate) fn available(&self) -> bool {
        matches!(self.resources, HistoryResources::Active(_))
    }

    pub(crate) fn performance_projection(&self) -> PerformanceHistoryProjection {
        self.controller
            .performance_history_projection(self.capability())
    }

    pub(crate) fn open(&mut self) -> bool {
        if !self.available() {
            return false;
        }
        self.performance = PerformancePresentation::Replay { row_scroll: 0 };
        if !self.controller.is_open() {
            let request = self.controller.open().ok();
            self.submit_new_request(request);
        }
        true
    }

    pub(crate) fn close(&mut self) {
        self.performance = PerformancePresentation::Live;
    }

    pub(crate) fn refresh(&mut self) {
        if self.available() {
            let request = self.controller.refresh().ok();
            self.submit_new_request(request);
        }
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
        self.controller.selected_window()
    }

    pub(crate) fn select_window(&mut self, window: HistoryWindow) -> bool {
        let request = self.controller.select_window(window).ok();
        let changed = request.is_some();
        self.submit_new_request(request);
        changed
    }

    pub(crate) fn drain(&mut self) -> bool {
        let mut changed = false;
        if let Some(connector) = self.connector.as_mut() {
            for completion in connector.drain() {
                let HistoryResources::Connecting(current) = self.resources else {
                    continue;
                };
                if completion.request != current || !self.requested {
                    continue;
                }
                self.resources = match completion.result {
                    Ok(session) => HistoryResources::Active(session),
                    Err(error) => HistoryResources::Unavailable(error.kind().into()),
                };
                if matches!(self.resources, HistoryResources::Active(_)) {
                    let request = self.controller.open().ok();
                    self.submit_new_request(request);
                } else {
                    self.controller.close();
                }
                changed = true;
            }
        }
        if let HistoryResources::Active(client) = &mut self.resources {
            let completions = client.replay.drain();
            for completion in completions {
                changed |= self.controller.complete(completion)
                    == HistoryReplayCompletionDisposition::Applied;
            }
        }
        changed
    }

    fn capability(&self) -> ApplicationHistoryCapability {
        match self.resources {
            HistoryResources::Disabled => ApplicationHistoryCapability::Disabled,
            HistoryResources::Connecting(_) => ApplicationHistoryCapability::Connecting,
            HistoryResources::Unavailable(reason) => {
                ApplicationHistoryCapability::Unavailable(reason)
            }
            HistoryResources::Active(_) => ApplicationHistoryCapability::Available,
        }
    }

    fn submit_new_request(&mut self, request: Option<HistoryReplayRequest>) {
        let Some(request) = request else {
            return;
        };
        let error = match &mut self.resources {
            HistoryResources::Active(session) => session.replay.try_request(request).err(),
            HistoryResources::Disabled
            | HistoryResources::Connecting(_)
            | HistoryResources::Unavailable(_) => None,
        };
        if let Some(error) = error {
            let _ = self.controller.reject_submission(request, error);
        }
    }

    pub(crate) fn unavailable_reason(&self) -> Option<ApplicationHistoryUnavailableReason> {
        match self.resources {
            HistoryResources::Unavailable(reason) => Some(reason),
            HistoryResources::Disabled
            | HistoryResources::Connecting(_)
            | HistoryResources::Active(_) => None,
        }
    }

    pub(crate) fn record_sink(&self) -> Option<std::sync::Arc<dyn HistoryRecordSink>> {
        match &self.resources {
            HistoryResources::Active(session) => Some(session.persistence.record_sink.clone()),
            HistoryResources::Disabled
            | HistoryResources::Connecting(_)
            | HistoryResources::Unavailable(_) => None,
        }
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

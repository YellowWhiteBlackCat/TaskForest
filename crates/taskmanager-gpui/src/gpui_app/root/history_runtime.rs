//! Typed non-blocking persistent-history resources owned by one GPUI root.
//!
//! The session lifecycle (connect handshake, session state, writer/replay
//! capabilities) is owned by [`taskmanager_app_host::HistoryFrontendRuntime`].
//! This module only manages GPUI-specific presentation state (the replay toggle
//! and performance presentation).

use crate::gpui_app::perf_views::history_replay::HistoryReplayState;
use taskforest_core::core::history::HistoryRecordSink;
use taskmanager_app_host::{
    HistoryFrontendConnectOutcome, HistoryFrontendConnector, HistoryFrontendConnectorStartError,
    HistoryFrontendRuntime, HistoryReplayClient,
};
use taskmanager_application::ApplicationHistoryCapability;
use taskmanager_application::ApplicationHistoryUnavailableReason;
use taskmanager_application::HistoryReplayRequest;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PerformanceHistoryPresentation {
    #[default]
    Live,
    Replay,
}

pub(in crate::gpui_app) struct HistoryRuntimeState {
    runtime: HistoryFrontendRuntime,
    replay: HistoryReplayState,
    performance_presentation: PerformanceHistoryPresentation,
}

impl Default for HistoryRuntimeState {
    fn default() -> Self {
        Self {
            runtime: HistoryFrontendRuntime::default(),
            replay: HistoryReplayState::new(),
            performance_presentation: PerformanceHistoryPresentation::Live,
        }
    }
}

impl HistoryRuntimeState {
    pub(in crate::gpui_app) const fn enabled_next_start(&self) -> bool {
        self.runtime.requested()
    }

    pub(in crate::gpui_app) fn install_connector(
        &mut self,
        connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        self.runtime.install_connector(connector);
    }

    pub(in crate::gpui_app) fn request(&mut self, enabled: bool) {
        self.runtime.request(enabled);
        if !enabled {
            self.replay.close();
            self.performance_presentation = PerformanceHistoryPresentation::Live;
        }
    }

    pub(in crate::gpui_app) fn drain_connector(&mut self) -> bool {
        match self.runtime.drain_connector() {
            HistoryFrontendConnectOutcome::Activated => {
                if !self.replay.is_open() {
                    let request = self.replay.open();
                    self.submit_replay(request);
                }
                true
            }
            HistoryFrontendConnectOutcome::Failed(_) => {
                self.replay.close();
                self.performance_presentation = PerformanceHistoryPresentation::Live;
                true
            }
            HistoryFrontendConnectOutcome::Idle => false,
        }
    }

    pub(in crate::gpui_app) const fn replay(&self) -> &HistoryReplayState {
        &self.replay
    }

    pub(in crate::gpui_app) fn replay_mut(&mut self) -> &mut HistoryReplayState {
        &mut self.replay
    }

    pub(in crate::gpui_app) fn replay_available(&self) -> bool {
        self.runtime.is_active()
    }

    pub(in crate::gpui_app) const fn performance_replay_visible(&self) -> bool {
        matches!(
            self.performance_presentation,
            PerformanceHistoryPresentation::Replay
        )
    }

    pub(in crate::gpui_app) fn toggle_performance_presentation(&mut self) {
        self.performance_presentation = match self.performance_presentation {
            PerformanceHistoryPresentation::Live => PerformanceHistoryPresentation::Replay,
            PerformanceHistoryPresentation::Replay => PerformanceHistoryPresentation::Live,
        };
    }

    pub(in crate::gpui_app) fn unavailable_reason(
        &self,
    ) -> Option<ApplicationHistoryUnavailableReason> {
        self.runtime.unavailable_reason()
    }

    pub(in crate::gpui_app) fn application_history_capability(
        &self,
    ) -> ApplicationHistoryCapability {
        self.runtime.capability()
    }

    pub(in crate::gpui_app) fn replay_client_mut(&mut self) -> Option<&mut HistoryReplayClient> {
        self.runtime.replay_client_mut()
    }

    pub(in crate::gpui_app) fn record_sink(&self) -> Option<std::sync::Arc<dyn HistoryRecordSink>> {
        self.runtime.record_sink()
    }

    fn submit_replay(&mut self, request: Option<HistoryReplayRequest>) {
        let Some(request) = request else {
            return;
        };
        let error = self
            .replay_client_mut()
            .and_then(|client| client.try_request(request).err());
        if let Some(error) = error {
            self.replay.reject_submission(request, error);
        }
    }
}

impl super::RootView {
    pub(in crate::gpui_app) fn set_history_persistence(
        &mut self,
        enabled: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.history_runtime.request(enabled);
        self.sync_history_persistence_sink();
        cx.notify();
    }

    pub(in crate::gpui_app) fn sync_history_persistence_sink(&mut self) {
        let sink = self.history_runtime.record_sink();
        self.telemetry_ingestor = match sink.clone() {
            Some(sink) => self.telemetry_ingestor.clone().with_record_sink(sink),
            None => self.telemetry_ingestor.clone().without_record_sink(),
        };
        self.shell.set_history_persistence_sink(sink);
    }
}

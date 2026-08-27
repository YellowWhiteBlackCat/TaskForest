//! Typed non-blocking persistent-history resources owned by one GPUI root.

use crate::gpui_app::perf_views::history_replay::HistoryReplayState;
use taskmanager_app_host::{
    HistoryFrontendConnectRequestId, HistoryFrontendConnector, HistoryFrontendConnectorStartError,
    HistoryFrontendSession, HistoryReplayClient,
};

enum HistoryRuntimeResources {
    Disabled,
    Connecting(HistoryFrontendConnectRequestId),
    Unavailable(taskmanager_application::ApplicationHistoryUnavailableReason),
    Active(HistoryFrontendSession),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PerformanceHistoryPresentation {
    #[default]
    Live,
    Replay,
}

pub(in crate::gpui_app) struct HistoryRuntimeState {
    requested: bool,
    connector: Option<HistoryFrontendConnector>,
    resources: HistoryRuntimeResources,
    replay: HistoryReplayState,
    performance_presentation: PerformanceHistoryPresentation,
}

impl Default for HistoryRuntimeState {
    fn default() -> Self {
        Self {
            requested: false,
            connector: None,
            resources: HistoryRuntimeResources::Disabled,
            replay: HistoryReplayState::new(),
            performance_presentation: PerformanceHistoryPresentation::Live,
        }
    }
}

impl HistoryRuntimeState {
    pub(in crate::gpui_app) const fn enabled_next_start(&self) -> bool {
        self.requested
    }

    pub(in crate::gpui_app) fn install_connector(
        &mut self,
        connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        match connector {
            Ok(connector) => {
                self.connector = Some(connector);
                self.request(self.requested);
            }
            Err(error) if self.requested => {
                self.resources = HistoryRuntimeResources::Unavailable(error.into())
            }
            Err(_) => self.resources = HistoryRuntimeResources::Disabled,
        }
    }

    pub(in crate::gpui_app) fn request(&mut self, enabled: bool) {
        self.requested = enabled;
        if !enabled {
            self.resources = HistoryRuntimeResources::Disabled;
            self.replay.close();
            self.performance_presentation = PerformanceHistoryPresentation::Live;
            return;
        }
        let Some(connector) = self.connector.as_mut() else {
            self.resources = HistoryRuntimeResources::Unavailable(
                taskmanager_application::ApplicationHistoryUnavailableReason::ConnectorStopped,
            );
            return;
        };
        self.resources = match connector.try_connect() {
            Ok(request) => HistoryRuntimeResources::Connecting(request),
            Err(error) => HistoryRuntimeResources::Unavailable(error.into()),
        };
    }

    pub(in crate::gpui_app) fn drain_connector(&mut self) -> bool {
        let mut changed = false;
        if let Some(connector) = self.connector.as_mut() {
            for completion in connector.drain() {
                let HistoryRuntimeResources::Connecting(current) = self.resources else {
                    continue;
                };
                if completion.request != current || !self.requested {
                    continue;
                }
                self.resources = match completion.result {
                    Ok(session) => HistoryRuntimeResources::Active(session),
                    Err(error) => {
                        tracing::warn!(
                            kind = ?error.kind(),
                            detail = error.detail(),
                            "continuous history connection failed"
                        );
                        HistoryRuntimeResources::Unavailable(error.kind().into())
                    }
                };
                if self.replay_available() && !self.replay.is_open() {
                    let request = self.replay.open();
                    self.submit_replay(request);
                } else if !self.replay_available() {
                    self.replay.close();
                    self.performance_presentation = PerformanceHistoryPresentation::Live;
                }
                changed = true;
            }
        }
        changed
    }

    pub(in crate::gpui_app) const fn replay(&self) -> &HistoryReplayState {
        &self.replay
    }

    pub(in crate::gpui_app) fn replay_mut(&mut self) -> &mut HistoryReplayState {
        &mut self.replay
    }

    pub(in crate::gpui_app) fn replay_available(&self) -> bool {
        matches!(&self.resources, HistoryRuntimeResources::Active(_))
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

    pub(in crate::gpui_app) const fn unavailable_reason(
        &self,
    ) -> Option<taskmanager_application::ApplicationHistoryUnavailableReason> {
        match self.resources {
            HistoryRuntimeResources::Unavailable(reason) => Some(reason),
            HistoryRuntimeResources::Disabled
            | HistoryRuntimeResources::Connecting(_)
            | HistoryRuntimeResources::Active(_) => None,
        }
    }

    pub(in crate::gpui_app) const fn application_history_capability(
        &self,
    ) -> taskmanager_application::ApplicationHistoryCapability {
        match self.resources {
            HistoryRuntimeResources::Disabled => {
                taskmanager_application::ApplicationHistoryCapability::Disabled
            }
            HistoryRuntimeResources::Connecting(_) => {
                taskmanager_application::ApplicationHistoryCapability::Connecting
            }
            HistoryRuntimeResources::Unavailable(reason) => {
                taskmanager_application::ApplicationHistoryCapability::Unavailable(reason)
            }
            HistoryRuntimeResources::Active(_) => {
                taskmanager_application::ApplicationHistoryCapability::Available
            }
        }
    }

    pub(in crate::gpui_app) fn replay_client_mut(&mut self) -> Option<&mut HistoryReplayClient> {
        if let HistoryRuntimeResources::Active(session) = &mut self.resources {
            return Some(&mut session.replay);
        }
        None
    }

    pub(in crate::gpui_app) fn record_sink(
        &self,
    ) -> Option<std::sync::Arc<dyn taskmanager_application::HistoryRecordSink>> {
        match &self.resources {
            HistoryRuntimeResources::Active(session) => {
                Some(session.persistence.record_sink.clone())
            }
            HistoryRuntimeResources::Disabled
            | HistoryRuntimeResources::Connecting(_)
            | HistoryRuntimeResources::Unavailable(_) => None,
        }
    }

    fn submit_replay(&mut self, request: Option<taskmanager_application::HistoryReplayRequest>) {
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

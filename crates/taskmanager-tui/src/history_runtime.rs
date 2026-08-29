//! Read-only persistent-history adapter for the terminal frontend.

use taskmanager_app_host::{
    HistoryFrontendConnectRequestId, HistoryFrontendConnector, HistoryFrontendConnectorStartError,
};
use taskmanager_application::{
    ApplicationHistoryCapability, ApplicationHistoryProjection, HistoryReplayController,
    HistoryReplayRequest,
};
use taskmanager_core::core::history::HistoryWindow;

enum HistoryResources {
    Disabled,
    Connecting(HistoryFrontendConnectRequestId),
    Unavailable(taskmanager_application::ApplicationHistoryUnavailableReason),
    Active(taskmanager_app_host::HistoryFrontendSession),
}

pub(crate) struct TuiHistoryRuntime {
    resources: HistoryResources,
    controller: HistoryReplayController,
    connector: Option<HistoryFrontendConnector>,
    requested: bool,
}

impl Default for TuiHistoryRuntime {
    fn default() -> Self {
        Self {
            resources: HistoryResources::Disabled,
            controller: HistoryReplayController::default(),
            connector: None,
            requested: false,
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
            return;
        }
        let Some(connector) = self.connector.as_mut() else {
            self.resources = HistoryResources::Unavailable(
                taskmanager_application::ApplicationHistoryUnavailableReason::ConnectorStopped,
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
            changed |= !completions.is_empty();
            for completion in completions {
                let _ = self.controller.complete(completion);
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

    pub(crate) fn unavailable_reason(
        &self,
    ) -> Option<taskmanager_application::ApplicationHistoryUnavailableReason> {
        match self.resources {
            HistoryResources::Unavailable(reason) => Some(reason),
            HistoryResources::Disabled
            | HistoryResources::Connecting(_)
            | HistoryResources::Active(_) => None,
        }
    }

    pub(crate) fn record_sink(
        &self,
    ) -> Option<std::sync::Arc<dyn taskmanager_core::core::history::HistoryRecordSink>> {
        match &self.resources {
            HistoryResources::Active(session) => Some(session.persistence.record_sink.clone()),
            HistoryResources::Disabled
            | HistoryResources::Connecting(_)
            | HistoryResources::Unavailable(_) => None,
        }
    }
}

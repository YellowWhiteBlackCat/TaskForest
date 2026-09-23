//! Iced adapter for the application history-replay lifecycle.

use std::rc::Rc;

use taskmanager_application::{
    HistoryReplayCompletionDisposition, HistoryReplayController, HistoryReplayRequest,
    HistoryReplayRequestId,
};
use taskmanager_core::core::history::{HistorySeriesKey, HistoryWindow};

use super::IcedApp;
use taskmanager_app_host::HistoryFrontendConnectRequestId;
use taskmanager_app_host::HistoryFrontendConnector;
use taskmanager_app_host::HistoryFrontendConnectorStartError;
use taskmanager_app_host::HistoryPersistenceWriter;
use taskmanager_app_host::HistoryReplayClient;
use taskmanager_application::ApplicationHistoryCapability;
use taskmanager_application::ApplicationHistoryProjection;
use taskmanager_application::ApplicationHistoryUnavailableReason;
use taskmanager_application::HistoryReplayCompletion;
use taskmanager_application::HistoryReplayError;
use taskmanager_core::core::history::HistoryRecordSink;

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

#[derive(Debug, Default)]
pub(crate) struct IcedHistoryReplay {
    controller: HistoryReplayController,
    projected_request: Option<HistoryReplayRequestId>,
    rows: Vec<HistoryReplayRow>,
    presentation: IcedHistoryPresentation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum IcedHistoryPresentation {
    #[default]
    Live,
    Replay,
}

impl IcedHistoryReplay {
    pub(crate) const fn is_open(&self) -> bool {
        matches!(self.presentation, IcedHistoryPresentation::Replay)
    }

    pub(crate) const fn is_loading(&self) -> bool {
        self.controller.is_loading()
    }

    pub(crate) const fn window(&self) -> HistoryWindow {
        self.controller.selected_window()
    }

    pub(crate) fn rows(&self) -> &[HistoryReplayRow] {
        &self.rows
    }

    pub(crate) fn failure(&self) -> Option<&HistoryReplayError> {
        self.controller.failure()
    }

    pub(crate) fn rows_window(&self) -> Option<HistoryWindow> {
        self.controller.rows_window()
    }

    pub(crate) fn loaded_at_ms(&self) -> Option<u64> {
        self.controller.loaded_at_ms()
    }

    fn open(&mut self) -> Option<HistoryReplayRequest> {
        self.presentation = IcedHistoryPresentation::Replay;
        (!self.controller.is_open())
            .then(|| self.controller.open().ok())
            .flatten()
    }

    fn close(&mut self) {
        self.presentation = IcedHistoryPresentation::Live;
    }

    fn refresh(&mut self) -> Option<HistoryReplayRequest> {
        self.controller.refresh().ok()
    }

    fn select_window(&mut self, window: HistoryWindow) -> Option<HistoryReplayRequest> {
        self.controller.select_window(window).ok()
    }

    fn reject_submission(&mut self, request: HistoryReplayRequest, error: HistoryReplayError) {
        let _ = self.controller.reject_submission(request, error);
        self.sync_rows_projection();
    }

    fn complete(&mut self, completion: HistoryReplayCompletion) -> bool {
        if self.controller.complete(completion) != HistoryReplayCompletionDisposition::Applied {
            return false;
        }
        self.sync_rows_projection();
        true
    }

    fn sync_rows_projection(&mut self) {
        let request = self.controller.rows_request_id();
        if request == self.projected_request {
            return;
        }
        self.rows = self
            .controller
            .rows()
            .iter()
            .filter(|row| !row.key.is_application_series())
            .map(|row| HistoryReplayRow {
                key: row.key.clone(),
                samples: Rc::from(row.samples.as_ref()),
                peak_value: row.peak_value,
                peak_measured_at_ms: row.peak_measured_at_ms,
                observed: row.observed,
                gaps: row.gaps,
                clock_jumps: row.clock_jumps,
            })
            .collect();
        self.projected_request = request;
    }

    fn ensure_data_request(&mut self) -> Option<HistoryReplayRequest> {
        (!self.controller.is_open())
            .then(|| self.controller.open().ok())
            .flatten()
    }

    fn application_history_projection(
        &self,
        capability: ApplicationHistoryCapability,
    ) -> ApplicationHistoryProjection {
        self.controller.application_history_projection(capability)
    }
}

enum IcedHistoryResources {
    PendingBoot(Option<HistoryReplayClient>),
    Disabled,
    Connecting(HistoryFrontendConnectRequestId),
    Unavailable(ApplicationHistoryUnavailableReason),
    Active(IcedHistoryActive),
}

struct IcedHistoryActive {
    replay: HistoryReplayClient,
    persistence: Option<HistoryPersistenceWriter>,
}

pub(crate) struct IcedHistoryRuntime {
    resources: IcedHistoryResources,
    replay: IcedHistoryReplay,
    connector: Option<HistoryFrontendConnector>,
    requested: bool,
}

impl IcedHistoryRuntime {
    pub(crate) fn new(candidate: Option<HistoryReplayClient>) -> Self {
        Self {
            resources: IcedHistoryResources::PendingBoot(candidate),
            replay: IcedHistoryReplay::default(),
            connector: None,
            requested: false,
        }
    }

    fn resolve_boot(&mut self, requested: bool) {
        self.requested = requested;
        if self.connector.is_some() {
            self.request_connection(requested);
            return;
        }
        let IcedHistoryResources::PendingBoot(candidate) = std::mem::replace(
            &mut self.resources,
            IcedHistoryResources::Unavailable(ApplicationHistoryUnavailableReason::ConnectorStart),
        ) else {
            return;
        };
        self.resources = if requested {
            candidate.map_or(
                IcedHistoryResources::Unavailable(
                    ApplicationHistoryUnavailableReason::ConnectorStart,
                ),
                |replay| {
                    IcedHistoryResources::Active(IcedHistoryActive {
                        replay,
                        persistence: None,
                    })
                },
            )
        } else {
            IcedHistoryResources::Disabled
        };
    }

    const fn is_active(&self) -> bool {
        matches!(&self.resources, IcedHistoryResources::Active(_))
    }

    /// Persistence was requested but the runtime never became active: the
    /// startup-unavailable notice state. Never true for runs that did not
    /// request history persistence.
    const fn startup_unavailable(&self) -> bool {
        self.requested && !self.is_active()
    }

    fn install_connector(
        &mut self,
        connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        match connector {
            Ok(connector) => {
                self.connector = Some(connector);
                self.request_connection(self.requested);
            }
            Err(error) if self.requested => {
                self.resources = IcedHistoryResources::Unavailable(error.into())
            }
            Err(_) => self.resources = IcedHistoryResources::Disabled,
        }
    }

    fn request_connection(&mut self, enabled: bool) {
        self.requested = enabled;
        if !enabled {
            self.resources = IcedHistoryResources::Disabled;
            self.replay.controller.close();
            return;
        }
        let Some(connector) = self.connector.as_mut() else {
            return;
        };
        self.resources = match connector.try_connect() {
            Ok(request) => IcedHistoryResources::Connecting(request),
            Err(error) => IcedHistoryResources::Unavailable(error.into()),
        };
    }

    fn drain_connector(&mut self) -> bool {
        let mut changed = false;
        if let Some(connector) = self.connector.as_mut() {
            for completion in connector.drain() {
                let IcedHistoryResources::Connecting(current) = self.resources else {
                    continue;
                };
                if completion.request != current || !self.requested {
                    continue;
                }
                self.resources = match completion.result {
                    Ok(session) => IcedHistoryResources::Active(IcedHistoryActive {
                        replay: session.replay,
                        persistence: Some(session.persistence),
                    }),
                    Err(error) => IcedHistoryResources::Unavailable(error.kind().into()),
                };
                if self.is_active() {
                    let request = self.replay.ensure_data_request();
                    if let Some(request) = request {
                        self.submit(request);
                    }
                } else {
                    self.replay.controller.close();
                }
                changed = true;
            }
        }
        changed
    }

    fn submit(&mut self, request: HistoryReplayRequest) {
        let error = self
            .client_mut()
            .and_then(|client| client.try_request(request).err());
        if let Some(error) = error {
            self.replay.reject_submission(request, error);
        }
    }

    const fn application_history_capability(&self) -> ApplicationHistoryCapability {
        match &self.resources {
            IcedHistoryResources::Disabled => ApplicationHistoryCapability::Disabled,
            IcedHistoryResources::Active(_) => ApplicationHistoryCapability::Available,
            IcedHistoryResources::Connecting(_) => ApplicationHistoryCapability::Connecting,
            IcedHistoryResources::Unavailable(reason) => {
                ApplicationHistoryCapability::Unavailable(*reason)
            }
            IcedHistoryResources::PendingBoot(_) => ApplicationHistoryCapability::Unavailable(
                ApplicationHistoryUnavailableReason::ConnectorStart,
            ),
        }
    }

    fn client_mut(&mut self) -> Option<&mut HistoryReplayClient> {
        match &mut self.resources {
            IcedHistoryResources::Active(active) => Some(&mut active.replay),
            IcedHistoryResources::PendingBoot(_)
            | IcedHistoryResources::Disabled
            | IcedHistoryResources::Connecting(_)
            | IcedHistoryResources::Unavailable(_) => None,
        }
    }

    fn record_sink(&self) -> Option<std::sync::Arc<dyn HistoryRecordSink>> {
        match &self.resources {
            IcedHistoryResources::Active(active) => active
                .persistence
                .as_ref()
                .map(|writer| writer.record_sink.clone()),
            IcedHistoryResources::PendingBoot(_)
            | IcedHistoryResources::Disabled
            | IcedHistoryResources::Connecting(_)
            | IcedHistoryResources::Unavailable(_) => None,
        }
    }

    pub(crate) const fn replay(&self) -> &IcedHistoryReplay {
        &self.replay
    }

    fn replay_mut(&mut self) -> &mut IcedHistoryReplay {
        &mut self.replay
    }
}

impl IcedApp {
    pub(crate) fn install_history_frontend_connector(
        &mut self,
        connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        self.history_runtime.install_connector(connector);
        self.sync_history_persistence_sink();
    }

    pub(crate) fn request_history_frontend(&mut self, enabled: bool) {
        self.history_runtime.request_connection(enabled);
        self.sync_history_persistence_sink();
    }

    pub(crate) fn history_replay_entry_available(&self) -> bool {
        self.history_runtime.is_active()
    }

    /// Whether persistence was requested for this run but the replay runtime
    /// could not start — the startup-unavailable notice state (GPUI
    /// `history_replay_startup_unavailable` parity). A run that never
    /// requested persistence shows nothing, never a dead notice.
    pub(crate) fn history_replay_startup_unavailable(&self) -> bool {
        self.history_runtime.startup_unavailable()
    }

    pub(crate) fn activate_history_replay_for_boot(&mut self) {
        self.history_runtime
            .resolve_boot(self.configuration.draft().history_persistence);
        self.sync_history_persistence_sink();
        if self.history_runtime.is_active()
            && let Some(request) = self.history_runtime.replay_mut().ensure_data_request()
        {
            self.submit_history_replay(request);
        }
    }

    pub(super) fn toggle_history_replay(&mut self) {
        if self.history_runtime.replay().is_open() {
            self.history_runtime.replay_mut().close();
        } else if self.history_replay_entry_available()
            && let Some(request) = self.history_runtime.replay_mut().open()
        {
            self.submit_history_replay(request);
        }
    }

    pub(super) fn select_history_replay_window(&mut self, window: HistoryWindow) {
        if let Some(request) = self.history_runtime.replay_mut().select_window(window) {
            self.submit_history_replay(request);
        }
    }

    pub(super) fn refresh_history_replay(&mut self) {
        if let Some(request) = self.history_runtime.replay_mut().refresh() {
            self.submit_history_replay(request);
        }
    }

    pub(super) fn drain_history_replay_completions(&mut self) {
        let connector_changed = self.history_runtime.drain_connector();
        if connector_changed {
            self.sync_history_persistence_sink();
        }
        let Some(client) = self.history_runtime.client_mut() else {
            return;
        };
        let completions = client.drain();
        for completion in completions {
            let _ = self.history_runtime.replay_mut().complete(completion);
        }
    }

    fn submit_history_replay(&mut self, request: HistoryReplayRequest) {
        let error = self
            .history_runtime
            .client_mut()
            .and_then(|client| client.try_request(request).err());
        if let Some(error) = error {
            self.history_runtime
                .replay_mut()
                .reject_submission(request, error);
        }
    }

    pub(crate) const fn history_replay_state(&self) -> &IcedHistoryReplay {
        self.history_runtime.replay()
    }

    pub(crate) fn application_history_projection(&self) -> ApplicationHistoryProjection {
        self.history_runtime
            .replay()
            .application_history_projection(self.history_runtime.application_history_capability())
    }

    fn sync_history_persistence_sink(&mut self) {
        self.shell
            .set_history_persistence_sink(self.history_runtime.record_sink());
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/history_replay_tests.rs"]
mod tests;

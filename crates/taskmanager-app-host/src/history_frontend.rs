//! Read-only persistent-history composition for product frontends.

use std::fmt;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use super::{HistoryPersistenceWriter, HistoryReplayClient, NativeAppHost};

use crate::worker_fault::catch_worker_panic;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryFrontendStartErrorKind {
    PersistenceWriter,
    ReplayWorker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryFrontendStartError {
    kind: HistoryFrontendStartErrorKind,
    detail: Arc<str>,
}

impl HistoryFrontendStartError {
    #[must_use]
    pub const fn kind(&self) -> HistoryFrontendStartErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for HistoryFrontendStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            HistoryFrontendStartErrorKind::PersistenceWriter => {
                "history persistence writer could not start"
            }
            HistoryFrontendStartErrorKind::ReplayWorker => "history replay worker could not start",
        })
    }
}

impl std::error::Error for HistoryFrontendStartError {}

pub struct HistoryFrontendSession {
    pub replay: HistoryReplayClient,
    pub persistence: HistoryPersistenceWriter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HistoryFrontendConnectRequestId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HistoryFrontendConnectRequest {
    id: HistoryFrontendConnectRequestId,
}

#[derive(Debug)]
pub struct HistoryFrontendConnectCompletion {
    pub request: HistoryFrontendConnectRequestId,
    pub result: Result<HistoryFrontendSession, HistoryFrontendStartError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryFrontendConnectSubmitError {
    Busy,
    WorkerStopped,
    RequestSpaceExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryFrontendConnectorStartError {
    detail: Arc<str>,
}

impl HistoryFrontendConnectorStartError {
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for HistoryFrontendConnectorStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("history frontend connector could not start")
    }
}

impl std::error::Error for HistoryFrontendConnectorStartError {}

impl From<HistoryFrontendConnectorStartError>
    for taskmanager_application::ApplicationHistoryUnavailableReason
{
    fn from(_: HistoryFrontendConnectorStartError) -> Self {
        Self::ConnectorStart
    }
}

impl From<HistoryFrontendConnectSubmitError>
    for taskmanager_application::ApplicationHistoryUnavailableReason
{
    fn from(error: HistoryFrontendConnectSubmitError) -> Self {
        match error {
            HistoryFrontendConnectSubmitError::Busy => Self::ConnectorBusy,
            HistoryFrontendConnectSubmitError::WorkerStopped => Self::ConnectorStopped,
            HistoryFrontendConnectSubmitError::RequestSpaceExhausted => Self::RequestSpaceExhausted,
        }
    }
}

impl From<HistoryFrontendStartErrorKind>
    for taskmanager_application::ApplicationHistoryUnavailableReason
{
    fn from(error: HistoryFrontendStartErrorKind) -> Self {
        match error {
            HistoryFrontendStartErrorKind::PersistenceWriter => Self::PersistenceWriter,
            HistoryFrontendStartErrorKind::ReplayWorker => Self::ReplayWorker,
        }
    }
}

/// Narrow non-blocking capability for changing the history-recording
/// preference at runtime. Filesystem bootstrap and replay startup stay on this
/// worker; both capabilities stop with the owning frontend process.
pub struct HistoryFrontendConnector {
    requests: SyncSender<HistoryFrontendConnectRequest>,
    completions: Receiver<HistoryFrontendConnectCompletion>,
    next_request: Option<u64>,
}

impl HistoryFrontendConnector {
    pub fn try_connect(
        &mut self,
    ) -> Result<HistoryFrontendConnectRequestId, HistoryFrontendConnectSubmitError> {
        let Some(next_request) = self.next_request else {
            return Err(HistoryFrontendConnectSubmitError::RequestSpaceExhausted);
        };
        let request = HistoryFrontendConnectRequest {
            id: HistoryFrontendConnectRequestId(next_request),
        };
        match self.requests.try_send(request) {
            Ok(()) => {
                self.next_request = next_request.checked_add(1);
                Ok(request.id)
            }
            Err(TrySendError::Full(_)) => Err(HistoryFrontendConnectSubmitError::Busy),
            Err(TrySendError::Disconnected(_)) => {
                Err(HistoryFrontendConnectSubmitError::WorkerStopped)
            }
        }
    }

    pub fn drain(&mut self) -> Vec<HistoryFrontendConnectCompletion> {
        self.completions.try_iter().collect()
    }
}

impl fmt::Debug for HistoryFrontendSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryFrontendSession")
            .finish_non_exhaustive()
    }
}

impl NativeAppHost {
    pub fn history_frontend_connector(
        &self,
    ) -> Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError> {
        let (request_tx, request_rx) = sync_channel::<HistoryFrontendConnectRequest>(4);
        let (completion_tx, completion_rx) = sync_channel::<HistoryFrontendConnectCompletion>(2);
        let host = self.clone();
        std::thread::Builder::new()
            .name("taskforest-history-frontend".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    // Per-request isolation with an honest exit: composition
                    // panics resolve the submitted request with a typed
                    // startup failure instead of leaving the frontend waiting
                    // forever, and the connector then stops. `request_rx`
                    // drops with the thread, so later `try_connect` observes
                    // the typed `WorkerStopped` disconnect rather than a
                    // silent zombie lane. The fault maps to the broadest
                    // existing start kind; the bounded detail carries the
                    // isolated-panic fact.
                    let (result, faulted) =
                        match catch_worker_panic(|| host.connect_enabled_history()) {
                            Ok(result) => (result, false),
                            Err(detail) => (
                                Err(HistoryFrontendStartError {
                                    kind: HistoryFrontendStartErrorKind::PersistenceWriter,
                                    detail,
                                }),
                                true,
                            ),
                        };
                    let completion = HistoryFrontendConnectCompletion {
                        request: request.id,
                        result,
                    };
                    if completion_tx.send(completion).is_err() {
                        break;
                    }
                    if faulted {
                        break;
                    }
                }
            })
            .map_err(|error| HistoryFrontendConnectorStartError {
                detail: Arc::from(error.to_string()),
            })?;
        Ok(HistoryFrontendConnector {
            requests: request_tx,
            completions: completion_rx,
            next_request: Some(1),
        })
    }

    /// Start the in-process persistence and replay capabilities for an
    /// enabled frontend session. The frontend retains the writer only as an
    /// owned capability; it never receives a filesystem path or store handle.
    fn connect_enabled_history(&self) -> Result<HistoryFrontendSession, HistoryFrontendStartError> {
        let persistence =
            self.history_persistence_writer()
                .map_err(|error| HistoryFrontendStartError {
                    kind: HistoryFrontendStartErrorKind::PersistenceWriter,
                    detail: Arc::from(error.detail()),
                })?;
        let replay =
            self.enabled_history_replay_client()
                .map_err(|error| HistoryFrontendStartError {
                    kind: HistoryFrontendStartErrorKind::ReplayWorker,
                    detail: Arc::from(error.detail()),
                })?;
        Ok(HistoryFrontendSession {
            replay,
            persistence,
        })
    }
}

#[cfg(test)]
#[path = "../tests/headless/history_frontend_tests.rs"]
mod tests;

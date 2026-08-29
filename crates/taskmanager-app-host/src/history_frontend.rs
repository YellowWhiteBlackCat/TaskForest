//! Read-only persistent-history composition for product frontends.

use std::fmt;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

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
    inner: Arc<HistoryFrontendRuntimeInner>,
    completions: Receiver<HistoryFrontendConnectCompletion>,
    next_request: Option<u64>,
}

struct HistoryFrontendRuntimeInner {
    requests: SyncSender<HistoryFrontendConnectRequest>,
    start_receiver: Mutex<Option<Receiver<HistoryFrontendConnectRequest>>>,
    completion_tx: SyncSender<HistoryFrontendConnectCompletion>,
    host: NativeAppHost,
    start_result: OnceLock<Result<(), Arc<str>>>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl HistoryFrontendRuntimeInner {
    fn ensure_started(&self) -> Result<(), Arc<str>> {
        let result = self.start_result.get_or_init(|| {
            let Some(request_rx) = self
                .start_receiver
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            else {
                return Err(Arc::from(
                    "history frontend worker start state was consumed",
                ));
            };
            let host = self.host.clone();
            let completion_tx = self.completion_tx.clone();
            let join = std::thread::Builder::new()
                .name("taskforest-history-frontend".to_owned())
                .stack_size(1024 * 1024)
                .spawn(move || {
                    while let Ok(request) = request_rx.recv() {
                        // Per-request isolation with an honest exit: a fault
                        // resolves the submitted request with a typed startup
                        // failure and then stops this connector worker.
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
                        if completion_tx.send(completion).is_err() || faulted {
                            break;
                        }
                    }
                })
                .map_err(|error| Arc::<str>::from(error.to_string()));
            match join {
                Ok(join) => {
                    *self
                        .join
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(join);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        });
        result.clone()
    }
}

impl Drop for HistoryFrontendRuntimeInner {
    fn drop(&mut self) {
        // Dropping the last request sender disconnects the receiver. Join only
        // when the worker has already observed that disconnect; a provider
        // bootstrap must never make frontend teardown block indefinitely.
        let join = self.join.get_mut().ok().and_then(Option::take);
        if let Some(join) = join
            && join.is_finished()
        {
            let _ = join.join();
        }
    }
}

impl HistoryFrontendConnector {
    pub fn try_connect(
        &mut self,
    ) -> Result<HistoryFrontendConnectRequestId, HistoryFrontendConnectSubmitError> {
        let Some(next_request) = self.next_request else {
            return Err(HistoryFrontendConnectSubmitError::RequestSpaceExhausted);
        };
        if self.inner.ensure_started().is_err() {
            return Err(HistoryFrontendConnectSubmitError::WorkerStopped);
        }
        let request = HistoryFrontendConnectRequest {
            id: HistoryFrontendConnectRequestId(next_request),
        };
        match self.inner.requests.try_send(request) {
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
        let inner = Arc::new(HistoryFrontendRuntimeInner {
            requests: request_tx,
            start_receiver: Mutex::new(Some(request_rx)),
            completion_tx,
            host: self.clone(),
            start_result: OnceLock::new(),
            join: Mutex::new(None),
        });
        Ok(HistoryFrontendConnector {
            inner,
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

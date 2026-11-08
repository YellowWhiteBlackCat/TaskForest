//! The connector-backed history session lifecycle.
//!
//! One owner for the `Disabled → Connecting(request) → Active(replay +
//! writer) / Unavailable` state machine that
//! [`docs/STATE_OWNERSHIP.md`](../../../docs/STATE_OWNERSHIP.md) records as
//! the `history frontend` authority. Frontends bind to this runtime: none of
//! them re-declares the resource enum, the connect handshake, the capability
//! projection, the record sink or the replay request rule.
//!
//! The renderer-visible open/closed state of a replay panel stays with the
//! frontend — that is a surface decision. Everything below it (which session
//! is live, which window it replays, whether a request was accepted) is one
//! fact with one owner.

use std::sync::Arc;

use taskforest_core::core::history::{HistoryRecordSink, HistoryWindow};
use taskmanager_application::{
    ApplicationHistoryCapability, ApplicationHistoryProjection,
    ApplicationHistoryUnavailableReason, HistoryReplayCompletionDisposition,
    HistoryReplayController, HistoryReplayRequest, PerformanceHistoryProjection,
};

use crate::history_frontend::{
    HistoryFrontendConnectRequestId, HistoryFrontendConnector, HistoryFrontendConnectorStartError,
    HistoryFrontendSession, HistoryFrontendStartErrorKind,
};
use crate::history_persistence_runtime::HistoryPersistenceWriter;
use crate::history_replay_runtime::HistoryReplayClient;

/// The live session a frontend holds while history persistence is enabled.
struct HistoryFrontendActive {
    replay: HistoryReplayClient,
    /// `None` only when the frontend was handed a replay client directly and
    /// no native writer stands behind it.
    persistence: Option<HistoryPersistenceWriter>,
}

/// The one resource enum behind the history frontend lifecycle.
enum HistoryFrontendResources {
    Disabled,
    Connecting(HistoryFrontendConnectRequestId),
    Unavailable(ApplicationHistoryUnavailableReason),
    Active(HistoryFrontendActive),
}

/// What one connector drain did to the lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryFrontendConnectOutcome {
    /// No matching completion arrived; nothing changed.
    Idle,
    /// The session became active and the replay view opened.
    Activated,
    /// The session failed with a typed startup error.
    Failed(HistoryFrontendStartErrorKind),
}

/// The one history-frontend session lifecycle.
pub struct HistoryFrontendRuntime {
    resources: HistoryFrontendResources,
    controller: HistoryReplayController,
    connector: Option<HistoryFrontendConnector>,
    requested: bool,
}

impl Default for HistoryFrontendRuntime {
    fn default() -> Self {
        Self {
            resources: HistoryFrontendResources::Disabled,
            controller: HistoryReplayController::default(),
            connector: None,
            requested: false,
        }
    }
}

impl HistoryFrontendRuntime {
    /// Install the app-host connector after composition.
    ///
    /// A failed connector is retained as a typed unavailable state only when
    /// history was requested; disabled history stays inert.
    pub fn install_connector(
        &mut self,
        connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        match connector {
            Ok(connector) => {
                self.connector = Some(connector);
                self.request(self.requested);
            }
            Err(error) if self.requested => {
                self.resources = HistoryFrontendResources::Unavailable(error.into());
            }
            Err(_) => self.resources = HistoryFrontendResources::Disabled,
        }
    }

    /// Change the canonical enabled preference without touching storage.
    ///
    /// The connector performs the bounded persistence/replay bootstrap away
    /// from the UI thread. An already connecting or active session keeps its
    /// request: a second connect would submit a duplicate bootstrap.
    pub fn request(&mut self, enabled: bool) {
        self.requested = enabled;
        if !enabled {
            self.resources = HistoryFrontendResources::Disabled;
            self.controller.close();
            return;
        }
        if matches!(
            self.resources,
            HistoryFrontendResources::Connecting(_) | HistoryFrontendResources::Active(_)
        ) {
            return;
        }
        let Some(connector) = self.connector.as_mut() else {
            self.resources = HistoryFrontendResources::Unavailable(
                ApplicationHistoryUnavailableReason::ConnectorStart,
            );
            return;
        };
        self.resources = match connector.try_connect() {
            Ok(request) => HistoryFrontendResources::Connecting(request),
            Err(error) => HistoryFrontendResources::Unavailable(error.into()),
        };
    }

    /// Adopt a replay client the frontend already owns as the active session.
    ///
    /// The boot path that hands a client in before the native connector is
    /// installed uses this: the session is live, but no native writer stands
    /// behind it, so the record sink stays absent.
    pub fn adopt_replay_client(&mut self, replay: HistoryReplayClient) {
        self.resources = HistoryFrontendResources::Active(HistoryFrontendActive {
            replay,
            persistence: None,
        });
        self.open_replay();
    }

    /// Drain the connector's connect completions.
    ///
    /// A completion for a request that is no longer current, or that arrives
    /// after history was disabled, is discarded. A matching completion
    /// activates the session and opens the replay view; a failed one closes
    /// the view again and reports a typed reason.
    pub fn drain_connector(&mut self) -> HistoryFrontendConnectOutcome {
        let mut outcome = HistoryFrontendConnectOutcome::Idle;
        let Some(connector) = self.connector.as_mut() else {
            return outcome;
        };
        for completion in connector.drain() {
            let HistoryFrontendResources::Connecting(current) = self.resources else {
                continue;
            };
            if completion.request != current || !self.requested {
                continue;
            }
            match completion.result {
                Ok(session) => {
                    self.activate(session);
                    outcome = HistoryFrontendConnectOutcome::Activated;
                }
                Err(error) => {
                    self.resources = HistoryFrontendResources::Unavailable(error.kind().into());
                    self.controller.close();
                    outcome = HistoryFrontendConnectOutcome::Failed(error.kind());
                }
            }
        }
        outcome
    }

    /// Drain the active session's replay completions into the controller.
    ///
    /// `true` when at least one completion was applied; a stale completion is
    /// ignored rather than reported.
    pub fn drain_replay(&mut self) -> bool {
        let mut applied = false;
        if let HistoryFrontendResources::Active(active) = &mut self.resources {
            for completion in active.replay.drain() {
                applied |= self.controller.complete(completion)
                    == HistoryReplayCompletionDisposition::Applied;
            }
        }
        applied
    }

    /// Drain every pending completion and report whether the runtime's
    /// observable state advanced.
    ///
    /// The connector's outcome and the replay's completions are two drains
    /// with one answer: a frontend repaints only when something actually
    /// moved. A frontend that must react to the two separately (for example
    /// because a new session has to be published to the persistence sink)
    /// drains them one at a time.
    pub fn drain_changes(&mut self) -> bool {
        let connector_changed = self.drain_connector() != HistoryFrontendConnectOutcome::Idle;
        let replay_applied = self.drain_replay();
        connector_changed || replay_applied
    }

    /// Whether the frontend asked for history persistence in this run.
    #[must_use]
    pub const fn requested(&self) -> bool {
        self.requested
    }

    /// Whether the native connector is already installed.
    #[must_use]
    pub const fn connector_installed(&self) -> bool {
        self.connector.is_some()
    }

    /// Whether the history session is live.
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self.resources, HistoryFrontendResources::Active(_))
    }

    /// Whether persistence was requested for this run but the session never
    /// became active. A run that never requested persistence shows nothing.
    #[must_use]
    pub fn startup_unavailable(&self) -> bool {
        self.requested && !self.is_active()
    }

    /// The user-visible capability of this session.
    #[must_use]
    pub fn capability(&self) -> ApplicationHistoryCapability {
        match self.resources {
            HistoryFrontendResources::Disabled => ApplicationHistoryCapability::Disabled,
            HistoryFrontendResources::Connecting(_) => ApplicationHistoryCapability::Connecting,
            HistoryFrontendResources::Unavailable(reason) => {
                ApplicationHistoryCapability::Unavailable(reason)
            }
            HistoryFrontendResources::Active(_) => ApplicationHistoryCapability::Available,
        }
    }

    /// The typed reason the session is unavailable, if it is.
    #[must_use]
    pub fn unavailable_reason(&self) -> Option<ApplicationHistoryUnavailableReason> {
        match self.resources {
            HistoryFrontendResources::Unavailable(reason) => Some(reason),
            HistoryFrontendResources::Disabled
            | HistoryFrontendResources::Connecting(_)
            | HistoryFrontendResources::Active(_) => None,
        }
    }

    /// The record sink of the live session, if a native writer backs it.
    #[must_use]
    pub fn record_sink(&self) -> Option<Arc<dyn HistoryRecordSink>> {
        match &self.resources {
            HistoryFrontendResources::Active(active) => active
                .persistence
                .as_ref()
                .map(|writer| writer.record_sink.clone()),
            HistoryFrontendResources::Disabled
            | HistoryFrontendResources::Connecting(_)
            | HistoryFrontendResources::Unavailable(_) => None,
        }
    }

    /// The live session's replay client, when the session is active.
    pub fn replay_client_mut(&mut self) -> Option<&mut HistoryReplayClient> {
        match &mut self.resources {
            HistoryFrontendResources::Active(active) => Some(&mut active.replay),
            HistoryFrontendResources::Disabled
            | HistoryFrontendResources::Connecting(_)
            | HistoryFrontendResources::Unavailable(_) => None,
        }
    }

    /// The replay controller: one instance per frontend session.
    #[must_use]
    pub const fn controller(&self) -> &HistoryReplayController {
        &self.controller
    }

    /// The replay controller, mutably.
    pub fn controller_mut(&mut self) -> &mut HistoryReplayController {
        &mut self.controller
    }

    /// The replayed window.
    #[must_use]
    pub const fn window(&self) -> HistoryWindow {
        self.controller.selected_window()
    }

    /// Open the replay data view and submit its first request.
    ///
    /// A view that is already open keeps its current request: opening twice
    /// must not submit a second bootstrap.
    pub fn open_replay(&mut self) {
        if self.controller.is_open() {
            return;
        }
        let request = self.controller.open().ok();
        self.submit(request);
    }

    /// Close the replay data view; the session's evidence is discarded.
    pub fn close_replay(&mut self) {
        self.controller.close();
    }

    /// Reload the replayed window.
    pub fn refresh_replay(&mut self) {
        if !self.is_active() {
            return;
        }
        let request = self.controller.refresh().ok();
        self.submit(request);
    }

    /// Switch the replayed window and reload.
    ///
    /// `true` when a new window request was issued; a closed view issues
    /// none.
    pub fn select_window(&mut self, window: HistoryWindow) -> bool {
        let request = self.controller.select_window(window).ok();
        let changed = request.is_some();
        self.submit(request);
        changed
    }

    /// Submit one replay request to the live session's client.
    ///
    /// A rejected submission is reported to the controller as a typed
    /// failure, so a view never waits for a completion that cannot arrive.
    pub fn submit(&mut self, request: Option<HistoryReplayRequest>) {
        let Some(request) = request else {
            return;
        };
        let error = self
            .replay_client_mut()
            .and_then(|client| client.try_request(request).err());
        if let Some(error) = error {
            self.controller.reject_submission(request, error);
        }
    }

    /// The application-history projection of this session.
    #[must_use]
    pub fn application_history_projection(&self) -> ApplicationHistoryProjection {
        self.controller
            .application_history_projection(self.capability())
    }

    /// The performance-history projection of this session.
    #[must_use]
    pub fn performance_history_projection(&self) -> PerformanceHistoryProjection {
        self.controller
            .performance_history_projection(self.capability())
    }

    fn activate(&mut self, session: HistoryFrontendSession) {
        self.resources = HistoryFrontendResources::Active(HistoryFrontendActive {
            replay: session.replay,
            persistence: Some(session.persistence),
        });
        self.open_replay();
    }
}

impl std::fmt::Debug for HistoryFrontendRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HistoryFrontendRuntime")
            .field("active", &self.is_active())
            .field("requested", &self.requested)
            .field("window", &self.window())
            .finish_non_exhaustive()
    }
}

//! OS-neutral startup and login-session execution contracts.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::{
    PlatformEvent, SessionControlOutcome, SessionControlRequest, SessionEvent,
    SessionInventoryRequest, StartupControlOutcome, StartupControlRequest, StartupEvent,
    StartupEvidenceEvent, StartupEvidenceRequest, StartupInventoryRequest,
};
use taskmanager_core::core::session::{SessionControlAction, SessionItem};
use taskmanager_core::core::startup::{StartupBootEvidenceSnapshot, StartupEntry};
use taskmanager_core::core::target::SessionId;
use taskmanager_platform_contract::{CapabilityId, PartialSourceSnapshot, ProviderFailure};

use crate::{
    Queued, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, spawn_observation_lane,
    spawn_typed_outcome_lane,
};

type StartupInventoryExecutor =
    dyn FnMut() -> Result<PartialSourceSnapshot<StartupEntry>, ProviderFailure> + Send + 'static;
type StartupEvidenceExecutor =
    dyn FnMut(u64) -> Result<StartupBootEvidenceSnapshot, ProviderFailure> + Send + 'static;
type StartupControlExecutor =
    dyn FnMut(&StartupEntry, bool) -> Result<(), ProviderFailure> + Send + 'static;
type SessionInventoryExecutor =
    dyn FnMut() -> Result<PartialSourceSnapshot<SessionItem>, ProviderFailure> + Send + 'static;
type SessionControlExecutor =
    dyn FnMut(&SessionId, SessionControlAction) -> Result<(), ProviderFailure> + Send + 'static;

/// Native environment operations adapted into OS-independent closures.
pub struct EnvironmentExecutors {
    startup_inventory: Box<StartupInventoryExecutor>,
    startup_evidence: Box<StartupEvidenceExecutor>,
    startup_control: Box<StartupControlExecutor>,
    session_inventory: Box<SessionInventoryExecutor>,
    session_control: Box<SessionControlExecutor>,
}

impl EnvironmentExecutors {
    #[must_use]
    pub fn new<I, E, C, S, M>(
        startup_inventory: I,
        startup_evidence: E,
        startup_control: C,
        session_inventory: S,
        session_control: M,
    ) -> Self
    where
        I: FnMut() -> Result<PartialSourceSnapshot<StartupEntry>, ProviderFailure> + Send + 'static,
        E: FnMut(u64) -> Result<StartupBootEvidenceSnapshot, ProviderFailure> + Send + 'static,
        C: FnMut(&StartupEntry, bool) -> Result<(), ProviderFailure> + Send + 'static,
        S: FnMut() -> Result<PartialSourceSnapshot<SessionItem>, ProviderFailure> + Send + 'static,
        M: FnMut(&SessionId, SessionControlAction) -> Result<(), ProviderFailure> + Send + 'static,
    {
        Self {
            startup_inventory: Box::new(startup_inventory),
            startup_evidence: Box::new(startup_evidence),
            startup_control: Box::new(startup_control),
            session_inventory: Box::new(session_inventory),
            session_control: Box::new(session_control),
        }
    }
}

/// Optional environment receivers while native bindings are assembled.
pub struct PendingEnvironmentRuntimeLanes {
    pub startup_inventory_rx: Option<Receiver<Queued<StartupInventoryRequest>>>,
    pub startup_evidence_rx: Option<Receiver<Queued<StartupEvidenceRequest>>>,
    pub startup_control_rx: Option<Receiver<Queued<StartupControlRequest>>>,
    pub session_inventory_rx: Option<Receiver<Queued<SessionInventoryRequest>>>,
    pub session_control_rx: Option<Receiver<Queued<SessionControlRequest>>>,
}

impl PendingEnvironmentRuntimeLanes {
    pub(crate) fn new(
        startup_inventory_rx: Option<Receiver<Queued<StartupInventoryRequest>>>,
        startup_evidence_rx: Option<Receiver<Queued<StartupEvidenceRequest>>>,
        startup_control_rx: Option<Receiver<Queued<StartupControlRequest>>>,
        session_inventory_rx: Option<Receiver<Queued<SessionInventoryRequest>>>,
        session_control_rx: Option<Receiver<Queued<SessionControlRequest>>>,
    ) -> Self {
        Self {
            startup_inventory_rx,
            startup_evidence_rx,
            startup_control_rx,
            session_inventory_rx,
            session_control_rx,
        }
    }

    pub(crate) fn missing_capabilities(&self) -> impl Iterator<Item = CapabilityId> {
        [
            (self.startup_inventory_rx.is_none(), CapabilityId::STARTUP),
            (
                self.startup_evidence_rx.is_none(),
                CapabilityId::STARTUP_EVIDENCE,
            ),
            (
                self.startup_control_rx.is_none(),
                CapabilityId::STARTUP_CONTROL,
            ),
            (self.session_inventory_rx.is_none(), CapabilityId::SESSIONS),
            (
                self.session_control_rx.is_none(),
                CapabilityId::SESSION_CONTROL,
            ),
        ]
        .into_iter()
        .filter_map(|(is_missing, capability)| is_missing.then_some(capability))
    }

    /// Promote the environment family only when all five lanes exist.
    #[must_use]
    pub fn try_complete(self) -> Option<EnvironmentRuntimeLanes> {
        let Self {
            startup_inventory_rx: Some(startup_inventory),
            startup_evidence_rx: Some(startup_evidence),
            startup_control_rx: Some(startup_control),
            session_inventory_rx: Some(session_inventory),
            session_control_rx: Some(session_control),
        } = self
        else {
            return None;
        };
        Some(EnvironmentRuntimeLanes {
            startup_inventory,
            startup_evidence,
            startup_control,
            session_inventory,
            session_control,
        })
    }
}

/// Complete provider-side receivers for startup and session capabilities.
pub struct EnvironmentRuntimeLanes {
    startup_inventory: Receiver<Queued<StartupInventoryRequest>>,
    startup_evidence: Receiver<Queued<StartupEvidenceRequest>>,
    startup_control: Receiver<Queued<StartupControlRequest>>,
    session_inventory: Receiver<Queued<SessionInventoryRequest>>,
    session_control: Receiver<Queued<SessionControlRequest>>,
}

/// Attach all environment executors to their independent typed lanes.
pub fn spawn_environment_lanes(
    workers: &WorkerRuntime,
    lanes: EnvironmentRuntimeLanes,
    executors: EnvironmentExecutors,
    events: Arc<RuntimeEventPublisher>,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError> {
    let EnvironmentRuntimeLanes {
        startup_inventory,
        startup_evidence,
        startup_control,
        session_inventory,
        session_control,
    } = lanes;
    let EnvironmentExecutors {
        startup_inventory: mut execute_startup_inventory,
        startup_evidence: mut execute_startup_evidence,
        startup_control: mut execute_startup_control,
        session_inventory: mut execute_session_inventory,
        session_control: mut execute_session_control,
    } = executors;

    spawn_observation_lane(
        workers,
        startup_inventory,
        events.clone(),
        move |StartupInventoryRequest::Refresh| execute_startup_inventory(),
        |snapshot| PlatformEvent::Startup(StartupEvent::Snapshot(snapshot)),
    )?;
    spawn_observation_lane(
        workers,
        startup_evidence,
        events.clone(),
        move |StartupEvidenceRequest::Refresh| execute_startup_evidence(clock_ms()),
        |snapshot| PlatformEvent::StartupEvidence(StartupEvidenceEvent::Snapshot(snapshot)),
    )?;
    spawn_typed_outcome_lane(
        workers,
        startup_control,
        events.clone(),
        move |_, request: StartupControlRequest| {
            let result = execute_startup_control(&request.entry, request.enabled);
            let provider_result = result;
            (
                PlatformEvent::Startup(StartupEvent::Control(StartupControlOutcome {
                    request_id: request.request_id,
                    target_id: request.entry.id,
                    target_name: request.entry.name,
                    enabled: request.enabled,
                    result: result.map_err(ProviderFailure::kind),
                })),
                provider_result,
            )
        },
    )?;
    spawn_observation_lane(
        workers,
        session_inventory,
        events.clone(),
        move |SessionInventoryRequest::Refresh| execute_session_inventory(),
        |snapshot| PlatformEvent::Sessions(SessionEvent::Snapshot(snapshot)),
    )?;
    spawn_typed_outcome_lane(
        workers,
        session_control,
        events,
        move |_, request: SessionControlRequest| {
            let result = execute_session_control(&request.session_id, request.action);
            let provider_result = result;
            (
                PlatformEvent::Sessions(SessionEvent::Control(SessionControlOutcome {
                    request_id: request.request_id,
                    session_id: request.session_id,
                    action: request.action,
                    result: result.map_err(ProviderFailure::kind),
                })),
                provider_result,
            )
        },
    )
}

#[cfg(test)]
#[path = "../tests/headless/environment.rs"]
mod tests;

//! OS-neutral service execution contracts and typed lane routing.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::{
    CapabilityId, PartialSourceSnapshot, PlatformEvent, ProviderFailure, ServiceControlOutcome,
    ServiceControlRequest, ServiceDependenciesRequest, ServiceEvent, ServiceInventoryRequest,
    ServiceLogSnapshotRequest, ServiceLogStreamRequest, ServiceUpdate,
};
use taskmanager_core::{
    ServiceAction, ServiceDeps, ServiceId, ServiceItem, ServiceLogErrorKind, ServiceLogFailure,
    ServiceLogQuery, ServiceLogSnapshot, ServiceLogState, ServiceLogStreamEnd,
    ServiceLogStreamSnapshot, ServiceLogStreamState,
};

use crate::{
    Queued, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, spawn_observation_lane,
    spawn_typed_outcome_lane,
};

type InventoryExecutor =
    dyn FnMut() -> Result<PartialSourceSnapshot<ServiceItem>, ProviderFailure> + Send + 'static;
type DependenciesExecutor =
    dyn FnMut(ServiceId) -> Result<ServiceDeps, ProviderFailure> + Send + 'static;
type ControlExecutor =
    dyn FnMut(ServiceId, ServiceAction) -> Result<(), ProviderFailure> + Send + 'static;
type LogSnapshotExecutor =
    dyn FnMut(ServiceId) -> Result<ServiceLogState, ProviderFailure> + Send + 'static;
type LogStreamExecutor = dyn FnMut(ServiceLogQuery, u64) -> Result<ServiceLogStreamState, ProviderFailure>
    + Send
    + 'static;

/// Native service operations adapted into OS-independent executor closures.
///
/// An OS adapter owns provider SPI objects and delegates each closure to one
/// provider method. Request-to-event mapping and capability health remain in
/// this shared runtime.
pub struct ServiceExecutors {
    inventory: Box<InventoryExecutor>,
    dependencies: Box<DependenciesExecutor>,
    control: Box<ControlExecutor>,
    log_snapshot: Box<LogSnapshotExecutor>,
    log_stream: Box<LogStreamExecutor>,
}

impl ServiceExecutors {
    #[must_use]
    pub fn new<I, D, C, L, S>(
        inventory: I,
        dependencies: D,
        control: C,
        log_snapshot: L,
        log_stream: S,
    ) -> Self
    where
        I: FnMut() -> Result<PartialSourceSnapshot<ServiceItem>, ProviderFailure> + Send + 'static,
        D: FnMut(ServiceId) -> Result<ServiceDeps, ProviderFailure> + Send + 'static,
        C: FnMut(ServiceId, ServiceAction) -> Result<(), ProviderFailure> + Send + 'static,
        L: FnMut(ServiceId) -> Result<ServiceLogState, ProviderFailure> + Send + 'static,
        S: FnMut(ServiceLogQuery, u64) -> Result<ServiceLogStreamState, ProviderFailure>
            + Send
            + 'static,
    {
        Self {
            inventory: Box::new(inventory),
            dependencies: Box::new(dependencies),
            control: Box::new(control),
            log_snapshot: Box::new(log_snapshot),
            log_stream: Box::new(log_stream),
        }
    }
}

/// Optional service receivers while native capability bindings are assembled.
pub struct PendingServiceRuntimeLanes {
    pub inventory_rx: Option<Receiver<Queued<ServiceInventoryRequest>>>,
    pub dependencies_rx: Option<Receiver<Queued<ServiceDependenciesRequest>>>,
    pub control_rx: Option<Receiver<Queued<ServiceControlRequest>>>,
    pub log_snapshot_rx: Option<Receiver<Queued<ServiceLogSnapshotRequest>>>,
    pub log_stream_rx: Option<Receiver<Queued<ServiceLogStreamRequest>>>,
}

impl PendingServiceRuntimeLanes {
    #[must_use]
    pub(crate) fn new(
        inventory_rx: Option<Receiver<Queued<ServiceInventoryRequest>>>,
        dependencies_rx: Option<Receiver<Queued<ServiceDependenciesRequest>>>,
        control_rx: Option<Receiver<Queued<ServiceControlRequest>>>,
        log_snapshot_rx: Option<Receiver<Queued<ServiceLogSnapshotRequest>>>,
        log_stream_rx: Option<Receiver<Queued<ServiceLogStreamRequest>>>,
    ) -> Self {
        Self {
            inventory_rx,
            dependencies_rx,
            control_rx,
            log_snapshot_rx,
            log_stream_rx,
        }
    }

    pub(crate) fn missing_capabilities(&self) -> impl Iterator<Item = CapabilityId> {
        [
            (self.inventory_rx.is_none(), CapabilityId::SERVICES),
            (
                self.dependencies_rx.is_none(),
                CapabilityId::SERVICE_DEPENDENCIES,
            ),
            (self.control_rx.is_none(), CapabilityId::SERVICE_CONTROL),
            (self.log_snapshot_rx.is_none(), CapabilityId::SERVICE_LOGS),
            (
                self.log_stream_rx.is_none(),
                CapabilityId::SERVICE_LOG_STREAM,
            ),
        ]
        .into_iter()
        .filter_map(|(is_missing, capability)| is_missing.then_some(capability))
    }

    /// Promote the service family only when all five independent lanes exist.
    #[must_use]
    pub fn try_complete(self) -> Option<ServiceRuntimeLanes> {
        let Self {
            inventory_rx: Some(inventory),
            dependencies_rx: Some(dependencies),
            control_rx: Some(control),
            log_snapshot_rx: Some(log_snapshot),
            log_stream_rx: Some(log_stream),
        } = self
        else {
            return None;
        };
        Some(ServiceRuntimeLanes {
            inventory,
            dependencies,
            control,
            log_snapshot,
            log_stream,
        })
    }
}

/// Complete provider-side receivers for the service capability family.
pub struct ServiceRuntimeLanes {
    inventory: Receiver<Queued<ServiceInventoryRequest>>,
    dependencies: Receiver<Queued<ServiceDependenciesRequest>>,
    control: Receiver<Queued<ServiceControlRequest>>,
    log_snapshot: Receiver<Queued<ServiceLogSnapshotRequest>>,
    log_stream: Receiver<Queued<ServiceLogStreamRequest>>,
}

/// Attach all service executors to their independent typed lanes.
pub fn spawn_service_lanes(
    workers: &WorkerRuntime,
    lanes: ServiceRuntimeLanes,
    executors: ServiceExecutors,
    events: Arc<RuntimeEventPublisher>,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError> {
    let ServiceRuntimeLanes {
        inventory,
        dependencies,
        control,
        log_snapshot,
        log_stream,
    } = lanes;
    let ServiceExecutors {
        inventory: mut execute_inventory,
        dependencies: mut execute_dependencies,
        control: mut execute_control,
        log_snapshot: mut execute_log_snapshot,
        log_stream: mut execute_log_stream,
    } = executors;

    spawn_observation_lane(
        workers,
        inventory,
        events.clone(),
        move |ServiceInventoryRequest::Refresh| execute_inventory(),
        |snapshot| PlatformEvent::Services(ServiceEvent::Snapshot(snapshot)),
    )?;
    spawn_typed_outcome_lane(
        workers,
        dependencies,
        events.clone(),
        move |request_id, request| dependency_event(request_id, request, &mut execute_dependencies),
    )?;
    spawn_typed_outcome_lane(workers, control, events.clone(), move |_, request| {
        control_event(request, &mut execute_control)
    })?;
    spawn_typed_outcome_lane(workers, log_snapshot, events.clone(), move |_, request| {
        log_snapshot_event(request, &mut execute_log_snapshot)
    })?;
    spawn_typed_outcome_lane(workers, log_stream, events, move |request_id, request| {
        log_stream_event(request_id, request, &mut execute_log_stream, clock_ms())
    })
}

fn dependency_event(
    request_id: taskmanager_application::RequestId,
    ServiceDependenciesRequest { service_id }: ServiceDependenciesRequest,
    execute: &mut DependenciesExecutor,
) -> (PlatformEvent, Result<(), ProviderFailure>) {
    let result = execute(service_id.clone());
    let provider_result = result.as_ref().map(|_| ()).map_err(|error| *error);
    let update = match result {
        Ok(deps) => ServiceUpdate::Dependencies {
            request_id,
            service_id,
            deps,
        },
        Err(error) => ServiceUpdate::DependenciesUnavailable {
            request_id,
            service_id,
            error: error.kind(),
        },
    };
    (
        PlatformEvent::Services(ServiceEvent::Update(update)),
        provider_result,
    )
}

fn control_event(
    ServiceControlRequest {
        request_id,
        service_id,
        action,
    }: ServiceControlRequest,
    execute: &mut ControlExecutor,
) -> (PlatformEvent, Result<(), ProviderFailure>) {
    let result = execute(service_id.clone(), action);
    (
        PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::Action(
            ServiceControlOutcome {
                request_id,
                service_id,
                action,
                result: result.map_err(ProviderFailure::kind),
            },
        ))),
        result,
    )
}

fn log_snapshot_event(
    ServiceLogSnapshotRequest { service_id }: ServiceLogSnapshotRequest,
    execute: &mut LogSnapshotExecutor,
) -> (PlatformEvent, Result<(), ProviderFailure>) {
    let result = execute(service_id.clone());
    let provider_result = snapshot_provider_result(&result);
    let state = result.unwrap_or_else(|error| {
        ServiceLogState::Unavailable(ServiceLogFailure::with_detail(
            ServiceLogErrorKind::from_failure(error.kind()),
            format!("service log snapshot executor failed: {error:?}"),
        ))
    });
    (
        PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::Logs(
            ServiceLogSnapshot { service_id, state },
        ))),
        provider_result,
    )
}

fn log_stream_event(
    request_id: taskmanager_application::RequestId,
    ServiceLogStreamRequest { query }: ServiceLogStreamRequest,
    execute: &mut LogStreamExecutor,
    observed_at_ms: u64,
) -> (PlatformEvent, Result<(), ProviderFailure>) {
    let result = execute(query.clone(), observed_at_ms);
    let provider_result = stream_provider_result(&result);
    let state = result.unwrap_or_else(|error| {
        ServiceLogStreamState::Unavailable(ServiceLogFailure::with_detail(
            ServiceLogErrorKind::from_failure(error.kind()),
            format!("service log stream executor failed: {error:?}"),
        ))
    });
    (
        PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::LogStream {
            request_id,
            observed_at_ms,
            snapshot: ServiceLogStreamSnapshot { query, state },
        })),
        provider_result,
    )
}

fn snapshot_provider_result(
    result: &Result<ServiceLogState, ProviderFailure>,
) -> Result<(), ProviderFailure> {
    match result {
        Ok(ServiceLogState::Unavailable(failure)) => Err(log_provider_failure(failure.kind)),
        Ok(_) => Ok(()),
        Err(error) => Err(*error),
    }
}

fn stream_provider_result(
    result: &Result<ServiceLogStreamState, ProviderFailure>,
) -> Result<(), ProviderFailure> {
    match result {
        Ok(ServiceLogStreamState::Unavailable(failure)) => Err(log_provider_failure(failure.kind)),
        Ok(ServiceLogStreamState::Ended(ServiceLogStreamEnd::Disconnected { .. })) => {
            Err(ProviderFailure::TemporarilyUnavailable)
        }
        Ok(_) => Ok(()),
        Err(error) => Err(*error),
    }
}

const fn log_provider_failure(kind: ServiceLogErrorKind) -> ProviderFailure {
    match kind {
        ServiceLogErrorKind::MissingTool => ProviderFailure::MissingDependency,
        ServiceLogErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
        ServiceLogErrorKind::Unsupported => ProviderFailure::Unsupported,
        ServiceLogErrorKind::TimedOut => ProviderFailure::TimedOut,
        ServiceLogErrorKind::TemporarilyUnavailable => ProviderFailure::TemporarilyUnavailable,
        ServiceLogErrorKind::ProviderFailed => ProviderFailure::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../tests/headless/service.rs"]
mod tests;

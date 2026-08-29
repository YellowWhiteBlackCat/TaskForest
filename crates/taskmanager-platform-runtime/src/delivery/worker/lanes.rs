//! Generic provider-lane loops shared by native runtime domains.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::PlatformEvent;
use taskmanager_platform_contract::{CapabilityId, CapabilityRequest, ProviderFailure, RequestId};

use super::registry::{recv_or_shutdown_with_idle, spawn_or_register_lane};
use super::{
    LaneExitGuard, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, execute_isolated,
    panic_context, shutdown_requested, worker_name,
};
use crate::channel::Queued;
use crate::health::{CapabilityHealth, ObservationHealth};

/// Attach one independently blocking provider lane to a bounded request lane.
pub fn spawn_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(R) -> Result<PlatformEvent, ProviderFailure> + Send + 'static,
{
    spawn_lane_impl(workers, receiver, publisher, execute, None)
}

/// Attach one provider lane to the shared production lazy-start registry.
pub(crate) fn spawn_lazy_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: CapabilityRequest + Send + 'static,
    F: FnMut(R) -> Result<PlatformEvent, ProviderFailure> + Send + 'static,
{
    spawn_lane_impl(
        workers,
        receiver,
        publisher,
        execute,
        Some(R::CAPABILITY.clone()),
    )
}

fn spawn_lane_impl<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
    capability: Option<CapabilityId>,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(R) -> Result<PlatformEvent, ProviderFailure> + Send + 'static,
{
    let lane = worker_name::<R>();
    spawn_or_register_lane(
        workers,
        capability,
        receiver,
        publisher,
        execute,
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                let result = execute_isolated(&panic_notes, panic_context(&lane, &queued), || {
                    let mut execute = execute
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    execute(queued.payload)
                });
                if shutdown_requested(&shutdown)
                    || publisher
                        .publish(
                            queued.request_id,
                            queued.capability,
                            queued.provider,
                            result,
                        )
                        .is_stop()
                {
                    break;
                }
            }
        },
    )
}

/// Attach a source-rich observation provider whose execution result and
/// observation health are independent.
pub fn spawn_health_observation_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(R) -> Result<(PlatformEvent, CapabilityHealth), ProviderFailure> + Send + 'static,
{
    spawn_health_observation_lane_impl(workers, receiver, publisher, execute, None)
}

pub(crate) fn spawn_lazy_health_observation_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: CapabilityRequest + Send + 'static,
    F: FnMut(R) -> Result<(PlatformEvent, CapabilityHealth), ProviderFailure> + Send + 'static,
{
    spawn_health_observation_lane_impl(
        workers,
        receiver,
        publisher,
        execute,
        Some(R::CAPABILITY.clone()),
    )
}

fn spawn_health_observation_lane_impl<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
    capability: Option<CapabilityId>,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(R) -> Result<(PlatformEvent, CapabilityHealth), ProviderFailure> + Send + 'static,
{
    let lane = worker_name::<R>();
    spawn_or_register_lane(
        workers,
        capability,
        receiver,
        publisher,
        execute,
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                let publication =
                    execute_isolated(&panic_notes, panic_context(&lane, &queued), || {
                        let mut execute = execute
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        execute(queued.payload)
                    });
                if shutdown_requested(&shutdown) {
                    break;
                }
                let published = match publication {
                    Ok((event, health)) => publisher.publish_health(
                        queued.request_id,
                        queued.capability,
                        queued.provider,
                        event,
                        health,
                    ),
                    Err(failure) => publisher.publish(
                        queued.request_id,
                        queued.capability,
                        queued.provider,
                        Err(failure),
                    ),
                };
                if published.is_stop() {
                    break;
                }
            }
        },
    )
}

/// Attach a typed observation provider whose health is derived from the exact
/// snapshot mapped into the published domain event.
pub fn spawn_observation_lane<R, S, F, M>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    observe: F,
    map_event: M,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    S: ObservationHealth + Send + 'static,
    F: FnMut(R) -> Result<S, ProviderFailure> + Send + 'static,
    M: Fn(S) -> PlatformEvent + Send + 'static,
{
    spawn_observation_lane_impl(workers, receiver, publisher, observe, map_event, None)
}

pub(crate) fn spawn_lazy_observation_lane<R, S, F, M>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    observe: F,
    map_event: M,
) -> Result<(), WorkerSpawnError>
where
    R: CapabilityRequest + Send + 'static,
    S: ObservationHealth + Send + 'static,
    F: FnMut(R) -> Result<S, ProviderFailure> + Send + 'static,
    M: Fn(S) -> PlatformEvent + Send + 'static,
{
    spawn_observation_lane_impl(
        workers,
        receiver,
        publisher,
        observe,
        map_event,
        Some(R::CAPABILITY.clone()),
    )
}

fn spawn_observation_lane_impl<R, S, F, M>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    observe: F,
    map_event: M,
    capability: Option<CapabilityId>,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    S: ObservationHealth + Send + 'static,
    F: FnMut(R) -> Result<S, ProviderFailure> + Send + 'static,
    M: Fn(S) -> PlatformEvent + Send + 'static,
{
    let lane = worker_name::<R>();
    spawn_or_register_lane(
        workers,
        capability,
        receiver,
        publisher,
        (observe, map_event),
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                // The observation health and event mapping run inside the same
                // isolation boundary as the observation itself.
                let publication =
                    execute_isolated(&panic_notes, panic_context(&lane, &queued), || {
                        let (observe, map_event) = &mut *execute
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let snapshot = observe(queued.payload)?;
                        let health = snapshot.observation_health();
                        let event = map_event(snapshot);
                        Ok((event, health))
                    });
                if shutdown_requested(&shutdown) {
                    break;
                }
                let published = match publication {
                    Ok((event, health)) => publisher.publish_health(
                        queued.request_id,
                        queued.capability,
                        queued.provider,
                        event,
                        health,
                    ),
                    Err(failure) => publisher.publish(
                        queued.request_id,
                        queued.capability,
                        queued.provider,
                        Err(failure),
                    ),
                };
                if published.is_stop() {
                    break;
                }
            }
        },
    )
}

/// Attach a provider closure whose domain event carries its own typed outcome.
pub fn spawn_typed_outcome_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(RequestId, R) -> (PlatformEvent, Result<(), ProviderFailure>) + Send + 'static,
{
    spawn_typed_outcome_lane_impl(workers, receiver, publisher, execute, None)
}

pub(crate) fn spawn_lazy_typed_outcome_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: CapabilityRequest + Send + 'static,
    F: FnMut(RequestId, R) -> (PlatformEvent, Result<(), ProviderFailure>) + Send + 'static,
{
    spawn_typed_outcome_lane_impl(
        workers,
        receiver,
        publisher,
        execute,
        Some(R::CAPABILITY.clone()),
    )
}

fn spawn_typed_outcome_lane_impl<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
    capability: Option<CapabilityId>,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(RequestId, R) -> (PlatformEvent, Result<(), ProviderFailure>) + Send + 'static,
{
    let lane = worker_name::<R>();
    spawn_or_register_lane(
        workers,
        capability,
        receiver,
        publisher,
        execute,
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                // The typed provider result is wrapped in `Ok` so the event
                // and its exact outcome survive the common panic seam.
                let publication =
                    execute_isolated(&panic_notes, panic_context(&lane, &queued), || {
                        let mut execute = execute
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        Ok(execute(queued.request_id, queued.payload))
                    });
                if shutdown_requested(&shutdown) {
                    break;
                }
                let published = match publication {
                    Ok((event, provider_result)) => publisher.publish_typed_outcome(
                        queued.request_id,
                        queued.capability,
                        queued.provider,
                        event,
                        provider_result,
                    ),
                    Err(failure) => publisher.publish(
                        queued.request_id,
                        queued.capability,
                        queued.provider,
                        Err(failure),
                    ),
                };
                if published.is_stop() {
                    break;
                }
            }
        },
    )
}

//! Capability health and fair correlated event delivery.

mod catalog;
mod event_port;
pub(crate) mod event_queue;
mod publisher;
#[cfg(test)]
#[path = "../tests/headless/delivery/terminal_mailbox.rs"]
mod terminal_mailbox_tests;
#[cfg(test)]
#[path = "../tests/headless/delivery.rs"]
mod tests;
mod worker;

pub(crate) use worker::{execute_isolated, recv_or_shutdown_with_idle, shutdown_requested};

pub(crate) use catalog::{ProviderPanicContext, RuntimeCapabilityCatalog};
pub(crate) use event_port::FairEventPort;
pub(crate) use event_queue::EventQueueState;
pub use publisher::{LaneFlow, RuntimeEventPublisher};
pub use worker::{
    DEFAULT_WORKER_LIMIT, PROCESS_WORKER_LIMIT, WorkerRuntime, WorkerSpawnError,
    spawn_health_observation_lane, spawn_lane, spawn_observation_lane, spawn_typed_outcome_lane,
};

pub(crate) use worker::LaneExitGuard;
pub(crate) use worker::LaneStartRegistry;
pub(crate) use worker::{
    spawn_lazy_health_observation_lane, spawn_lazy_lane, spawn_lazy_observation_lane,
    spawn_lazy_typed_outcome_lane, spawn_or_register_lane,
};

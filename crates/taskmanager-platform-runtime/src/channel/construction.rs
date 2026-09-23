//! Channel runtime construction: bounded request ports, fair event delivery,
//! the capability catalog, and provider-side execution lanes.
//!
//! Built once from explicit native adapter bindings via `ChannelRuntime::try_new`.
//! The per-facet-domain lane/facet helpers live in the sibling `construction/`
//! submodules; this root owns catalog/event setup and orchestration.

use std::sync::Arc;

use crossbeam_channel::bounded;
use taskmanager_application::{PlatformFacets, PlatformHandle};

use super::lanes::RuntimeLanes;
use crate::config::{DeliveryClass, RuntimeBudgets, RuntimeConfig, RuntimeProviderBindings};
use crate::delivery::LaneStartRegistry;
use crate::delivery::{
    EventQueueState, FairEventPort, RuntimeCapabilityCatalog, RuntimeEventPublisher,
};

mod budget;
use budget::validate_runtime_config;
pub use budget::{RuntimeBudgetField, RuntimeConstructionError};

mod context;
mod facets;
mod process;
mod system;

use context::LaneContext;
use facets::{
    environment_runtime, integration_runtime, power_runtime, sensor_runtime, service_runtime,
    storage_runtime,
};
use process::process_runtime;
use system::system_runtime;

/// Complete reusable channel runtime plus its provider-side execution lanes.
pub struct ChannelRuntime {
    pub handle: PlatformHandle,
    pub publisher: Arc<RuntimeEventPublisher>,
    pub lanes: RuntimeLanes,
    pub(crate) lane_starters: Arc<LaneStartRegistry>,
}

impl ChannelRuntime {
    /// Construct bounded application ports, correlated event delivery, and the
    /// capability catalog from explicit native adapter bindings.
    fn build(bindings: RuntimeProviderBindings, config: RuntimeConfig) -> Self {
        let (routes, initial_statuses) = bindings.routes_with_initial_statuses();
        let control_capabilities: Vec<_> = routes
            .iter()
            .filter(|route| route.delivery == DeliveryClass::Control)
            .map(|route| route.capability.clone())
            .collect();
        let (capabilities, event_queues) = if config.budgets == RuntimeBudgets::DEFAULT {
            let capabilities = Arc::new(RuntimeCapabilityCatalog::new(
                &routes,
                config.monotonic_clock_ms,
            ));
            capabilities.seed_initial_statuses(&initial_statuses);
            let event_queues = capabilities.event_queue_state();
            (capabilities, event_queues)
        } else {
            let event_queues =
                Arc::new(EventQueueState::new(config.budgets.pending_delivery_limit));
            let capabilities = Arc::new(RuntimeCapabilityCatalog::with_resources(
                &routes,
                config.monotonic_clock_ms,
                config.budgets,
                event_queues.clone(),
            ));
            capabilities.seed_initial_statuses(&initial_statuses);
            (capabilities, event_queues)
        };
        let lane_starters = Arc::new(LaneStartRegistry::default());
        let context = LaneContext {
            observation_capacity: config.queues.observation_requests,
            control_capacity: config.queues.control_requests,
            ecs_scheduler: capabilities.ecs_scheduler_handle(),
            lane_starters: lane_starters.clone(),
        };

        let (system, system_lanes) = system_runtime(&bindings.system, &context);
        let (process, process_lanes) = process_runtime(&bindings.process, &context);
        let (service, service_lanes) = service_runtime(&bindings.service, &context);
        let (environment, environment_lanes) = environment_runtime(&bindings.environment, &context);
        let (integration, integration_lanes) = integration_runtime(&bindings.integration, &context);
        let (storage, storage_lanes) = storage_runtime(&bindings.storage, &context);
        let (sensor, sensor_lanes) = sensor_runtime(&bindings.sensor, &context);
        let (power, power_lanes) = power_runtime(&bindings.power, &context);

        let (control_event_tx, control_event_rx) = bounded(config.queues.control_events);
        let (observation_event_tx, observation_event_rx) =
            bounded(config.queues.observation_events);
        let publisher = Arc::new(if config.budgets == RuntimeBudgets::DEFAULT {
            RuntimeEventPublisher::new(
                control_event_tx,
                observation_event_tx,
                capabilities.clone(),
                control_capabilities,
                config.clock_ms,
            )
        } else {
            RuntimeEventPublisher::with_event_queues(
                control_event_tx,
                observation_event_tx,
                event_queues.clone(),
                capabilities.clone(),
                control_capabilities,
                config.clock_ms,
            )
        });
        let events = Arc::new(FairEventPort::new(
            control_event_rx,
            observation_event_rx,
            event_queues,
            capabilities.clone(),
        ));

        let facets = PlatformFacets::default()
            .with_system(system)
            .with_process(process)
            .with_service(service)
            .with_environment(environment)
            .with_integration(integration)
            .with_storage(storage)
            .with_sensor(sensor)
            .with_power(power);
        let handle = PlatformHandle::new(capabilities.clone(), events, facets);
        let handle = handle.with_scheduler(capabilities);

        Self {
            handle,
            publisher,
            lane_starters,
            lanes: RuntimeLanes {
                system: system_lanes,
                process: process_lanes,
                service: service_lanes,
                environment: environment_lanes,
                integration: integration_lanes,
                storage: storage_lanes,
                sensor: sensor_lanes,
                power: power_lanes,
            },
        }
    }

    pub fn try_new(
        bindings: RuntimeProviderBindings,
        config: RuntimeConfig,
    ) -> Result<Self, RuntimeConstructionError> {
        validate_runtime_config(&bindings, config)?;
        Ok(Self::build(bindings, config))
    }
}

#[cfg(test)]
#[path = "../../tests/headless/runtime_channel_construction_tests.rs"]
mod tests;

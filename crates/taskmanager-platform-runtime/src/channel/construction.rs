//! Channel runtime construction: bounded request ports, fair event delivery,
//! the capability catalog, and provider-side execution lanes.
//!
//! Built once from explicit native adapter bindings via `ChannelRuntime::try_new`.

use std::sync::Arc;

use crossbeam_channel::bounded;
use taskmanager_application::{
    EnvironmentFacets, IntegrationFacets, PlatformFacets, PlatformHandle, PowerFacets,
    ProcessFacets, SensorFacets, ServiceFacets, StorageFacets, SystemFacets,
};

use super::lanes::RuntimeLanes;
use super::port::request_lane;
use crate::config::{DeliveryClass, RuntimeBudgets, RuntimeConfig, RuntimeProviderBindings};
use crate::delivery::{
    EventQueueState, FairEventPort, RuntimeCapabilityCatalog, RuntimeEventPublisher,
};
use crate::environment::PendingEnvironmentRuntimeLanes;
use crate::integration::PendingIntegrationRuntimeLanes;
use crate::power::PendingPowerRuntimeLanes;
use crate::process::{
    PendingProcessControlLanes, PendingProcessObservationLanes, PendingProcessRuntimeLanes,
};
use crate::sensor::PendingSensorRuntimeLanes;
use crate::service::PendingServiceRuntimeLanes;
use crate::storage::PendingStorageRuntimeLanes;
use crate::system::PendingSystemRuntimeLanes;

mod budget;
use budget::validate_runtime_config;
pub use budget::{RuntimeBudgetField, RuntimeConstructionError};

/// Complete reusable channel runtime plus its provider-side execution lanes.
pub struct ChannelRuntime {
    pub handle: PlatformHandle,
    pub publisher: Arc<RuntimeEventPublisher>,
    pub lanes: RuntimeLanes,
}

impl ChannelRuntime {
    /// Construct bounded application ports, correlated event delivery, and the
    /// capability catalog from explicit native adapter bindings.
    fn build(bindings: RuntimeProviderBindings, config: RuntimeConfig) -> Self {
        let observation_capacity = config.queues.observation_requests;
        let control_capacity = config.queues.control_requests;
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
        let ecs_scheduler = capabilities.ecs_scheduler_handle();

        let (host_telemetry_port, host_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.host.as_ref(),
            ecs_scheduler.clone(),
        );
        let (cpu_telemetry_port, cpu_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.cpu.as_ref(),
            ecs_scheduler.clone(),
        );
        let (memory_telemetry_port, memory_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.memory.as_ref(),
            ecs_scheduler.clone(),
        );
        let (storage_telemetry_port, storage_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.storage.as_ref(),
            ecs_scheduler.clone(),
        );
        let (network_telemetry_port, network_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.network.as_ref(),
            ecs_scheduler.clone(),
        );
        let (gpu_telemetry_port, gpu_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.gpu.as_ref(),
            ecs_scheduler.clone(),
        );
        let (hardware_inventory_port, hardware_inventory_rx) = request_lane(
            observation_capacity,
            bindings.system.hardware_inventory.as_ref(),
            ecs_scheduler.clone(),
        );
        let (containers_port, containers_rx) = request_lane(
            observation_capacity,
            bindings.system.containers.as_ref(),
            ecs_scheduler.clone(),
        );
        let (gpu_engine_rows_port, gpu_engine_rows_rx) = request_lane(
            observation_capacity,
            bindings.system.gpu_engine_rows.as_ref(),
            ecs_scheduler.clone(),
        );
        let (npu_inventory_port, npu_inventory_rx) = request_lane(
            observation_capacity,
            bindings.system.npu_inventory.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_list_port, process_list_rx) = request_lane(
            observation_capacity,
            bindings.process.list.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_network_port, process_network_rx) = request_lane(
            observation_capacity,
            bindings.process.network.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_gpu_port, process_gpu_rx) = request_lane(
            observation_capacity,
            bindings.process.gpu.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_resources_port, process_resources_rx) = request_lane(
            observation_capacity,
            bindings.process.resources.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_isolation_port, process_isolation_rx) = request_lane(
            observation_capacity,
            bindings.process.isolation.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_threads_port, process_threads_rx) = request_lane(
            observation_capacity,
            bindings.process.threads.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_open_files_port, process_open_files_rx) = request_lane(
            observation_capacity,
            bindings.process.open_files.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_environment_port, process_environment_rx) = request_lane(
            observation_capacity,
            bindings.process.environment.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_affinity_port, process_affinity_rx) = request_lane(
            observation_capacity,
            bindings.process.affinity.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_affinity_control_port, process_affinity_control_rx) = request_lane(
            control_capacity,
            bindings.process.affinity_control.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_resource_control_port, process_resource_control_rx) = request_lane(
            control_capacity,
            bindings.process.resource_control.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_network_escalation_port, process_network_escalation_rx) = request_lane(
            control_capacity,
            bindings.process.network_escalation.as_ref(),
            ecs_scheduler.clone(),
        );
        let (process_control_port, process_control_rx) = request_lane(
            control_capacity,
            bindings.process.control.as_ref(),
            ecs_scheduler.clone(),
        );
        let (service_inventory_port, service_inventory_rx) = request_lane(
            observation_capacity,
            bindings.service.inventory.as_ref(),
            ecs_scheduler.clone(),
        );
        let (service_dependencies_port, service_dependencies_rx) = request_lane(
            observation_capacity,
            bindings.service.dependencies.as_ref(),
            ecs_scheduler.clone(),
        );
        let (service_control_port, service_control_rx) = request_lane(
            control_capacity,
            bindings.service.control.as_ref(),
            ecs_scheduler.clone(),
        );
        let (service_log_snapshot_port, service_log_snapshot_rx) = request_lane(
            observation_capacity,
            bindings.service.log_snapshot.as_ref(),
            ecs_scheduler.clone(),
        );
        let (service_log_stream_port, service_log_stream_rx) = request_lane(
            observation_capacity,
            bindings.service.log_stream.as_ref(),
            ecs_scheduler.clone(),
        );
        let (startup_inventory_port, startup_inventory_rx) = request_lane(
            observation_capacity,
            bindings.environment.startup_inventory.as_ref(),
            ecs_scheduler.clone(),
        );
        let (startup_evidence_port, startup_evidence_rx) = request_lane(
            observation_capacity,
            bindings.environment.startup_evidence.as_ref(),
            ecs_scheduler.clone(),
        );
        let (startup_control_port, startup_control_rx) = request_lane(
            control_capacity,
            bindings.environment.startup_control.as_ref(),
            ecs_scheduler.clone(),
        );
        let (session_inventory_port, session_inventory_rx) = request_lane(
            observation_capacity,
            bindings.environment.session_inventory.as_ref(),
            ecs_scheduler.clone(),
        );
        let (session_control_port, session_control_rx) = request_lane(
            control_capacity,
            bindings.environment.session_control.as_ref(),
            ecs_scheduler.clone(),
        );
        let (command_launch_port, command_launch_rx) = request_lane(
            control_capacity,
            bindings.integration.command_launch.as_ref(),
            ecs_scheduler.clone(),
        );
        let (resource_reveal_port, resource_reveal_rx) = request_lane(
            control_capacity,
            bindings.integration.resource_reveal.as_ref(),
            ecs_scheduler.clone(),
        );
        let (url_open_port, url_open_rx) = request_lane(
            control_capacity,
            bindings.integration.url_open.as_ref(),
            ecs_scheduler.clone(),
        );
        let (desktop_appearance_port, desktop_appearance_rx) = request_lane(
            observation_capacity,
            bindings.integration.desktop_appearance.as_ref(),
            ecs_scheduler.clone(),
        );
        let (setup_script_port, setup_script_rx) = request_lane(
            control_capacity,
            bindings.integration.setup_script.as_ref(),
            ecs_scheduler.clone(),
        );
        let (desktop_notification_port, desktop_notification_rx) = request_lane(
            control_capacity,
            bindings.integration.desktop_notification.as_ref(),
            ecs_scheduler.clone(),
        );
        let (storage_health_port, storage_health_rx) = request_lane(
            observation_capacity,
            bindings.storage.health.as_ref(),
            ecs_scheduler.clone(),
        );
        let (sensor_port, sensor_rx) = request_lane(
            observation_capacity,
            bindings.sensor.observation.as_ref(),
            ecs_scheduler.clone(),
        );
        let (power_supply_port, power_supply_rx) = request_lane(
            observation_capacity,
            bindings.power.supplies.as_ref(),
            ecs_scheduler.clone(),
        );
        let (smart_observation_port, smart_observation_rx) = request_lane(
            observation_capacity,
            bindings.storage.smart_observation.as_ref(),
            ecs_scheduler.clone(),
        );
        let (smart_control_port, smart_control_rx) = request_lane(
            control_capacity,
            bindings.storage.smart_control.as_ref(),
            ecs_scheduler.clone(),
        );
        let (directory_usage_port, directory_usage_rx) = request_lane(
            observation_capacity,
            bindings.storage.directory_usage.as_ref(),
            ecs_scheduler.clone(),
        );

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

        let mut system = SystemFacets::default();
        if let Some(port) = host_telemetry_port {
            system = system.with_host(port);
        }
        if let Some(port) = cpu_telemetry_port {
            system = system.with_cpu(port);
        }
        if let Some(port) = memory_telemetry_port {
            system = system.with_memory(port);
        }
        if let Some(port) = storage_telemetry_port {
            system = system.with_storage(port);
        }
        if let Some(port) = network_telemetry_port {
            system = system.with_network(port);
        }
        if let Some(port) = gpu_telemetry_port {
            system = system.with_gpu(port);
        }
        if let Some(port) = hardware_inventory_port {
            system = system.with_hardware_inventory(port);
        }
        if let Some(port) = containers_port {
            system = system.with_containers(port);
        }
        if let Some(port) = gpu_engine_rows_port {
            system = system.with_gpu_engine_rows(port);
        }
        if let Some(port) = npu_inventory_port {
            system = system.with_npu_inventory(port);
        }

        let mut process = ProcessFacets::default();
        if let Some(port) = process_list_port {
            process = process.with_list(port);
        }
        if let Some(port) = process_control_port {
            process = process.with_control(port);
        }
        if let Some(port) = process_network_port {
            process = process.with_network(port);
        }
        if let Some(port) = process_gpu_port {
            process = process.with_gpu(port);
        }
        if let Some(port) = process_resources_port {
            process = process.with_resources(port);
        }
        if let Some(port) = process_isolation_port {
            process = process.with_isolation(port);
        }
        if let Some(port) = process_threads_port {
            process = process.with_threads(port);
        }
        if let Some(port) = process_open_files_port {
            process = process.with_open_files(port);
        }
        if let Some(port) = process_environment_port {
            process = process.with_environment(port);
        }
        if let Some(port) = process_affinity_port {
            process = process.with_affinity(port);
        }
        if let Some(port) = process_affinity_control_port {
            process = process.with_affinity_control(port);
        }
        if let Some(port) = process_resource_control_port {
            process = process.with_resource_control(port);
        }
        if let Some(port) = process_network_escalation_port {
            process = process.with_network_escalation(port);
        }

        let mut service = ServiceFacets::default();
        if let Some(port) = service_inventory_port {
            service = service.with_inventory(port);
        }
        if let Some(port) = service_dependencies_port {
            service = service.with_dependencies(port);
        }
        if let Some(port) = service_control_port {
            service = service.with_control(port);
        }
        if let Some(port) = service_log_snapshot_port {
            service = service.with_log_snapshot(port);
        }
        if let Some(port) = service_log_stream_port {
            service = service.with_log_stream(port);
        }

        let mut environment = EnvironmentFacets::default();
        if let Some(port) = startup_inventory_port {
            environment = environment.with_startup_inventory(port);
        }
        if let Some(port) = startup_evidence_port {
            environment = environment.with_startup_evidence(port);
        }
        if let Some(port) = startup_control_port {
            environment = environment.with_startup_control(port);
        }
        if let Some(port) = session_inventory_port {
            environment = environment.with_session_inventory(port);
        }
        if let Some(port) = session_control_port {
            environment = environment.with_session_control(port);
        }

        let mut integration = IntegrationFacets::default();
        if let Some(port) = command_launch_port {
            integration = integration.with_command_launch(port);
        }
        if let Some(port) = resource_reveal_port {
            integration = integration.with_resource_reveal(port);
        }
        if let Some(port) = url_open_port {
            integration = integration.with_url_open(port);
        }
        if let Some(port) = desktop_appearance_port {
            integration = integration.with_desktop_appearance(port);
        }
        if let Some(port) = setup_script_port {
            integration = integration.with_setup_script(port);
        }
        if let Some(port) = desktop_notification_port {
            integration = integration.with_desktop_notification(port);
        }

        let mut storage = StorageFacets::default();
        if let Some(port) = storage_health_port {
            storage = storage.with_health(port);
        }
        if let Some(port) = smart_observation_port {
            storage = storage.with_smart_observation(port);
        }
        if let Some(port) = smart_control_port {
            storage = storage.with_smart_control(port);
        }
        if let Some(port) = directory_usage_port {
            storage = storage.with_directory_usage(port);
        }

        let mut sensor = SensorFacets::default();
        if let Some(port) = sensor_port {
            sensor = sensor.with_observation(port);
        }
        let mut power = PowerFacets::default();
        if let Some(port) = power_supply_port {
            power = power.with_supplies(port);
        }
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
            lanes: RuntimeLanes {
                system: PendingSystemRuntimeLanes::new(
                    crate::PendingSystemObservationLanes::new(
                        host_telemetry_rx,
                        cpu_telemetry_rx,
                        memory_telemetry_rx,
                        storage_telemetry_rx,
                        network_telemetry_rx,
                        gpu_telemetry_rx,
                        containers_rx,
                    ),
                    crate::PendingSystemAuxiliaryLanes::new(
                        hardware_inventory_rx,
                        gpu_engine_rows_rx,
                        npu_inventory_rx,
                    ),
                ),
                process: PendingProcessRuntimeLanes::new(
                    PendingProcessObservationLanes {
                        list_rx: process_list_rx,
                        network_rx: process_network_rx,
                        gpu_rx: process_gpu_rx,
                        resources_rx: process_resources_rx,
                        isolation_rx: process_isolation_rx,
                        threads_rx: process_threads_rx,
                        affinity_rx: process_affinity_rx,
                        open_files_rx: process_open_files_rx,
                        environment_rx: process_environment_rx,
                    },
                    PendingProcessControlLanes::new(
                        process_affinity_control_rx,
                        process_resource_control_rx,
                        process_control_rx,
                        process_network_escalation_rx,
                    ),
                ),
                service: PendingServiceRuntimeLanes::new(
                    service_inventory_rx,
                    service_dependencies_rx,
                    service_control_rx,
                    service_log_snapshot_rx,
                    service_log_stream_rx,
                ),
                environment: PendingEnvironmentRuntimeLanes::new(
                    startup_inventory_rx,
                    startup_evidence_rx,
                    startup_control_rx,
                    session_inventory_rx,
                    session_control_rx,
                ),
                integration: PendingIntegrationRuntimeLanes::new(
                    command_launch_rx,
                    resource_reveal_rx,
                    url_open_rx,
                    desktop_appearance_rx,
                    desktop_notification_rx,
                    setup_script_rx,
                ),
                storage: PendingStorageRuntimeLanes::new(
                    storage_health_rx,
                    smart_observation_rx,
                    smart_control_rx,
                    directory_usage_rx,
                ),
                sensor: PendingSensorRuntimeLanes::new(sensor_rx),
                power: PendingPowerRuntimeLanes::new(power_supply_rx),
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

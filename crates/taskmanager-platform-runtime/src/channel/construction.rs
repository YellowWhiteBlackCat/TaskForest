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
use super::port::{ChannelRequestPort, request_lane};
use crate::config::{DeliveryClass, RuntimeBudgets, RuntimeConfig, RuntimeProviderBindings};
use crate::delivery::LaneStartRegistry;
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
use taskmanager_platform_contract::CapabilityRequest;

mod budget;
use budget::validate_runtime_config;
pub use budget::{RuntimeBudgetField, RuntimeConstructionError};

/// Complete reusable channel runtime plus its provider-side execution lanes.
pub struct ChannelRuntime {
    pub handle: PlatformHandle,
    pub publisher: Arc<RuntimeEventPublisher>,
    pub lanes: RuntimeLanes,
    pub(crate) lane_starters: Arc<LaneStartRegistry>,
}

fn attach_optional<R, F, T>(facets: T, port: Option<Arc<ChannelRequestPort<R>>>, attach: F) -> T
where
    R: CapabilityRequest,
    F: FnOnce(T, Arc<ChannelRequestPort<R>>) -> T,
{
    match port {
        Some(port) => attach(facets, port),
        None => facets,
    }
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
        let lane_starters = Arc::new(LaneStartRegistry::default());

        let (host_telemetry_port, host_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.host.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (cpu_telemetry_port, cpu_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.cpu.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (memory_telemetry_port, memory_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.memory.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (storage_telemetry_port, storage_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.storage.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (network_telemetry_port, network_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.network.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (gpu_telemetry_port, gpu_telemetry_rx) = request_lane(
            observation_capacity,
            bindings.system.gpu.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (hardware_inventory_port, hardware_inventory_rx) = request_lane(
            observation_capacity,
            bindings.system.hardware_inventory.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (containers_port, containers_rx) = request_lane(
            observation_capacity,
            bindings.system.containers.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (gpu_engine_rows_port, gpu_engine_rows_rx) = request_lane(
            observation_capacity,
            bindings.system.gpu_engine_rows.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (npu_inventory_port, npu_inventory_rx) = request_lane(
            observation_capacity,
            bindings.system.npu_inventory.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (smbios_memory_port, smbios_memory_rx) = request_lane(
            observation_capacity,
            bindings.system.smbios_memory.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (rapl_power_port, rapl_power_rx) = request_lane(
            observation_capacity,
            bindings.system.rapl_power.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (msr_readout_port, msr_readout_rx) = request_lane(
            observation_capacity,
            bindings.system.msr_readout.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_list_port, process_list_rx) = request_lane(
            observation_capacity,
            bindings.process.list.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_network_port, process_network_rx) = request_lane(
            observation_capacity,
            bindings.process.network.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_gpu_port, process_gpu_rx) = request_lane(
            observation_capacity,
            bindings.process.gpu.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_resources_port, process_resources_rx) = request_lane(
            observation_capacity,
            bindings.process.resources.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_isolation_port, process_isolation_rx) = request_lane(
            observation_capacity,
            bindings.process.isolation.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_threads_port, process_threads_rx) = request_lane(
            observation_capacity,
            bindings.process.threads.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_open_files_port, process_open_files_rx) = request_lane(
            observation_capacity,
            bindings.process.open_files.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_environment_port, process_environment_rx) = request_lane(
            observation_capacity,
            bindings.process.environment.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_affinity_port, process_affinity_rx) = request_lane(
            observation_capacity,
            bindings.process.affinity.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_affinity_control_port, process_affinity_control_rx) = request_lane(
            control_capacity,
            bindings.process.affinity_control.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_resource_control_port, process_resource_control_rx) = request_lane(
            control_capacity,
            bindings.process.resource_control.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_network_escalation_port, process_network_escalation_rx) = request_lane(
            control_capacity,
            bindings.process.network_escalation.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (process_control_port, process_control_rx) = request_lane(
            control_capacity,
            bindings.process.control.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (service_inventory_port, service_inventory_rx) = request_lane(
            observation_capacity,
            bindings.service.inventory.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (service_dependencies_port, service_dependencies_rx) = request_lane(
            observation_capacity,
            bindings.service.dependencies.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (service_control_port, service_control_rx) = request_lane(
            control_capacity,
            bindings.service.control.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (service_log_snapshot_port, service_log_snapshot_rx) = request_lane(
            observation_capacity,
            bindings.service.log_snapshot.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (service_log_stream_port, service_log_stream_rx) = request_lane(
            observation_capacity,
            bindings.service.log_stream.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (startup_inventory_port, startup_inventory_rx) = request_lane(
            observation_capacity,
            bindings.environment.startup_inventory.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (startup_evidence_port, startup_evidence_rx) = request_lane(
            observation_capacity,
            bindings.environment.startup_evidence.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (startup_control_port, startup_control_rx) = request_lane(
            control_capacity,
            bindings.environment.startup_control.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (session_inventory_port, session_inventory_rx) = request_lane(
            observation_capacity,
            bindings.environment.session_inventory.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (session_control_port, session_control_rx) = request_lane(
            control_capacity,
            bindings.environment.session_control.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (command_launch_port, command_launch_rx) = request_lane(
            control_capacity,
            bindings.integration.command_launch.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (resource_reveal_port, resource_reveal_rx) = request_lane(
            control_capacity,
            bindings.integration.resource_reveal.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (url_open_port, url_open_rx) = request_lane(
            control_capacity,
            bindings.integration.url_open.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (desktop_appearance_port, desktop_appearance_rx) = request_lane(
            observation_capacity,
            bindings.integration.desktop_appearance.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (setup_script_port, setup_script_rx) = request_lane(
            control_capacity,
            bindings.integration.setup_script.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (desktop_notification_port, desktop_notification_rx) = request_lane(
            control_capacity,
            bindings.integration.desktop_notification.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (storage_health_port, storage_health_rx) = request_lane(
            observation_capacity,
            bindings.storage.health.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (sensor_port, sensor_rx) = request_lane(
            observation_capacity,
            bindings.sensor.observation.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (power_supply_port, power_supply_rx) = request_lane(
            observation_capacity,
            bindings.power.supplies.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (smart_observation_port, smart_observation_rx) = request_lane(
            observation_capacity,
            bindings.storage.smart_observation.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (smart_control_port, smart_control_rx) = request_lane(
            control_capacity,
            bindings.storage.smart_control.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
        );
        let (directory_usage_port, directory_usage_rx) = request_lane(
            observation_capacity,
            bindings.storage.directory_usage.as_ref(),
            ecs_scheduler.clone(),
            lane_starters.clone(),
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

        let system = SystemFacets::default();
        let system = attach_optional(system, host_telemetry_port, |system, port| {
            system.with_host(port)
        });
        let system = attach_optional(system, cpu_telemetry_port, |system, port| {
            system.with_cpu(port)
        });
        let system = attach_optional(system, memory_telemetry_port, |system, port| {
            system.with_memory(port)
        });
        let system = attach_optional(system, storage_telemetry_port, |system, port| {
            system.with_storage(port)
        });
        let system = attach_optional(system, network_telemetry_port, |system, port| {
            system.with_network(port)
        });
        let system = attach_optional(system, gpu_telemetry_port, |system, port| {
            system.with_gpu(port)
        });
        let system = attach_optional(system, hardware_inventory_port, |system, port| {
            system.with_hardware_inventory(port)
        });
        let system = attach_optional(system, containers_port, |system, port| {
            system.with_containers(port)
        });
        let system = attach_optional(system, gpu_engine_rows_port, |system, port| {
            system.with_gpu_engine_rows(port)
        });
        let system = attach_optional(system, npu_inventory_port, |system, port| {
            system.with_npu_inventory(port)
        });
        let system = attach_optional(system, smbios_memory_port, |system, port| {
            system.with_smbios_memory(port)
        });
        let system = attach_optional(system, rapl_power_port, |system, port| {
            system.with_rapl_power(port)
        });
        let system = attach_optional(system, msr_readout_port, |system, port| {
            system.with_msr_readout(port)
        });

        let process = ProcessFacets::default();
        let process = attach_optional(process, process_list_port, |process, port| {
            process.with_list(port)
        });
        let process = attach_optional(process, process_control_port, |process, port| {
            process.with_control(port)
        });
        let process = attach_optional(process, process_network_port, |process, port| {
            process.with_network(port)
        });
        let process = attach_optional(process, process_gpu_port, |process, port| {
            process.with_gpu(port)
        });
        let process = attach_optional(process, process_resources_port, |process, port| {
            process.with_resources(port)
        });
        let process = attach_optional(process, process_isolation_port, |process, port| {
            process.with_isolation(port)
        });
        let process = attach_optional(process, process_threads_port, |process, port| {
            process.with_threads(port)
        });
        let process = attach_optional(process, process_open_files_port, |process, port| {
            process.with_open_files(port)
        });
        let process = attach_optional(process, process_environment_port, |process, port| {
            process.with_environment(port)
        });
        let process = attach_optional(process, process_affinity_port, |process, port| {
            process.with_affinity(port)
        });
        let process = attach_optional(process, process_affinity_control_port, |process, port| {
            process.with_affinity_control(port)
        });
        let process = attach_optional(process, process_resource_control_port, |process, port| {
            process.with_resource_control(port)
        });
        let process = attach_optional(process, process_network_escalation_port, |process, port| {
            process.with_network_escalation(port)
        });

        let service = ServiceFacets::default();
        let service = attach_optional(service, service_inventory_port, |service, port| {
            service.with_inventory(port)
        });
        let service = attach_optional(service, service_dependencies_port, |service, port| {
            service.with_dependencies(port)
        });
        let service = attach_optional(service, service_control_port, |service, port| {
            service.with_control(port)
        });
        let service = attach_optional(service, service_log_snapshot_port, |service, port| {
            service.with_log_snapshot(port)
        });
        let service = attach_optional(service, service_log_stream_port, |service, port| {
            service.with_log_stream(port)
        });

        let environment = EnvironmentFacets::default();
        let environment =
            attach_optional(environment, startup_inventory_port, |environment, port| {
                environment.with_startup_inventory(port)
            });
        let environment =
            attach_optional(environment, startup_evidence_port, |environment, port| {
                environment.with_startup_evidence(port)
            });
        let environment =
            attach_optional(environment, startup_control_port, |environment, port| {
                environment.with_startup_control(port)
            });
        let environment =
            attach_optional(environment, session_inventory_port, |environment, port| {
                environment.with_session_inventory(port)
            });
        let environment =
            attach_optional(environment, session_control_port, |environment, port| {
                environment.with_session_control(port)
            });

        let integration = IntegrationFacets::default();
        let integration = attach_optional(integration, command_launch_port, |integration, port| {
            integration.with_command_launch(port)
        });
        let integration =
            attach_optional(integration, resource_reveal_port, |integration, port| {
                integration.with_resource_reveal(port)
            });
        let integration = attach_optional(integration, url_open_port, |integration, port| {
            integration.with_url_open(port)
        });
        let integration =
            attach_optional(integration, desktop_appearance_port, |integration, port| {
                integration.with_desktop_appearance(port)
            });
        let integration = attach_optional(integration, setup_script_port, |integration, port| {
            integration.with_setup_script(port)
        });
        let integration = attach_optional(
            integration,
            desktop_notification_port,
            |integration, port| integration.with_desktop_notification(port),
        );

        let storage = StorageFacets::default();
        let storage = attach_optional(storage, storage_health_port, |storage, port| {
            storage.with_health(port)
        });
        let storage = attach_optional(storage, smart_observation_port, |storage, port| {
            storage.with_smart_observation(port)
        });
        let storage = attach_optional(storage, smart_control_port, |storage, port| {
            storage.with_smart_control(port)
        });
        let storage = attach_optional(storage, directory_usage_port, |storage, port| {
            storage.with_directory_usage(port)
        });

        let sensor = attach_optional(SensorFacets::default(), sensor_port, |sensor, port| {
            sensor.with_observation(port)
        });
        let power = attach_optional(PowerFacets::default(), power_supply_port, |power, port| {
            power.with_supplies(port)
        });
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
                        smbios_memory_rx,
                        rapl_power_rx,
                        msr_readout_rx,
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

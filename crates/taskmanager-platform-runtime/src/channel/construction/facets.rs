//! Service, environment, integration, storage, sensor, and power facet-domain
//! lane construction.
//!
//! Each helper opens one domain's bounded request lanes, attaches the present
//! provider ports to that domain's facets, and bundles the consumer receivers
//! into its pending runtime lanes.

use taskmanager_application::{
    EnvironmentFacets, IntegrationFacets, PowerFacets, SensorFacets, ServiceFacets, StorageFacets,
};

use super::super::facet_attach::attach_optional;
use super::super::port::request_lane;
use super::context::LaneContext;
use crate::config::{
    EnvironmentProviderBindings, IntegrationProviderBindings, PowerProviderBindings,
    SensorProviderBindings, ServiceProviderBindings, StorageProviderBindings,
};
use crate::environment::PendingEnvironmentRuntimeLanes;
use crate::integration::PendingIntegrationRuntimeLanes;
use crate::power::PendingPowerRuntimeLanes;
use crate::sensor::PendingSensorRuntimeLanes;
use crate::service::PendingServiceRuntimeLanes;
use crate::storage::PendingStorageRuntimeLanes;

/// Open the service request lanes, attach the present provider ports to the
/// service facets, and bundle the consumer receivers into runtime lanes.
pub(super) fn service_runtime(
    bindings: &ServiceProviderBindings,
    context: &LaneContext,
) -> (ServiceFacets, PendingServiceRuntimeLanes) {
    let (service_inventory_port, service_inventory_rx) = request_lane(
        context.observation_capacity,
        bindings.inventory.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (service_dependencies_port, service_dependencies_rx) = request_lane(
        context.observation_capacity,
        bindings.dependencies.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (service_control_port, service_control_rx) = request_lane(
        context.control_capacity,
        bindings.control.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (service_log_snapshot_port, service_log_snapshot_rx) = request_lane(
        context.observation_capacity,
        bindings.log_snapshot.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (service_log_stream_port, service_log_stream_rx) = request_lane(
        context.observation_capacity,
        bindings.log_stream.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );

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

    let lanes = PendingServiceRuntimeLanes::new(
        service_inventory_rx,
        service_dependencies_rx,
        service_control_rx,
        service_log_snapshot_rx,
        service_log_stream_rx,
    );
    (service, lanes)
}

/// Open the environment request lanes, attach the present provider ports to the
/// environment facets, and bundle the consumer receivers into runtime lanes.
pub(super) fn environment_runtime(
    bindings: &EnvironmentProviderBindings,
    context: &LaneContext,
) -> (EnvironmentFacets, PendingEnvironmentRuntimeLanes) {
    let (startup_inventory_port, startup_inventory_rx) = request_lane(
        context.observation_capacity,
        bindings.startup_inventory.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (startup_evidence_port, startup_evidence_rx) = request_lane(
        context.observation_capacity,
        bindings.startup_evidence.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (startup_control_port, startup_control_rx) = request_lane(
        context.control_capacity,
        bindings.startup_control.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (session_inventory_port, session_inventory_rx) = request_lane(
        context.observation_capacity,
        bindings.session_inventory.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (session_control_port, session_control_rx) = request_lane(
        context.control_capacity,
        bindings.session_control.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );

    let environment = EnvironmentFacets::default();
    let environment = attach_optional(environment, startup_inventory_port, |environment, port| {
        environment.with_startup_inventory(port)
    });
    let environment = attach_optional(environment, startup_evidence_port, |environment, port| {
        environment.with_startup_evidence(port)
    });
    let environment = attach_optional(environment, startup_control_port, |environment, port| {
        environment.with_startup_control(port)
    });
    let environment = attach_optional(environment, session_inventory_port, |environment, port| {
        environment.with_session_inventory(port)
    });
    let environment = attach_optional(environment, session_control_port, |environment, port| {
        environment.with_session_control(port)
    });

    let lanes = PendingEnvironmentRuntimeLanes::new(
        startup_inventory_rx,
        startup_evidence_rx,
        startup_control_rx,
        session_inventory_rx,
        session_control_rx,
    );
    (environment, lanes)
}

/// Open the integration request lanes, attach the present provider ports to the
/// integration facets, and bundle the consumer receivers into runtime lanes.
pub(super) fn integration_runtime(
    bindings: &IntegrationProviderBindings,
    context: &LaneContext,
) -> (IntegrationFacets, PendingIntegrationRuntimeLanes) {
    let (command_launch_port, command_launch_rx) = request_lane(
        context.control_capacity,
        bindings.command_launch.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (resource_reveal_port, resource_reveal_rx) = request_lane(
        context.control_capacity,
        bindings.resource_reveal.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (url_open_port, url_open_rx) = request_lane(
        context.control_capacity,
        bindings.url_open.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (desktop_appearance_port, desktop_appearance_rx) = request_lane(
        context.observation_capacity,
        bindings.desktop_appearance.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (setup_script_port, setup_script_rx) = request_lane(
        context.control_capacity,
        bindings.setup_script.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (desktop_notification_port, desktop_notification_rx) = request_lane(
        context.control_capacity,
        bindings.desktop_notification.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );

    let integration = IntegrationFacets::default();
    let integration = attach_optional(integration, command_launch_port, |integration, port| {
        integration.with_command_launch(port)
    });
    let integration = attach_optional(integration, resource_reveal_port, |integration, port| {
        integration.with_resource_reveal(port)
    });
    let integration = attach_optional(integration, url_open_port, |integration, port| {
        integration.with_url_open(port)
    });
    let integration = attach_optional(integration, desktop_appearance_port, |integration, port| {
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

    let lanes = PendingIntegrationRuntimeLanes::new(
        command_launch_rx,
        resource_reveal_rx,
        url_open_rx,
        desktop_appearance_rx,
        desktop_notification_rx,
        setup_script_rx,
    );
    (integration, lanes)
}

/// Open the storage request lanes, attach the present provider ports to the
/// storage facets, and bundle the consumer receivers into runtime lanes.
pub(super) fn storage_runtime(
    bindings: &StorageProviderBindings,
    context: &LaneContext,
) -> (StorageFacets, PendingStorageRuntimeLanes) {
    let (storage_health_port, storage_health_rx) = request_lane(
        context.observation_capacity,
        bindings.health.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (smart_observation_port, smart_observation_rx) = request_lane(
        context.observation_capacity,
        bindings.smart_observation.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (smart_control_port, smart_control_rx) = request_lane(
        context.control_capacity,
        bindings.smart_control.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (directory_usage_port, directory_usage_rx) = request_lane(
        context.observation_capacity,
        bindings.directory_usage.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
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

    let lanes = PendingStorageRuntimeLanes::new(
        storage_health_rx,
        smart_observation_rx,
        smart_control_rx,
        directory_usage_rx,
    );
    (storage, lanes)
}

/// Open the sensor request lane, attach the present provider port to the sensor
/// facets, and bundle the consumer receiver into runtime lanes.
pub(super) fn sensor_runtime(
    bindings: &SensorProviderBindings,
    context: &LaneContext,
) -> (SensorFacets, PendingSensorRuntimeLanes) {
    let (sensor_port, sensor_rx) = request_lane(
        context.observation_capacity,
        bindings.observation.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );

    let sensor = attach_optional(SensorFacets::default(), sensor_port, |sensor, port| {
        sensor.with_observation(port)
    });
    let lanes = PendingSensorRuntimeLanes::new(sensor_rx);
    (sensor, lanes)
}

/// Open the power request lane, attach the present provider port to the power
/// facets, and bundle the consumer receiver into runtime lanes.
pub(super) fn power_runtime(
    bindings: &PowerProviderBindings,
    context: &LaneContext,
) -> (PowerFacets, PendingPowerRuntimeLanes) {
    let (power_supply_port, power_supply_rx) = request_lane(
        context.observation_capacity,
        bindings.supplies.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );

    let power = attach_optional(PowerFacets::default(), power_supply_port, |power, port| {
        power.with_supplies(port)
    });
    let lanes = PendingPowerRuntimeLanes::new(power_supply_rx);
    (power, lanes)
}

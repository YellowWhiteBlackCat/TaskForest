//! Process facet-domain lane construction.
//!
//! Opens the bounded process observation and control request lanes, attaches
//! the present provider ports to `ProcessFacets`, and bundles the consumer
//! receivers into `PendingProcessRuntimeLanes`.

use taskmanager_application::ProcessFacets;

use super::super::facet_attach::attach_optional;
use super::super::port::request_lane;
use super::context::LaneContext;
use crate::config::ProcessProviderBindings;
use crate::process::{
    PendingProcessControlLanes, PendingProcessObservationLanes, PendingProcessRuntimeLanes,
};

/// Open the process request lanes, attach the present provider ports to the
/// process facets, and bundle the consumer receivers into runtime lanes.
pub(super) fn process_runtime(
    bindings: &ProcessProviderBindings,
    context: &LaneContext,
) -> (ProcessFacets, PendingProcessRuntimeLanes) {
    let (process_list_port, process_list_rx) = request_lane(
        context.observation_capacity,
        bindings.list.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_network_port, process_network_rx) = request_lane(
        context.observation_capacity,
        bindings.network.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_gpu_port, process_gpu_rx) = request_lane(
        context.observation_capacity,
        bindings.gpu.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_resources_port, process_resources_rx) = request_lane(
        context.observation_capacity,
        bindings.resources.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_isolation_port, process_isolation_rx) = request_lane(
        context.observation_capacity,
        bindings.isolation.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_threads_port, process_threads_rx) = request_lane(
        context.observation_capacity,
        bindings.threads.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_open_files_port, process_open_files_rx) = request_lane(
        context.observation_capacity,
        bindings.open_files.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_environment_port, process_environment_rx) = request_lane(
        context.observation_capacity,
        bindings.environment.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_affinity_port, process_affinity_rx) = request_lane(
        context.observation_capacity,
        bindings.affinity.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_affinity_control_port, process_affinity_control_rx) = request_lane(
        context.control_capacity,
        bindings.affinity_control.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_resource_control_port, process_resource_control_rx) = request_lane(
        context.control_capacity,
        bindings.resource_control.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_network_escalation_port, process_network_escalation_rx) = request_lane(
        context.control_capacity,
        bindings.network_escalation.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (process_control_port, process_control_rx) = request_lane(
        context.control_capacity,
        bindings.control.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );

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

    let lanes = PendingProcessRuntimeLanes::new(
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
    );
    (process, lanes)
}

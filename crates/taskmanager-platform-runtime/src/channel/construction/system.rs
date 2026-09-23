//! System facet-domain lane construction.
//!
//! Opens the bounded system telemetry and auxiliary request lanes, attaches
//! the present provider ports to `SystemFacets`, and bundles the consumer
//! receivers into `PendingSystemRuntimeLanes`.

use taskmanager_application::SystemFacets;

use super::super::facet_attach::{
    SystemAuxiliaryPorts, attach_optional, attach_system_auxiliary_facets,
};
use super::super::port::request_lane;
use super::context::LaneContext;
use crate::config::SystemProviderBindings;
use crate::system::PendingSystemRuntimeLanes;

/// Open the system request lanes, attach the present provider ports to the
/// system facets, and bundle the consumer receivers into runtime lanes.
pub(super) fn system_runtime(
    bindings: &SystemProviderBindings,
    context: &LaneContext,
) -> (SystemFacets, PendingSystemRuntimeLanes) {
    let (host_telemetry_port, host_telemetry_rx) = request_lane(
        context.observation_capacity,
        bindings.host.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (cpu_telemetry_port, cpu_telemetry_rx) = request_lane(
        context.observation_capacity,
        bindings.cpu.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (memory_telemetry_port, memory_telemetry_rx) = request_lane(
        context.observation_capacity,
        bindings.memory.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (storage_telemetry_port, storage_telemetry_rx) = request_lane(
        context.observation_capacity,
        bindings.storage.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (network_telemetry_port, network_telemetry_rx) = request_lane(
        context.observation_capacity,
        bindings.network.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (gpu_telemetry_port, gpu_telemetry_rx) = request_lane(
        context.observation_capacity,
        bindings.gpu.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (hardware_inventory_port, hardware_inventory_rx) = request_lane(
        context.observation_capacity,
        bindings.hardware_inventory.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (containers_port, containers_rx) = request_lane(
        context.observation_capacity,
        bindings.containers.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (gpu_engine_rows_port, gpu_engine_rows_rx) = request_lane(
        context.observation_capacity,
        bindings.gpu_engine_rows.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (npu_inventory_port, npu_inventory_rx) = request_lane(
        context.observation_capacity,
        bindings.npu_inventory.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (smbios_memory_port, smbios_memory_rx) = request_lane(
        context.observation_capacity,
        bindings.smbios_memory.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (rapl_power_port, rapl_power_rx) = request_lane(
        context.observation_capacity,
        bindings.rapl_power.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (msr_readout_port, msr_readout_rx) = request_lane(
        context.observation_capacity,
        bindings.msr_readout.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );
    let (cpu_throttle_port, cpu_throttle_rx) = request_lane(
        context.observation_capacity,
        bindings.cpu_throttle.as_ref(),
        context.ecs_scheduler.clone(),
        context.lane_starters.clone(),
    );

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
    let system = attach_system_auxiliary_facets(
        system,
        SystemAuxiliaryPorts {
            gpu_engine_rows: gpu_engine_rows_port,
            npu_inventory: npu_inventory_port,
            smbios_memory: smbios_memory_port,
            rapl_power: rapl_power_port,
            msr_readout: msr_readout_port,
            cpu_throttle: cpu_throttle_port,
        },
    );

    let lanes = PendingSystemRuntimeLanes::new(
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
            cpu_throttle_rx,
        ),
    );
    (system, lanes)
}

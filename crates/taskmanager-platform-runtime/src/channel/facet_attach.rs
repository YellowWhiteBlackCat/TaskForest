//! Optional facet attachment for the bounded request lanes.
//!
//! One typed request port is attached to its facet group only when the
//! provider binding created a port; an absent binding leaves the facet empty,
//! so the capability stays honestly unavailable — no request lane, no catalog
//! descriptor. Split from `construction.rs` so that file stays inside the
//! workspace file-line budget.

use std::sync::Arc;

use taskmanager_application::{
    CpuThrottleRequest, GpuEngineRowsRequest, MsrReadoutRequest, NpuInventoryRequest,
    RaplPowerRequest, SmbiosMemoryRequest, SystemFacets,
};
use taskmanager_platform_contract::CapabilityRequest;

use super::port::ChannelRequestPort;

pub(super) fn attach_optional<R, F, T>(
    facets: T,
    port: Option<Arc<ChannelRequestPort<R>>>,
    attach: F,
) -> T
where
    R: CapabilityRequest,
    F: FnOnce(T, Arc<ChannelRequestPort<R>>) -> T,
{
    match port {
        Some(port) => attach(facets, port),
        None => facets,
    }
}

/// Attach the optional auxiliary system facets (per-engine GPU rows, NPU
/// inventory, SMBIOS memory, RAPL package power, CPU MSR readouts, CPU
/// thermal-throttle counters).
#[allow(clippy::too_many_arguments)]
pub(super) fn attach_system_auxiliary_facets(
    system: SystemFacets,
    gpu_engine_rows: Option<Arc<ChannelRequestPort<GpuEngineRowsRequest>>>,
    npu_inventory: Option<Arc<ChannelRequestPort<NpuInventoryRequest>>>,
    smbios_memory: Option<Arc<ChannelRequestPort<SmbiosMemoryRequest>>>,
    rapl_power: Option<Arc<ChannelRequestPort<RaplPowerRequest>>>,
    msr_readout: Option<Arc<ChannelRequestPort<MsrReadoutRequest>>>,
    cpu_throttle: Option<Arc<ChannelRequestPort<CpuThrottleRequest>>>,
) -> SystemFacets {
    let system = attach_optional(system, gpu_engine_rows, |system, port| {
        system.with_gpu_engine_rows(port)
    });
    let system = attach_optional(system, npu_inventory, |system, port| {
        system.with_npu_inventory(port)
    });
    let system = attach_optional(system, smbios_memory, |system, port| {
        system.with_smbios_memory(port)
    });
    let system = attach_optional(system, rapl_power, |system, port| {
        system.with_rapl_power(port)
    });
    let system = attach_optional(system, msr_readout, |system, port| {
        system.with_msr_readout(port)
    });
    attach_optional(system, cpu_throttle, |system, port| {
        system.with_cpu_throttle(port)
    })
}

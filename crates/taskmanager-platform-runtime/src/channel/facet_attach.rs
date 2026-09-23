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

/// The optional auxiliary system provider ports, bundled so the facet
/// attachment seam takes one typed parameter instead of six positional lanes.
///
/// Each field is `Some` only when its provider binding created a lane; an
/// absent binding stays `None` and its facet is left empty.
pub(super) struct SystemAuxiliaryPorts {
    /// Per-engine GPU utilization rows.
    pub(super) gpu_engine_rows: Option<Arc<ChannelRequestPort<GpuEngineRowsRequest>>>,
    /// NPU inventory and utilization.
    pub(super) npu_inventory: Option<Arc<ChannelRequestPort<NpuInventoryRequest>>>,
    /// SMBIOS memory-module inventory.
    pub(super) smbios_memory: Option<Arc<ChannelRequestPort<SmbiosMemoryRequest>>>,
    /// RAPL package power counters.
    pub(super) rapl_power: Option<Arc<ChannelRequestPort<RaplPowerRequest>>>,
    /// CPU MSR readout counters.
    pub(super) msr_readout: Option<Arc<ChannelRequestPort<MsrReadoutRequest>>>,
    /// CPU thermal-throttle counters.
    pub(super) cpu_throttle: Option<Arc<ChannelRequestPort<CpuThrottleRequest>>>,
}

/// Attach the optional auxiliary system facets (per-engine GPU rows, NPU
/// inventory, SMBIOS memory, RAPL package power, CPU MSR readouts, CPU
/// thermal-throttle counters) from their bundled provider ports.
pub(super) fn attach_system_auxiliary_facets(
    system: SystemFacets,
    ports: SystemAuxiliaryPorts,
) -> SystemFacets {
    let SystemAuxiliaryPorts {
        gpu_engine_rows,
        npu_inventory,
        smbios_memory,
        rapl_power,
        msr_readout,
        cpu_throttle,
    } = ports;
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

//! macOS auxiliary system providers: the request/response lanes outside
//! domain observation ownership, boxed into one immutable composition group.
//! Split from `system.rs` so that file stays inside the workspace line budget.

use taskmanager_platform_runtime::SystemAuxiliaryExecutors;

use super::{
    CpuThrottleRegistration, GpuEngineRowsRegistration, HardwareInventoryRegistration,
    MsrReadoutRegistration, NpuInventoryRegistration, RaplPowerRegistration,
    SmbiosMemoryRegistration,
};

/// Hardware operations outside domain observation ownership.
pub struct MacSystemAuxiliaryProviders {
    pub(super) hardware_inventory: HardwareInventoryRegistration,
    pub(super) gpu_engine_rows: GpuEngineRowsRegistration,
    pub(super) npu_inventory: NpuInventoryRegistration,
    pub(super) smbios_memory: SmbiosMemoryRegistration,
    pub(super) rapl_power: RaplPowerRegistration,
    pub(super) msr_readout: MsrReadoutRegistration,
    pub(super) cpu_throttle: CpuThrottleRegistration,
}

/// Named composition input for [`MacSystemAuxiliaryProviders`].
///
/// Each field is one hardware capability outside domain observation ownership,
/// already erased to the platform-neutral provider trait object the group
/// stores. Naming the seven axes keeps the auxiliary vocabulary explicit at the
/// composition seam without a positional argument list.
pub struct MacSystemAuxiliaryProvidersParams {
    /// Hardware inventory provider.
    pub hardware_inventory: HardwareInventoryRegistration,
    /// Per-engine GPU utilization provider.
    pub gpu_engine_rows: GpuEngineRowsRegistration,
    /// NPU accelerator inventory provider.
    pub npu_inventory: NpuInventoryRegistration,
    /// SMBIOS memory inventory provider.
    pub smbios_memory: SmbiosMemoryRegistration,
    /// CPU package power provider.
    pub rapl_power: RaplPowerRegistration,
    /// CPU MSR readout provider.
    pub msr_readout: MsrReadoutRegistration,
    /// CPU thermal-throttle counter provider.
    pub cpu_throttle: CpuThrottleRegistration,
}

impl MacSystemAuxiliaryProviders {
    #[must_use]
    pub fn new(params: MacSystemAuxiliaryProvidersParams) -> Self {
        Self {
            hardware_inventory: params.hardware_inventory,
            gpu_engine_rows: params.gpu_engine_rows,
            npu_inventory: params.npu_inventory,
            smbios_memory: params.smbios_memory,
            rapl_power: params.rapl_power,
            msr_readout: params.msr_readout,
            cpu_throttle: params.cpu_throttle,
        }
    }

    pub(crate) fn into_runtime(self) -> SystemAuxiliaryExecutors {
        let Self {
            hardware_inventory,
            gpu_engine_rows,
            npu_inventory,
            smbios_memory,
            rapl_power,
            msr_readout,
            cpu_throttle,
        } = self;
        let mut hardware_inventory = hardware_inventory.into_provider();
        let mut gpu_engine_rows = gpu_engine_rows.into_provider();
        let mut npu_inventory = npu_inventory.into_provider();
        let mut smbios_memory = smbios_memory.into_provider();
        let mut rapl_power = rapl_power.into_provider();
        let mut msr_readout = msr_readout.into_provider();
        let mut cpu_throttle = cpu_throttle.into_provider();
        SystemAuxiliaryExecutors::new(move || hardware_inventory.refresh())
            .with_gpu_engine_rows(move |request| {
                gpu_engine_rows.read_engine_rows(&request.device_id)
            })
            .with_npu_inventory(move |observed_at_ms| npu_inventory.read_inventory(observed_at_ms))
            .with_smbios_memory(move || smbios_memory.read_memory_smbios())
            .with_rapl_power(move || rapl_power.read_package_power())
            .with_msr_readout(move || msr_readout.read_msr_readouts())
            .with_cpu_throttle(move || cpu_throttle.read_cpu_throttle())
    }
}

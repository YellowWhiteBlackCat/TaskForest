//! Windows auxiliary system providers: the request/response lanes outside
//! domain observation ownership, boxed into one immutable composition group.
//! Split from `system.rs` so that file stays inside the workspace line budget.

use taskmanager_application::{
    GpuEngineRowsRequest, HardwareInventoryRequest, MsrReadoutRequest, NpuInventoryRequest,
    RaplPowerRequest, SmbiosMemoryRequest,
};
use taskmanager_platform_provider::{
    GpuEngineRowsProvider, HardwareInventoryProvider, MsrReadoutProvider, NpuInventoryProvider,
    RaplPowerProvider, SmbiosMemoryProvider,
};
use taskmanager_platform_runtime::{ProviderRegistration, SystemAuxiliaryExecutors};

use super::{
    GpuEngineRowsRegistration, HardwareInventoryRegistration, MsrReadoutRegistration,
    NpuInventoryRegistration, RaplPowerRegistration, SmbiosMemoryRegistration,
};

/// Hardware operations outside domain observation ownership.
pub struct WinSystemAuxiliaryProviders {
    pub(super) hardware_inventory: HardwareInventoryRegistration,
    pub(super) gpu_engine_rows: GpuEngineRowsRegistration,
    pub(super) npu_inventory: NpuInventoryRegistration,
    pub(super) smbios_memory: SmbiosMemoryRegistration,
    pub(super) rapl_power: RaplPowerRegistration,
    pub(super) msr_readout: MsrReadoutRegistration,
}

impl WinSystemAuxiliaryProviders {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new<P, E, N, S, R, M>(
        hardware_inventory: ProviderRegistration<HardwareInventoryRequest, P>,
        gpu_engine_rows: ProviderRegistration<GpuEngineRowsRequest, E>,
        npu_inventory: ProviderRegistration<NpuInventoryRequest, N>,
        smbios_memory: ProviderRegistration<SmbiosMemoryRequest, S>,
        rapl_power: ProviderRegistration<RaplPowerRequest, R>,
        msr_readout: ProviderRegistration<MsrReadoutRequest, M>,
    ) -> Self
    where
        P: HardwareInventoryProvider,
        E: GpuEngineRowsProvider,
        N: NpuInventoryProvider,
        S: SmbiosMemoryProvider,
        R: RaplPowerProvider,
        M: MsrReadoutProvider,
    {
        Self {
            hardware_inventory: hardware_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn HardwareInventoryProvider>),
            gpu_engine_rows: gpu_engine_rows
                .map_provider(|provider| Box::new(provider) as Box<dyn GpuEngineRowsProvider>),
            npu_inventory: npu_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn NpuInventoryProvider>),
            smbios_memory: smbios_memory
                .map_provider(|provider| Box::new(provider) as Box<dyn SmbiosMemoryProvider>),
            rapl_power: rapl_power
                .map_provider(|provider| Box::new(provider) as Box<dyn RaplPowerProvider>),
            msr_readout: msr_readout
                .map_provider(|provider| Box::new(provider) as Box<dyn MsrReadoutProvider>),
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
        } = self;
        let mut hardware_inventory = hardware_inventory.into_provider();
        let mut gpu_engine_rows = gpu_engine_rows.into_provider();
        let mut npu_inventory = npu_inventory.into_provider();
        let mut smbios_memory = smbios_memory.into_provider();
        let mut rapl_power = rapl_power.into_provider();
        let mut msr_readout = msr_readout.into_provider();
        SystemAuxiliaryExecutors::new(move || hardware_inventory.refresh())
            .with_gpu_engine_rows(move |request| {
                gpu_engine_rows.read_engine_rows(&request.device_id)
            })
            .with_npu_inventory(move |observed_at_ms| npu_inventory.read_inventory(observed_at_ms))
            .with_smbios_memory(move || smbios_memory.read_memory_smbios())
            .with_rapl_power(move || rapl_power.read_package_power())
            .with_msr_readout(move || msr_readout.read_msr_readouts())
    }
}

//! Linux system-domain observation, control, and hardware providers.

use std::time::Instant;

use taskmanager_application::{
    ContainerRollupRequest, CpuTelemetryRequest, GpuEngineRowsRequest, GpuTelemetryRequest,
    HardwareInventoryRequest, HostTelemetryRequest, MemoryTelemetryRequest, MsrReadoutRequest,
    NetworkTelemetryRequest, NpuInventoryRequest, RaplPowerRequest, SmbiosMemoryRequest,
    StorageTelemetryRequest,
};
use taskmanager_core::{
    ContainerRollup, CpuTelemetryObservation, GpuTelemetryObservation, HardwareInfo,
    HostRuntimeObservation, MemoryTelemetryObservation, NetworkTelemetryObservation, ProviderId,
    StorageTelemetryObservation,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    ContainerRollupProvider, CpuTelemetryProvider, GpuTelemetryProvider, HardwareInventoryProvider,
    HostTelemetryProvider, MemoryTelemetryProvider, NetworkTelemetryProvider,
    StorageTelemetryProvider,
};
use taskmanager_platform_runtime::ProviderRegistration;

use crate::backend::{SystemAuxiliaryProviders, SystemObservationProviders, SystemProviders};
use crate::engine::collector::domains::{
    LinuxCpuTelemetryCollector, LinuxGpuTelemetryCollector, LinuxHostTelemetryCollector,
    LinuxMemoryTelemetryCollector, LinuxNetworkTelemetryCollector, LinuxStorageTelemetryCollector,
};
use crate::engine::storage_target::StorageTargetResolver;
use crate::provider::gpu_engine_rows::NativeGpuEngineRowsProvider;
use crate::provider::msr_readout::NativeMsrReadoutProvider;
use crate::provider::npu_inventory::NativeNpuInventoryProvider;
use crate::provider::rapl_power::NativeRaplPowerProvider;
use crate::provider::smbios_memory::NativeSmbiosMemoryProvider;

const HOST_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("linux.telemetry.host.procfs");
const CPU_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.telemetry.cpu.sysinfo-sysfs");
const MEMORY_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.telemetry.memory.sysinfo-procfs");
const STORAGE_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.telemetry.storage.sysfs-smart");
const NETWORK_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.telemetry.network.sysinfo-procfs");
const GPU_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.telemetry.gpu.runtime-registry");
const HARDWARE_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("linux.hardware.inventory");
const CONTAINER_ROLLUP_PROVIDER: ProviderId = ProviderId::borrowed("linux.containers.cgroup-v2");
const GPU_ENGINE_ROWS_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.telemetry.gpu-engines.pmu");
const NPU_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("linux.accelerator.npu.sysfs");
const SMBIOS_MEMORY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.telemetry.memory.smbios-helper");
const RAPL_POWER_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.telemetry.cpu.package-power.rapl-helper");
const MSR_READOUT_PROVIDER: ProviderId = ProviderId::borrowed("linux.telemetry.cpu.msr-helper");

pub(super) struct NativeHostTelemetryProvider {
    pub(super) collector: LinuxHostTelemetryCollector,
}

impl HostTelemetryProvider for NativeHostTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<HostRuntimeObservation, ProviderFailure> {
        Ok(self.collector.observe(observed_at_ms))
    }
}

pub(super) struct NativeCpuTelemetryProvider {
    pub(super) collector: LinuxCpuTelemetryCollector,
}

impl CpuTelemetryProvider for NativeCpuTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<CpuTelemetryObservation, ProviderFailure> {
        Ok(self.collector.observe(Instant::now(), observed_at_ms))
    }
}

pub(super) struct NativeMemoryTelemetryProvider {
    pub(super) collector: LinuxMemoryTelemetryCollector,
}

impl MemoryTelemetryProvider for NativeMemoryTelemetryProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<MemoryTelemetryObservation, ProviderFailure> {
        Ok(self.collector.observe(Instant::now(), observed_at_ms))
    }
}

pub(super) struct NativeStorageTelemetryProvider {
    pub(super) collector: LinuxStorageTelemetryCollector,
}

impl StorageTelemetryProvider for NativeStorageTelemetryProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StorageTelemetryObservation, ProviderFailure> {
        Ok(self.collector.observe(Instant::now(), observed_at_ms))
    }
}

pub(super) struct NativeNetworkTelemetryProvider {
    pub(super) collector: LinuxNetworkTelemetryCollector,
}

impl NetworkTelemetryProvider for NativeNetworkTelemetryProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NetworkTelemetryObservation, ProviderFailure> {
        Ok(self.collector.observe(Instant::now(), observed_at_ms))
    }
}

pub(super) struct NativeGpuTelemetryProvider {
    pub(super) collector: LinuxGpuTelemetryCollector,
}

impl GpuTelemetryProvider for NativeGpuTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<GpuTelemetryObservation, ProviderFailure> {
        Ok(self.collector.observe(Instant::now(), observed_at_ms))
    }
}

#[derive(Default)]
pub(super) struct NativeHardwareInventoryProvider {
    collector: crate::engine::hardware::HardwareInventoryCollector,
}

impl HardwareInventoryProvider for NativeHardwareInventoryProvider {
    fn refresh(&mut self) -> Result<CompositeSourceSnapshot<HardwareInfo>, ProviderFailure> {
        Ok(self.collector.refresh())
    }
}

pub(super) struct NativeContainerRollupProvider {
    pub(super) collector: crate::engine::process::telemetry::containers::ContainerRollupCollector,
}

impl ContainerRollupProvider for NativeContainerRollupProvider {
    fn refresh(&mut self, now_ms: u64) -> Result<ContainerRollup, ProviderFailure> {
        Ok(self.collector.collect(now_ms))
    }
}

pub(super) fn native_system_providers() -> (SystemProviders, StorageTargetResolver) {
    let storage_collector = LinuxStorageTelemetryCollector::new();
    let storage_target_resolver = storage_collector.target_resolver();
    let observations = SystemObservationProviders::new(
        ProviderRegistration::<HostTelemetryRequest, _>::new(
            HOST_TELEMETRY_PROVIDER.clone(),
            NativeHostTelemetryProvider {
                collector: LinuxHostTelemetryCollector::new(),
            },
        ),
        ProviderRegistration::<CpuTelemetryRequest, _>::new(
            CPU_TELEMETRY_PROVIDER.clone(),
            NativeCpuTelemetryProvider {
                collector: LinuxCpuTelemetryCollector::new(),
            },
        ),
        ProviderRegistration::<MemoryTelemetryRequest, _>::new(
            MEMORY_TELEMETRY_PROVIDER.clone(),
            NativeMemoryTelemetryProvider {
                collector: LinuxMemoryTelemetryCollector::new(),
            },
        ),
        ProviderRegistration::<StorageTelemetryRequest, _>::new(
            STORAGE_TELEMETRY_PROVIDER.clone(),
            NativeStorageTelemetryProvider {
                collector: storage_collector,
            },
        ),
        ProviderRegistration::<NetworkTelemetryRequest, _>::new(
            NETWORK_TELEMETRY_PROVIDER.clone(),
            NativeNetworkTelemetryProvider {
                collector: LinuxNetworkTelemetryCollector::new(),
            },
        ),
        ProviderRegistration::<GpuTelemetryRequest, _>::new(
            GPU_TELEMETRY_PROVIDER.clone(),
            NativeGpuTelemetryProvider {
                collector: LinuxGpuTelemetryCollector::new(),
            },
        ),
        ProviderRegistration::<ContainerRollupRequest, _>::new(
            CONTAINER_ROLLUP_PROVIDER.clone(),
            NativeContainerRollupProvider {
                collector:
                    crate::engine::process::telemetry::containers::ContainerRollupCollector::default(
                    ),
            },
        ),
    );
    let auxiliary = SystemAuxiliaryProviders::new(
        ProviderRegistration::<HardwareInventoryRequest, _>::new(
            HARDWARE_INVENTORY_PROVIDER.clone(),
            NativeHardwareInventoryProvider::default(),
        ),
        {
            let provider = NativeGpuEngineRowsProvider::new();
            let initial_status = provider.initial_status();
            ProviderRegistration::<GpuEngineRowsRequest, _>::new(
                GPU_ENGINE_ROWS_PROVIDER.clone(),
                provider,
            )
            .with_initial_status(initial_status)
        },
        ProviderRegistration::<NpuInventoryRequest, _>::new(
            NPU_INVENTORY_PROVIDER.clone(),
            NativeNpuInventoryProvider::new(),
        ),
        {
            let provider = NativeSmbiosMemoryProvider::new();
            let initial_status = provider.initial_status();
            ProviderRegistration::<SmbiosMemoryRequest, _>::new(
                SMBIOS_MEMORY_PROVIDER.clone(),
                provider,
            )
            .with_initial_status(initial_status)
        },
        {
            let provider = NativeRaplPowerProvider::new();
            let initial_status = provider.initial_status();
            ProviderRegistration::<RaplPowerRequest, _>::new(RAPL_POWER_PROVIDER.clone(), provider)
                .with_initial_status(initial_status)
        },
        {
            let provider = NativeMsrReadoutProvider::new();
            let initial_status = provider.initial_status();
            ProviderRegistration::<MsrReadoutRequest, _>::new(
                MSR_READOUT_PROVIDER.clone(),
                provider,
            )
            .with_initial_status(initial_status)
        },
    );
    (
        SystemProviders::new(observations, auxiliary),
        storage_target_resolver,
    )
}

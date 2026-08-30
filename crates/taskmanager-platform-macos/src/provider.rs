//! macOS construction registry for platform-neutral capability providers.
//!
//! Provider interfaces live in `taskmanager-platform-provider`, the single
//! public authority shared with the Linux adapter. This module only groups
//! concrete macOS providers for composition; the groups never imply a shared
//! execution lane.
//!
//! Safe-crate implementations cover system/process/startup/services/power/
//! sensors/storage/integration; providers without a safe source (GPU,
//! per-process network, isolation, affinity, service dependencies, session
//! control, boot evidence, the per-fd open-files insight, desktop
//! notifications, first-run setup) register here with typed unsupported
//! outcomes — honest capability publication, never fabricated observations
//! (ADR-019).

use taskmanager_application::{
    ContainerRollupRequest, CpuTelemetryRequest, DesktopNotificationRequest, GpuEngineRowsRequest,
    GpuTelemetryRequest, HardwareInventoryRequest, HostTelemetryRequest, MemoryTelemetryRequest,
    MsrReadoutRequest, NetworkTelemetryRequest, NpuInventoryRequest, ProcessAffinityControlRequest,
    ProcessAffinityRequest, ProcessControlRequest, ProcessGpuRequest, ProcessIsolationRequest,
    ProcessListRequest, ProcessNetworkEscalationRequest, ProcessNetworkRequest,
    ProcessOpenFilesRequest, ProcessResourceControlRequest, ProcessResourcesRequest,
    ProcessThreadsRequest, RaplPowerRequest, SessionControlRequest, SessionInventoryRequest,
    SetupScriptRequest, SmbiosMemoryRequest, StartupControlRequest, StartupEvidenceRequest,
    StartupInventoryRequest, StorageTelemetryRequest,
};
use taskmanager_core::ProviderId;
use taskmanager_platform_runtime::ProviderRegistration;

use self::environment::{
    MacSessionInventoryProvider, MacStartupControlProvider, MacStartupEvidenceProvider,
    MacStartupInventoryProvider, PendingSessionControlProvider,
};
use self::gpu::MacGpuTelemetryProvider;
use self::integration::{
    MacCommandLaunchProvider, MacDesktopAppearanceProvider, MacResourceRevealProvider,
    MacUrlOpenProvider, PendingDesktopNotificationProvider, PendingSetupScriptProvider,
};
use self::power::MacPowerSupplyProvider;
use self::process::{
    MacProcessControlProvider, MacProcessListProvider, MacProcessResourcesProvider,
    PendingProcessAffinityControlProvider, PendingProcessAffinityProvider,
    PendingProcessGpuProvider, PendingProcessIsolationProvider,
    PendingProcessNetworkEscalationProvider, PendingProcessNetworkProvider,
    PendingProcessOpenFilesProvider, PendingProcessResourceControlProvider,
    PendingProcessThreadsProvider,
};
use self::sensor::MacSensorProvider;
use self::service::{
    MacServiceControlProvider, MacServiceInventoryProvider, MacServiceLogSnapshotProvider,
    PendingServiceDependenciesProvider, PendingServiceLogStreamProvider,
};
use self::storage::{
    MacFilesystemHealthProvider, MacSmartSelfTestControlProvider,
    MacSmartSelfTestObservationProvider, MacStorageTelemetryProvider,
};
use self::system::{
    MacContainerRollupProvider, MacCpuTelemetryProvider, MacHardwareInventoryProvider,
    MacHostTelemetryProvider, MacMemoryTelemetryProvider, MacNetworkTelemetryProvider,
};

mod directory_usage;
mod environment;
mod gpu;
mod integration;
mod power;
mod process;
mod process_facts;
mod sensor;
mod service;
mod storage;
mod system;

pub use environment::MacEnvironmentProviders;
pub use integration::MacIntegrationProviders;
pub use power::MacPowerProviders;
pub use process::{
    MacProcessControlProviders, MacProcessObservationProviders, MacProcessProviders,
};
pub use sensor::MacSensorProviders;
pub use service::MacServiceProviders;
pub use storage::{MacDirectoryUsageProvider, MacStorageProviders};
pub use system::{
    MacSystemAuxiliaryProviders, MacSystemObservationProviders, MacSystemProviders,
    PendingGpuEngineRowsProvider, PendingMsrReadoutProvider, PendingNpuInventoryProvider,
    PendingRaplPowerProvider, PendingSmbiosMemoryProvider,
};

const HOST_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.system.host");
const CPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.system.cpu");
const MEMORY_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.system.memory");
const STORAGE_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.system.storage");
const NETWORK_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.system.network");
const GPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.system.gpu");
const GPU_ENGINE_ROWS_PROVIDER: ProviderId = ProviderId::borrowed("macos.system.gpu-engines");
const NPU_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("macos.accelerator.npu");
const SMBIOS_MEMORY_PROVIDER: ProviderId = ProviderId::borrowed("macos.telemetry.memory.smbios");
const RAPL_POWER_PROVIDER: ProviderId = ProviderId::borrowed("macos.telemetry.cpu.package-power");
const MSR_READOUT_PROVIDER: ProviderId = ProviderId::borrowed("macos.telemetry.cpu.msr");
const HARDWARE_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("macos.hardware.inventory");
const CONTAINER_ROLLUP_PROVIDER: ProviderId = ProviderId::borrowed("macos.containers.unavailable");
const PROCESS_LIST_PROVIDER: ProviderId = ProviderId::borrowed("macos.process.list");
const PROCESS_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("macos.process.control");
const PROCESS_NETWORK_PROVIDER: ProviderId = ProviderId::borrowed("macos.process.insights.network");
const PROCESS_GPU_PROVIDER: ProviderId = ProviderId::borrowed("macos.process.insights.gpu");
const PROCESS_RESOURCES_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.process.insights.resources");
const PROCESS_ISOLATION_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.process.insights.isolation");
const PROCESS_THREADS_PROVIDER: ProviderId = ProviderId::borrowed("macos.process.insights.threads");
const PROCESS_AFFINITY_PROVIDER: ProviderId = ProviderId::borrowed("macos.process.affinity");
const PROCESS_AFFINITY_CONTROL_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.process.affinity.control");
const PROCESS_RESOURCE_CONTROL_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.process.resource.control");
const PROCESS_OPEN_FILES_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.process.insights.open_files");
const PROCESS_NETWORK_ESCALATION_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.process.network.escalation");
const SERVICE_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("macos.service.inventory");
const SERVICE_DEPENDENCIES_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.service.dependencies");
const SERVICE_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("macos.service.control");
const SERVICE_LOG_SNAPSHOT_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.service.logs.snapshot");
const SERVICE_LOG_STREAM_PROVIDER: ProviderId = ProviderId::borrowed("macos.service.logs.stream");
const STARTUP_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("macos.startup.inventory");
const STARTUP_EVIDENCE_PROVIDER: ProviderId = ProviderId::borrowed("macos.startup.evidence");
const STARTUP_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("macos.startup.control");
const SESSION_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("macos.session.inventory");
const SESSION_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("macos.session.control");
const COMMAND_LAUNCH_PROVIDER: ProviderId = ProviderId::borrowed("macos.shell.command");
const RESOURCE_REVEAL_PROVIDER: ProviderId = ProviderId::borrowed("macos.shell.resource-reveal");
const URL_OPEN_PROVIDER: ProviderId = ProviderId::borrowed("macos.shell.url-open");
const DESKTOP_APPEARANCE_PROVIDER: ProviderId = ProviderId::borrowed("macos.desktop.appearance");
const DESKTOP_NOTIFICATION_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.alerts.desktop-notification");
const FIRST_RUN_SETUP_PROVIDER: ProviderId = ProviderId::borrowed("macos.first-run.setup-script");
const FILESYSTEM_HEALTH_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.storage.filesystem.registry");
const SMART_OBSERVATION_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.storage.smart.observation");
const SMART_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("macos.storage.smart.control");
const DIRECTORY_USAGE_PROVIDER: ProviderId = ProviderId::borrowed("macos.storage.directory-usage");
const SENSOR_CAPABILITY_PROVIDER: ProviderId = ProviderId::borrowed("macos.sensor.registry");
const POWER_SUPPLY_CAPABILITY_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.power-supply.registry");

/// Runtime registry consumed into isolated execution lanes.
///
/// A registry contains capability implementations, not a product SKU. Hardware
/// and optional system services remain runtime-discovered inside providers.
pub struct MacOsProviderRegistry {
    pub(crate) system: MacSystemProviders,
    pub(crate) processes: MacProcessProviders,
    pub(crate) services: MacServiceProviders,
    pub(crate) environment: MacEnvironmentProviders,
    pub(crate) integrations: MacIntegrationProviders,
    pub(crate) storage: MacStorageProviders,
    pub(crate) sensors: MacSensorProviders,
    pub(crate) power: MacPowerProviders,
}

impl MacOsProviderRegistry {
    // The eight arguments are the eight independent application change axes.
    // Nesting unrelated domains only to satisfy an argument-count heuristic
    // would recreate the aggregate provider bag this registry prevents.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        system: MacSystemProviders,
        processes: MacProcessProviders,
        services: MacServiceProviders,
        environment: MacEnvironmentProviders,
        integrations: MacIntegrationProviders,
        storage: MacStorageProviders,
        sensors: MacSensorProviders,
        power: MacPowerProviders,
    ) -> Self {
        Self {
            system,
            processes,
            services,
            environment,
            integrations,
            storage,
            sensors,
            power,
        }
    }
}

pub(super) fn macos_provider_registry() -> MacOsProviderRegistry {
    MacOsProviderRegistry::new(
        MacSystemProviders::new(
            MacSystemObservationProviders::new(
                ProviderRegistration::<HostTelemetryRequest, _>::new(
                    HOST_TELEMETRY_PROVIDER.clone(),
                    MacHostTelemetryProvider::new(),
                ),
                ProviderRegistration::<CpuTelemetryRequest, _>::new(
                    CPU_TELEMETRY_PROVIDER.clone(),
                    MacCpuTelemetryProvider::new(),
                ),
                ProviderRegistration::<MemoryTelemetryRequest, _>::new(
                    MEMORY_TELEMETRY_PROVIDER.clone(),
                    MacMemoryTelemetryProvider::new(),
                ),
                ProviderRegistration::<StorageTelemetryRequest, _>::new(
                    STORAGE_TELEMETRY_PROVIDER.clone(),
                    MacStorageTelemetryProvider::new(),
                ),
                ProviderRegistration::<NetworkTelemetryRequest, _>::new(
                    NETWORK_TELEMETRY_PROVIDER.clone(),
                    MacNetworkTelemetryProvider::new(),
                ),
                ProviderRegistration::<GpuTelemetryRequest, _>::new(
                    GPU_TELEMETRY_PROVIDER.clone(),
                    MacGpuTelemetryProvider,
                ),
                ProviderRegistration::<ContainerRollupRequest, _>::new(
                    CONTAINER_ROLLUP_PROVIDER.clone(),
                    MacContainerRollupProvider,
                ),
            ),
            MacSystemAuxiliaryProviders::new(
                ProviderRegistration::<HardwareInventoryRequest, _>::new(
                    HARDWARE_INVENTORY_PROVIDER.clone(),
                    MacHardwareInventoryProvider::new(),
                ),
                ProviderRegistration::<GpuEngineRowsRequest, _>::new(
                    GPU_ENGINE_ROWS_PROVIDER.clone(),
                    PendingGpuEngineRowsProvider,
                ),
                ProviderRegistration::<NpuInventoryRequest, _>::new(
                    NPU_INVENTORY_PROVIDER.clone(),
                    PendingNpuInventoryProvider,
                ),
                ProviderRegistration::<SmbiosMemoryRequest, _>::new(
                    SMBIOS_MEMORY_PROVIDER.clone(),
                    PendingSmbiosMemoryProvider,
                ),
                ProviderRegistration::<RaplPowerRequest, _>::new(
                    RAPL_POWER_PROVIDER.clone(),
                    PendingRaplPowerProvider,
                ),
                ProviderRegistration::<MsrReadoutRequest, _>::new(
                    MSR_READOUT_PROVIDER.clone(),
                    PendingMsrReadoutProvider,
                ),
            ),
        ),
        MacProcessProviders::new(
            MacProcessObservationProviders::new(
                ProviderRegistration::<ProcessListRequest, _>::new(
                    PROCESS_LIST_PROVIDER.clone(),
                    MacProcessListProvider::new(),
                ),
                ProviderRegistration::<ProcessNetworkRequest, _>::new(
                    PROCESS_NETWORK_PROVIDER.clone(),
                    PendingProcessNetworkProvider,
                ),
                ProviderRegistration::<ProcessGpuRequest, _>::new(
                    PROCESS_GPU_PROVIDER.clone(),
                    PendingProcessGpuProvider,
                ),
                ProviderRegistration::<ProcessResourcesRequest, _>::new(
                    PROCESS_RESOURCES_PROVIDER.clone(),
                    MacProcessResourcesProvider::new(),
                ),
                ProviderRegistration::<ProcessIsolationRequest, _>::new(
                    PROCESS_ISOLATION_PROVIDER.clone(),
                    PendingProcessIsolationProvider,
                ),
                ProviderRegistration::<ProcessThreadsRequest, _>::new(
                    PROCESS_THREADS_PROVIDER.clone(),
                    PendingProcessThreadsProvider,
                ),
                ProviderRegistration::<ProcessAffinityRequest, _>::new(
                    PROCESS_AFFINITY_PROVIDER.clone(),
                    PendingProcessAffinityProvider,
                ),
            )
            .with_open_files(ProviderRegistration::<ProcessOpenFilesRequest, _>::new(
                PROCESS_OPEN_FILES_PROVIDER.clone(),
                PendingProcessOpenFilesProvider,
            )),
            MacProcessControlProviders::new(
                ProviderRegistration::<ProcessAffinityControlRequest, _>::new(
                    PROCESS_AFFINITY_CONTROL_PROVIDER.clone(),
                    PendingProcessAffinityControlProvider,
                ),
                ProviderRegistration::<ProcessControlRequest, _>::new(
                    PROCESS_CONTROL_PROVIDER.clone(),
                    MacProcessControlProvider::new(),
                ),
                ProviderRegistration::<ProcessResourceControlRequest, _>::new(
                    PROCESS_RESOURCE_CONTROL_PROVIDER.clone(),
                    PendingProcessResourceControlProvider,
                ),
                ProviderRegistration::<ProcessNetworkEscalationRequest, _>::new(
                    PROCESS_NETWORK_ESCALATION_PROVIDER.clone(),
                    PendingProcessNetworkEscalationProvider,
                ),
            ),
        ),
        MacServiceProviders::new(
            ProviderRegistration::new(
                SERVICE_INVENTORY_PROVIDER.clone(),
                MacServiceInventoryProvider,
            ),
            ProviderRegistration::new(
                SERVICE_DEPENDENCIES_PROVIDER.clone(),
                PendingServiceDependenciesProvider,
            ),
            ProviderRegistration::new(SERVICE_CONTROL_PROVIDER.clone(), MacServiceControlProvider),
            ProviderRegistration::new(
                SERVICE_LOG_SNAPSHOT_PROVIDER.clone(),
                MacServiceLogSnapshotProvider,
            ),
            ProviderRegistration::new(
                SERVICE_LOG_STREAM_PROVIDER.clone(),
                PendingServiceLogStreamProvider,
            ),
        ),
        MacEnvironmentProviders::new(
            ProviderRegistration::<StartupInventoryRequest, _>::new(
                STARTUP_INVENTORY_PROVIDER.clone(),
                MacStartupInventoryProvider,
            ),
            ProviderRegistration::<StartupEvidenceRequest, _>::new(
                STARTUP_EVIDENCE_PROVIDER.clone(),
                MacStartupEvidenceProvider,
            ),
            ProviderRegistration::<StartupControlRequest, _>::new(
                STARTUP_CONTROL_PROVIDER.clone(),
                MacStartupControlProvider,
            ),
            ProviderRegistration::<SessionInventoryRequest, _>::new(
                SESSION_INVENTORY_PROVIDER.clone(),
                MacSessionInventoryProvider,
            ),
            ProviderRegistration::<SessionControlRequest, _>::new(
                SESSION_CONTROL_PROVIDER.clone(),
                PendingSessionControlProvider,
            ),
        ),
        MacIntegrationProviders::new(
            ProviderRegistration::new(COMMAND_LAUNCH_PROVIDER.clone(), MacCommandLaunchProvider),
            ProviderRegistration::new(RESOURCE_REVEAL_PROVIDER.clone(), MacResourceRevealProvider),
            ProviderRegistration::new(URL_OPEN_PROVIDER.clone(), MacUrlOpenProvider),
            ProviderRegistration::new(
                DESKTOP_APPEARANCE_PROVIDER.clone(),
                MacDesktopAppearanceProvider,
            ),
        )
        .with_desktop_notification(ProviderRegistration::<DesktopNotificationRequest, _>::new(
            DESKTOP_NOTIFICATION_PROVIDER.clone(),
            PendingDesktopNotificationProvider,
        ))
        .with_setup_script(ProviderRegistration::<SetupScriptRequest, _>::new(
            FIRST_RUN_SETUP_PROVIDER.clone(),
            PendingSetupScriptProvider,
        )),
        MacStorageProviders::new(
            ProviderRegistration::new(
                FILESYSTEM_HEALTH_PROVIDER.clone(),
                MacFilesystemHealthProvider::new(),
            ),
            ProviderRegistration::new(
                SMART_OBSERVATION_PROVIDER.clone(),
                MacSmartSelfTestObservationProvider,
            ),
            ProviderRegistration::new(
                SMART_CONTROL_PROVIDER.clone(),
                MacSmartSelfTestControlProvider,
            ),
            ProviderRegistration::new(
                DIRECTORY_USAGE_PROVIDER.clone(),
                MacDirectoryUsageProvider::new(),
            ),
        ),
        MacSensorProviders::new(ProviderRegistration::new(
            SENSOR_CAPABILITY_PROVIDER.clone(),
            MacSensorProvider,
        )),
        MacPowerProviders::new(ProviderRegistration::new(
            POWER_SUPPLY_CAPABILITY_PROVIDER.clone(),
            MacPowerSupplyProvider,
        )),
    )
}

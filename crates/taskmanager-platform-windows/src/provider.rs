//! Windows construction registry for platform-neutral capability providers.
//!
//! Provider interfaces live in `taskmanager-platform-provider`, the single
//! public authority shared with the Linux adapter. This module only groups
//! concrete Windows providers for composition; the groups never imply a
//! shared execution lane.
//!
//! Safety policy (2026-08-02): every implemented provider is built on a
//! published safe wrapper crate. Providers without a safe wrapper register
//! here with a typed unsupported outcome — honest capability publication,
//! never fabricated observations. Three optional facets remain
//! registered-pending (2026-08-14, G-05; open-files, desktop-notification,
//! first-run setup): their descriptors exist so catalog enumeration stays
//! honest, but submissions still complete with typed `Unsupported` outcomes.
//! The fourth facet — directory usage — was wired to the shared pure-safe
//! `DirectoryUsageScanner` on 2026-08-18 (the ADR-018 roadmap item after the
//! G-20 history ring). See `adr/018-windows-telemetry-safety.md`.

use taskmanager_application::{
    ContainerRollupRequest, CpuTelemetryRequest, DesktopNotificationRequest, GpuEngineRowsRequest,
    GpuTelemetryRequest, HardwareInventoryRequest, HostTelemetryRequest, MemoryTelemetryRequest,
    NetworkTelemetryRequest, NpuInventoryRequest, ProcessAffinityControlRequest,
    ProcessAffinityRequest, ProcessControlRequest, ProcessEnvironmentRequest, ProcessGpuRequest,
    ProcessIsolationRequest, ProcessListRequest, ProcessNetworkEscalationRequest,
    ProcessNetworkRequest, ProcessOpenFilesRequest, ProcessResourceControlRequest,
    ProcessResourcesRequest, ProcessThreadsRequest, SessionControlRequest, SessionInventoryRequest,
    SetupScriptRequest, StartupControlRequest, StartupEvidenceRequest, StartupInventoryRequest,
    StorageTelemetryRequest,
};
use taskmanager_core::ProviderId;
use taskmanager_platform_runtime::ProviderRegistration;

use self::environment::{
    WinSessionControlProvider, WinSessionInventoryProvider, WinStartupControlProvider,
    WinStartupEvidenceProvider, WinStartupInventoryProvider,
};
use self::integration::{
    PendingSetupScriptProvider, WinCommandLaunchProvider, WinDesktopAppearanceProvider,
    WinDesktopNotificationProvider, WinResourceRevealProvider, WinUrlOpenProvider,
};
use self::power::WinPowerSupplyProvider;
use self::process::{
    PendingProcessIsolationProvider, PendingProcessNetworkEscalationProvider,
    PendingProcessNetworkProvider, WinProcessAffinityControlProvider, WinProcessAffinityProvider,
    WinProcessControlProvider, WinProcessEnvironmentProvider, WinProcessGpuProvider,
    WinProcessListProvider, WinProcessOpenFilesProvider, WinProcessResourceControlProvider,
    WinProcessResourcesProvider, WinProcessThreadsProvider,
};
use self::sensor::WinSensorProvider;
use self::service::{
    WinServiceControlProvider, WinServiceDependenciesProvider, WinServiceInventoryProvider,
    WinServiceLogSnapshotProvider, WinServiceLogStreamProvider,
};
use self::storage::{
    WinDirectoryUsageProvider, WinFilesystemHealthProvider, WinSmartSelfTestControlProvider,
    WinSmartSelfTestObservationProvider,
};
use self::system::{
    WinContainerRollupProvider, WinCpuTelemetryProvider, WinGpuTelemetryProvider,
    WinHardwareInventoryProvider, WinHostTelemetryProvider, WinMemoryTelemetryProvider,
    WinNetworkTelemetryProvider, WinStorageTelemetryProvider,
};

mod environment;
mod integration;
mod power;
mod process;
mod sensor;
mod service;
mod storage;
mod system;

pub use environment::WinEnvironmentProviders;
pub use integration::WinIntegrationProviders;
pub use power::WinPowerProviders;
pub use process::{
    WinProcessControlProviders, WinProcessObservationProviders, WinProcessProviders,
};
pub use sensor::WinSensorProviders;
pub use service::WinServiceProviders;
pub use storage::WinStorageProviders;
pub use system::{
    WinGpuEngineRowsProvider, WinNpuInventoryProvider, WinSystemAuxiliaryProviders,
    WinSystemObservationProviders, WinSystemProviders,
};

const HOST_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.system.host");
const CPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.system.cpu");
const MEMORY_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.system.memory");
const STORAGE_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.system.storage");
const NETWORK_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.system.network");
const GPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.system.gpu");
const GPU_ENGINE_ROWS_PROVIDER: ProviderId = ProviderId::borrowed("windows.system.gpu-engines");
const NPU_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("windows.accelerator.npu");
const HARDWARE_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("windows.hardware.inventory");
const CONTAINER_ROLLUP_PROVIDER: ProviderId = ProviderId::borrowed("windows.containers.wsl");
const PROCESS_LIST_PROVIDER: ProviderId = ProviderId::borrowed("windows.process.list");
const PROCESS_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("windows.process.control");
const PROCESS_NETWORK_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.insights.network");
const PROCESS_GPU_PROVIDER: ProviderId = ProviderId::borrowed("windows.process.insights.gpu");
const PROCESS_RESOURCES_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.insights.resources");
const PROCESS_ISOLATION_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.insights.isolation");
const PROCESS_THREADS_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.insights.threads");
const PROCESS_OPEN_FILES_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.insights.open_files");
const PROCESS_ENVIRONMENT_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.insights.environment");
const PROCESS_AFFINITY_PROVIDER: ProviderId = ProviderId::borrowed("windows.process.affinity");
const PROCESS_AFFINITY_CONTROL_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.affinity.control");
const PROCESS_RESOURCE_CONTROL_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.resource.control");
const PROCESS_NETWORK_ESCALATION_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.network.escalation");
const SERVICE_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("windows.service.inventory");
const SERVICE_DEPENDENCIES_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.service.dependencies");
const SERVICE_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("windows.service.control");
const SERVICE_LOG_SNAPSHOT_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.service.logs.snapshot");
const SERVICE_LOG_STREAM_PROVIDER: ProviderId = ProviderId::borrowed("windows.service.logs.stream");
const STARTUP_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("windows.startup.inventory");
const STARTUP_EVIDENCE_PROVIDER: ProviderId = ProviderId::borrowed("windows.startup.evidence");
const STARTUP_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("windows.startup.control");
const SESSION_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("windows.session.inventory");
const SESSION_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("windows.session.control");
const COMMAND_LAUNCH_PROVIDER: ProviderId = ProviderId::borrowed("windows.shell.command");
const RESOURCE_REVEAL_PROVIDER: ProviderId = ProviderId::borrowed("windows.shell.resource-reveal");
const URL_OPEN_PROVIDER: ProviderId = ProviderId::borrowed("windows.shell.url-open");
const DESKTOP_APPEARANCE_PROVIDER: ProviderId = ProviderId::borrowed("windows.desktop.appearance");
const DESKTOP_NOTIFICATION_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.alerts.desktop-notification");
const FIRST_RUN_SETUP_PROVIDER: ProviderId = ProviderId::borrowed("windows.first-run.setup-script");
const DIRECTORY_USAGE_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.storage.directory-usage");
const FILESYSTEM_HEALTH_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.storage.filesystem.registry");
const SMART_OBSERVATION_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.storage.smart.observation");
const SMART_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("windows.storage.smart.control");
const SENSOR_CAPABILITY_PROVIDER: ProviderId = ProviderId::borrowed("windows.sensor.registry");
const POWER_SUPPLY_CAPABILITY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.power-supply.registry");

/// Runtime registry consumed into isolated execution lanes.
///
/// A registry contains capability implementations, not a product SKU. Hardware
/// and optional system services remain runtime-discovered inside providers.
pub struct WindowsProviderRegistry {
    pub(crate) system: WinSystemProviders,
    pub(crate) processes: WinProcessProviders,
    pub(crate) services: WinServiceProviders,
    pub(crate) environment: WinEnvironmentProviders,
    pub(crate) integrations: WinIntegrationProviders,
    pub(crate) storage: WinStorageProviders,
    pub(crate) sensors: WinSensorProviders,
    pub(crate) power: WinPowerProviders,
}

impl WindowsProviderRegistry {
    // The eight arguments are the eight independent application change axes.
    // This is the final OS-owned registry assembly, not one runtime
    // transaction; each field is consumed once into its separate lane family.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        system: WinSystemProviders,
        processes: WinProcessProviders,
        services: WinServiceProviders,
        environment: WinEnvironmentProviders,
        integrations: WinIntegrationProviders,
        storage: WinStorageProviders,
        sensors: WinSensorProviders,
        power: WinPowerProviders,
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

pub(super) fn windows_provider_registry() -> WindowsProviderRegistry {
    WindowsProviderRegistry::new(
        WinSystemProviders::new(
            WinSystemObservationProviders::new(
                ProviderRegistration::<HostTelemetryRequest, _>::new(
                    HOST_TELEMETRY_PROVIDER.clone(),
                    WinHostTelemetryProvider::new(),
                ),
                ProviderRegistration::<CpuTelemetryRequest, _>::new(
                    CPU_TELEMETRY_PROVIDER.clone(),
                    WinCpuTelemetryProvider::new(),
                ),
                ProviderRegistration::<MemoryTelemetryRequest, _>::new(
                    MEMORY_TELEMETRY_PROVIDER.clone(),
                    WinMemoryTelemetryProvider::new(),
                ),
                ProviderRegistration::<StorageTelemetryRequest, _>::new(
                    STORAGE_TELEMETRY_PROVIDER.clone(),
                    WinStorageTelemetryProvider::new(),
                ),
                ProviderRegistration::<NetworkTelemetryRequest, _>::new(
                    NETWORK_TELEMETRY_PROVIDER.clone(),
                    WinNetworkTelemetryProvider::new(),
                ),
                ProviderRegistration::<GpuTelemetryRequest, _>::new(
                    GPU_TELEMETRY_PROVIDER.clone(),
                    WinGpuTelemetryProvider::new(),
                ),
                ProviderRegistration::<ContainerRollupRequest, _>::new(
                    CONTAINER_ROLLUP_PROVIDER.clone(),
                    WinContainerRollupProvider::new(),
                ),
            ),
            WinSystemAuxiliaryProviders::new(
                ProviderRegistration::<HardwareInventoryRequest, _>::new(
                    HARDWARE_INVENTORY_PROVIDER.clone(),
                    WinHardwareInventoryProvider::new(),
                ),
                ProviderRegistration::<GpuEngineRowsRequest, _>::new(
                    GPU_ENGINE_ROWS_PROVIDER.clone(),
                    WinGpuEngineRowsProvider::new(),
                ),
                ProviderRegistration::<NpuInventoryRequest, _>::new(
                    NPU_INVENTORY_PROVIDER.clone(),
                    WinNpuInventoryProvider::new(),
                ),
            ),
        ),
        WinProcessProviders::new(
            WinProcessObservationProviders::new(
                ProviderRegistration::<ProcessListRequest, _>::new(
                    PROCESS_LIST_PROVIDER.clone(),
                    WinProcessListProvider::new(),
                ),
                ProviderRegistration::<ProcessNetworkRequest, _>::new(
                    PROCESS_NETWORK_PROVIDER.clone(),
                    PendingProcessNetworkProvider,
                ),
                ProviderRegistration::<ProcessGpuRequest, _>::new(
                    PROCESS_GPU_PROVIDER.clone(),
                    WinProcessGpuProvider::new(),
                ),
                ProviderRegistration::<ProcessResourcesRequest, _>::new(
                    PROCESS_RESOURCES_PROVIDER.clone(),
                    WinProcessResourcesProvider::new(),
                ),
                ProviderRegistration::<ProcessIsolationRequest, _>::new(
                    PROCESS_ISOLATION_PROVIDER.clone(),
                    PendingProcessIsolationProvider,
                ),
                ProviderRegistration::<ProcessThreadsRequest, _>::new(
                    PROCESS_THREADS_PROVIDER.clone(),
                    WinProcessThreadsProvider,
                ),
                ProviderRegistration::<ProcessAffinityRequest, _>::new(
                    PROCESS_AFFINITY_PROVIDER.clone(),
                    WinProcessAffinityProvider,
                ),
            )
            .with_open_files(ProviderRegistration::<ProcessOpenFilesRequest, _>::new(
                PROCESS_OPEN_FILES_PROVIDER.clone(),
                WinProcessOpenFilesProvider,
            ))
            .with_environment(
                ProviderRegistration::<ProcessEnvironmentRequest, _>::new(
                    PROCESS_ENVIRONMENT_PROVIDER.clone(),
                    WinProcessEnvironmentProvider,
                ),
            ),
            WinProcessControlProviders::new(
                ProviderRegistration::<ProcessAffinityControlRequest, _>::new(
                    PROCESS_AFFINITY_CONTROL_PROVIDER.clone(),
                    WinProcessAffinityControlProvider,
                ),
                ProviderRegistration::<ProcessResourceControlRequest, _>::new(
                    PROCESS_RESOURCE_CONTROL_PROVIDER.clone(),
                    WinProcessResourceControlProvider,
                ),
                ProviderRegistration::<ProcessNetworkEscalationRequest, _>::new(
                    PROCESS_NETWORK_ESCALATION_PROVIDER.clone(),
                    PendingProcessNetworkEscalationProvider,
                ),
                ProviderRegistration::<ProcessControlRequest, _>::new(
                    PROCESS_CONTROL_PROVIDER.clone(),
                    WinProcessControlProvider::new(),
                ),
            ),
        ),
        WinServiceProviders::new(
            ProviderRegistration::new(
                SERVICE_INVENTORY_PROVIDER.clone(),
                WinServiceInventoryProvider::new(),
            ),
            ProviderRegistration::new(
                SERVICE_DEPENDENCIES_PROVIDER.clone(),
                WinServiceDependenciesProvider::new(),
            ),
            ProviderRegistration::new(
                SERVICE_CONTROL_PROVIDER.clone(),
                WinServiceControlProvider::new(),
            ),
            ProviderRegistration::new(
                SERVICE_LOG_SNAPSHOT_PROVIDER.clone(),
                WinServiceLogSnapshotProvider::new(),
            ),
            ProviderRegistration::new(
                SERVICE_LOG_STREAM_PROVIDER.clone(),
                WinServiceLogStreamProvider,
            ),
        ),
        WinEnvironmentProviders::new(
            ProviderRegistration::<StartupInventoryRequest, _>::new(
                STARTUP_INVENTORY_PROVIDER.clone(),
                WinStartupInventoryProvider::new(),
            ),
            ProviderRegistration::<StartupEvidenceRequest, _>::new(
                STARTUP_EVIDENCE_PROVIDER.clone(),
                WinStartupEvidenceProvider,
            ),
            ProviderRegistration::<StartupControlRequest, _>::new(
                STARTUP_CONTROL_PROVIDER.clone(),
                WinStartupControlProvider,
            ),
            ProviderRegistration::<SessionInventoryRequest, _>::new(
                SESSION_INVENTORY_PROVIDER.clone(),
                WinSessionInventoryProvider,
            ),
            ProviderRegistration::<SessionControlRequest, _>::new(
                SESSION_CONTROL_PROVIDER.clone(),
                WinSessionControlProvider,
            ),
        ),
        WinIntegrationProviders::new(
            ProviderRegistration::new(COMMAND_LAUNCH_PROVIDER.clone(), WinCommandLaunchProvider),
            ProviderRegistration::new(
                RESOURCE_REVEAL_PROVIDER.clone(),
                WinResourceRevealProvider::new(),
            ),
            ProviderRegistration::new(URL_OPEN_PROVIDER.clone(), WinUrlOpenProvider),
            ProviderRegistration::new(
                DESKTOP_APPEARANCE_PROVIDER.clone(),
                WinDesktopAppearanceProvider,
            ),
        )
        .with_desktop_notification(ProviderRegistration::<DesktopNotificationRequest, _>::new(
            DESKTOP_NOTIFICATION_PROVIDER.clone(),
            WinDesktopNotificationProvider,
        ))
        .with_setup_script(ProviderRegistration::<SetupScriptRequest, _>::new(
            FIRST_RUN_SETUP_PROVIDER.clone(),
            PendingSetupScriptProvider,
        )),
        WinStorageProviders::new(
            ProviderRegistration::new(
                FILESYSTEM_HEALTH_PROVIDER.clone(),
                WinFilesystemHealthProvider::new(),
            ),
            ProviderRegistration::new(
                SMART_OBSERVATION_PROVIDER.clone(),
                WinSmartSelfTestObservationProvider::new(),
            ),
            ProviderRegistration::new(
                SMART_CONTROL_PROVIDER.clone(),
                WinSmartSelfTestControlProvider::new(),
            ),
        )
        .with_directory_usage(ProviderRegistration::new(
            DIRECTORY_USAGE_PROVIDER.clone(),
            WinDirectoryUsageProvider::new(),
        )),
        WinSensorProviders::new(ProviderRegistration::new(
            SENSOR_CAPABILITY_PROVIDER.clone(),
            WinSensorProvider::new(),
        )),
        WinPowerProviders::new(ProviderRegistration::new(
            POWER_SUPPLY_CAPABILITY_PROVIDER.clone(),
            WinPowerSupplyProvider,
        )),
    )
}

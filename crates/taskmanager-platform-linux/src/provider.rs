//! Linux provider composition.
//!
//! Capability contracts remain platform-neutral. This facade only selects the
//! Linux implementations that runtime-detect available services and hardware.

use std::sync::{Arc, Mutex};

use taskmanager_application::{
    DirectoryUsageRequest, ProcessAffinityControlRequest, ProcessAffinityRequest,
    ProcessControlRequest, ProcessEnvironmentRequest, ProcessGpuRequest, ProcessIsolationRequest,
    ProcessListRequest, ProcessNetworkEscalationRequest, ProcessNetworkRequest,
    ProcessOpenFilesRequest, ProcessResourceControlRequest, ProcessResourcesRequest,
    ProcessThreadsRequest, SessionControlRequest, SessionInventoryRequest, StartupControlRequest,
    StartupEvidenceRequest, StartupInventoryRequest,
};
use taskmanager_core::ProviderId;
#[cfg(not(target_os = "linux"))]
use taskmanager_escalation::polkit::NetLaunchHandle;
use taskmanager_escalation::polkit::NetLauncherProcess;
#[cfg(target_os = "linux")]
use taskmanager_escalation::polkit::PkexecNetLauncher;
use taskmanager_platform_contract::{
    CapabilityId, PlatformAxis, PlatformCapabilitySurface, PlatformSource,
};
use taskmanager_platform_runtime::ProviderRegistration;
use tracing::{info, warn};

use crate::backend::{
    EnvironmentProviders, IntegrationProviders, LinuxProviderRegistry, PowerProviders,
    ProcessControlProviders, ProcessObservationProviders, ProcessProviders, SensorProviders,
    ServiceProviders, StorageProviders,
};
use crate::engine::process::ProcessManager;
use crate::engine::process::telemetry::ProcessNetworkAccountingBackend;
use crate::engine::process::telemetry::{
    ProcessEnvironmentCollector, ProcessGpuCollector, ProcessIsolationCollector,
    ProcessNetworkCollector, ProcessOpenFilesCollector, ProcessResourcesCollector,
    ProcessThreadsCollector,
};
// The AF_PACKET accounting backend exists only behind the Linux boundary crate.
#[cfg(target_os = "linux")]
use crate::engine::process::telemetry::net_accounting;
use crate::engine::runtime_evidence::collect_linux_provider_capability_receipt;
use crate::engine::session::SessionManager;
use crate::engine::startup::StartupManager;
use crate::engine::storage_target::LiveStorageTargetVerifier;

mod directory_usage;
mod environment;
mod gpu_engine_rows;
mod integration;
mod msr_readout;
mod npu_inventory;
mod power;
mod process;
mod process_target;
mod rapl_power;
mod sensor;
mod service;
mod smbios_memory;
mod source_status;
mod storage;
mod system;

use directory_usage::NativeDirectoryUsageProvider;
use environment::{NativeSessionProvider, NativeStartupEvidenceProvider, NativeStartupProvider};
use integration::{
    NativeCommandLaunchProvider, NativeDesktopAppearanceProvider,
    NativeDesktopNotificationProvider, NativeResourceRevealProvider, NativeSetupScriptProvider,
    NativeUrlOpenProvider,
};
use power::NativePowerSupplyProvider;
use process::{
    NativeProcessAffinityControlProvider, NativeProcessAffinityProvider,
    NativeProcessCgroupControlProvider, NativeProcessControlProvider,
    NativeProcessEnvironmentProvider, NativeProcessGpuProvider, NativeProcessIsolationProvider,
    NativeProcessNetworkEscalationProvider, NativeProcessNetworkProvider,
    NativeProcessOpenFilesProvider, NativeProcessResourcesProvider, NativeProcessThreadsProvider,
    ProcfsProcessListProvider,
};
use sensor::NativeSensorProvider;
use service::{
    NativeServiceControlProvider, NativeServiceDependenciesProvider,
    NativeServiceInventoryProvider, NativeServiceLogSnapshotProvider,
    NativeServiceLogStreamProvider,
};
use storage::{
    NativeFilesystemHealthProvider, NativeSmartSelfTestControlProvider,
    NativeSmartSelfTestObservationProvider,
};
use system::native_system_providers;

const COMMAND_LAUNCH_PROVIDER: ProviderId = ProviderId::borrowed("linux.shell.command");
const RESOURCE_REVEAL_PROVIDER: ProviderId = ProviderId::borrowed("linux.shell.resource-reveal");
const URL_OPEN_PROVIDER: ProviderId = ProviderId::borrowed("linux.shell.url-open");
const DESKTOP_APPEARANCE_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.desktop.appearance.composite");
const FIRST_RUN_SETUP_PROVIDER: ProviderId = ProviderId::borrowed("linux.first-run.setup-script");
const DESKTOP_NOTIFICATION_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.alerts.desktop-notification");
const FILESYSTEM_HEALTH_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.storage.filesystem.registry");
const SMART_OBSERVATION_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.storage.smart.observation");
const SMART_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("linux.storage.smart.control");
const DIRECTORY_USAGE_PROVIDER: ProviderId = ProviderId::borrowed("linux.storage.directory-usage");
const SENSOR_CAPABILITY_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.registry");
const POWER_SUPPLY_CAPABILITY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.power-supply.registry");
const SERVICE_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.service.inventory.registry");
const SERVICE_DEPENDENCIES_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.service.dependencies.registry");
const SERVICE_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("linux.service.control.registry");
const SERVICE_LOG_SNAPSHOT_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.service.logs.snapshot");
const SERVICE_LOG_STREAM_PROVIDER: ProviderId = ProviderId::borrowed("linux.service.logs.stream");
const STARTUP_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.startup.inventory.registry");
const STARTUP_EVIDENCE_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.startup.evidence.systemd");
const STARTUP_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("linux.startup.control.registry");
const SESSION_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.session.inventory.registry");
const SESSION_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("linux.session.control.registry");
const PROCESS_LIST_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.procfs");
const PROCESS_CONTROL_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.control");
const PROCESS_NETWORK_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.insights.network");
const PROCESS_NETWORK_ESCALATION_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.insights.network.escalation");
const PROCESS_GPU_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.insights.gpu");
const PROCESS_RESOURCES_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.insights.resources");
const PROCESS_ISOLATION_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.insights.isolation");
const PROCESS_THREADS_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.insights.threads");
const PROCESS_OPEN_FILES_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.insights.open_files");
const PROCESS_ENVIRONMENT_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.insights.environment");
const PROCESS_AFFINITY_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.affinity");
const PROCESS_AFFINITY_CONTROL_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.affinity.control");
const PROCESS_RESOURCE_CONTROL_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.resource.control");

#[cfg(not(target_os = "linux"))]
#[derive(Debug, Default)]
struct UnsupportedNetLauncher;

#[cfg(not(target_os = "linux"))]
impl NetLauncherProcess for UnsupportedNetLauncher {
    fn obtain_fd(&self, _iface_index: u32) -> std::io::Result<NetLaunchHandle> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Linux AF_PACKET escalation is unavailable on this target",
        ))
    }
}

fn new_net_launcher() -> Box<dyn NetLauncherProcess + Send> {
    #[cfg(target_os = "linux")]
    {
        Box::new(PkexecNetLauncher::new())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Box::new(UnsupportedNetLauncher)
    }
}

/// Layer B of the three-axis parity ledger: the capability identities this
/// adapter registers a real provider for.
///
/// The list is declared AT the registration site and must stay in step with
/// [`real_provider_registry`] (and the `native_system_providers` function that
/// builds the system domain): Linux implements every lane it registers, so each
/// entry is `Present` and no lane is registered-pending. The matching proof is
/// `the_linux_capability_surface_is_the_live_catalog_registration_face`, which
/// compares this declaration with the live runtime catalog in both directions.
const REGISTERED_CAPABILITY_SOURCES: &[(CapabilityId, PlatformSource)] = &[
    // -- system telemetry, hardware inventory, accelerators -----------------
    (CapabilityId::TELEMETRY_HOST, PlatformSource::Present),
    (CapabilityId::TELEMETRY_CPU, PlatformSource::Present),
    (CapabilityId::TELEMETRY_MEMORY, PlatformSource::Present),
    (CapabilityId::TELEMETRY_STORAGE, PlatformSource::Present),
    (CapabilityId::TELEMETRY_NETWORK, PlatformSource::Present),
    (CapabilityId::TELEMETRY_GPU, PlatformSource::Present),
    (CapabilityId::TELEMETRY_GPU_ENGINES, PlatformSource::Present),
    (
        CapabilityId::TELEMETRY_MEMORY_SMBIOS,
        PlatformSource::Present,
    ),
    (
        CapabilityId::TELEMETRY_CPU_PACKAGE_POWER,
        PlatformSource::Present,
    ),
    (CapabilityId::TELEMETRY_CPU_MSR, PlatformSource::Present),
    (CapabilityId::ACCELERATOR_NPU, PlatformSource::Present),
    (CapabilityId::HARDWARE_INVENTORY, PlatformSource::Present),
    (CapabilityId::CONTAINERS, PlatformSource::Present),
    // -- process observation and control -------------------------------------
    (CapabilityId::PROCESS_LIST, PlatformSource::Present),
    (
        CapabilityId::PROCESS_INSIGHTS_NETWORK,
        PlatformSource::Present,
    ),
    (CapabilityId::PROCESS_INSIGHTS_GPU, PlatformSource::Present),
    (
        CapabilityId::PROCESS_INSIGHTS_RESOURCES,
        PlatformSource::Present,
    ),
    (
        CapabilityId::PROCESS_INSIGHTS_ISOLATION,
        PlatformSource::Present,
    ),
    (
        CapabilityId::PROCESS_INSIGHTS_THREADS,
        PlatformSource::Present,
    ),
    (
        CapabilityId::PROCESS_INSIGHTS_OPEN_FILES,
        PlatformSource::Present,
    ),
    (
        CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT,
        PlatformSource::Present,
    ),
    (CapabilityId::PROCESS_AFFINITY, PlatformSource::Present),
    (
        CapabilityId::PROCESS_AFFINITY_CONTROL,
        PlatformSource::Present,
    ),
    (CapabilityId::PROCESS_CONTROL, PlatformSource::Present),
    (
        CapabilityId::PROCESS_RESOURCE_CONTROL,
        PlatformSource::Present,
    ),
    (
        CapabilityId::PROCESS_NETWORK_ESCALATION,
        PlatformSource::Present,
    ),
    // -- services, startup, sessions ----------------------------------------
    (CapabilityId::SERVICES, PlatformSource::Present),
    (CapabilityId::SERVICE_DEPENDENCIES, PlatformSource::Present),
    (CapabilityId::SERVICE_CONTROL, PlatformSource::Present),
    (CapabilityId::SERVICE_LOGS, PlatformSource::Present),
    (CapabilityId::SERVICE_LOG_STREAM, PlatformSource::Present),
    (CapabilityId::STARTUP, PlatformSource::Present),
    (CapabilityId::STARTUP_EVIDENCE, PlatformSource::Present),
    (CapabilityId::STARTUP_CONTROL, PlatformSource::Present),
    (CapabilityId::SESSIONS, PlatformSource::Present),
    (CapabilityId::SESSION_CONTROL, PlatformSource::Present),
    // -- shell integration ---------------------------------------------------
    (CapabilityId::COMMAND_LAUNCH, PlatformSource::Present),
    (CapabilityId::RESOURCE_REVEAL, PlatformSource::Present),
    (CapabilityId::URL_OPEN, PlatformSource::Present),
    (CapabilityId::DESKTOP_APPEARANCE, PlatformSource::Present),
    (CapabilityId::DESKTOP_NOTIFY, PlatformSource::Present),
    (CapabilityId::FIRST_RUN_SETUP, PlatformSource::Present),
    // -- storage, sensors, power --------------------------------------------
    (CapabilityId::STORAGE_HEALTH, PlatformSource::Present),
    (CapabilityId::SMART, PlatformSource::Present),
    (CapabilityId::SMART_CONTROL, PlatformSource::Present),
    (CapabilityId::DIRECTORY_USAGE, PlatformSource::Present),
    (CapabilityId::SENSORS, PlatformSource::Present),
    (CapabilityId::POWER_SUPPLIES, PlatformSource::Present),
];

/// The Linux layer-B capability surface: every registered lane is `Present`,
/// every other product-expected identity answers `Absent(Unsupported)` because
/// this adapter registers no source for it.
#[must_use]
pub fn capability_surface() -> PlatformCapabilitySurface {
    PlatformCapabilitySurface::declaring(PlatformAxis::Linux, REGISTERED_CAPABILITY_SOURCES)
}

pub(super) fn real_provider_registry() -> LinuxProviderRegistry {
    log_provider_receipt();
    let (system, storage_target_resolver) = native_system_providers();
    let storage_target_verifier = LiveStorageTargetVerifier::standard();
    // Shared byte-accounting backend for the per-process network capability:
    // the observation provider reads through it, the escalation provider swaps
    // in a backend started from an escalated capture fd when the user grants
    // the OS-native prompt (ADR-023/024/025). On Linux the backend probes
    // CAP_NET_RAW at construction and degrades to RequiresEscalation; the
    // non-Linux compile of this adapter wires the neutral Unsupported backend
    // instead (the escalation request answers typed Unsupported).
    #[cfg(target_os = "linux")]
    let shared_net_accounting: Arc<Mutex<Box<dyn ProcessNetworkAccountingBackend>>> = Arc::new(
        Mutex::new(Box::new(net_accounting::AfPacketAccountingBackend::start(
            std::path::Path::new("/proc"),
            net_accounting::default_route_iface_index(std::path::Path::new("/proc")),
        ))),
    );
    #[cfg(not(target_os = "linux"))]
    let shared_net_accounting: Arc<Mutex<Box<dyn ProcessNetworkAccountingBackend>>> =
        Arc::new(Mutex::new(Box::new(
            crate::engine::process::telemetry::network::UnsupportedNetworkAccountingBackend,
        )));
    LinuxProviderRegistry::new(
        system,
        ProcessProviders::new(
            ProcessObservationProviders::new(
                ProviderRegistration::<ProcessListRequest, _>::new(
                    PROCESS_LIST_PROVIDER.clone(),
                    ProcfsProcessListProvider {
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessNetworkRequest, _>::new(
                    PROCESS_NETWORK_PROVIDER.clone(),
                    NativeProcessNetworkProvider {
                        // ADR-024: per-process byte accounting via the audited
                        // AF_PACKET seam. Probes CAP_NET_RAW at construction —
                        // unprivileged hosts degrade to RequiresEscalation (the
                        // honest "needs escalation" answer). The accounting
                        // backend handle is shared with the escalation provider
                        // below so a granted prompt swaps in a real capture
                        // backend without touching this registration.
                        collector: ProcessNetworkCollector::with_shared_accounting(
                            shared_net_accounting.clone(),
                        ),
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessGpuRequest, _>::new(
                    PROCESS_GPU_PROVIDER.clone(),
                    NativeProcessGpuProvider {
                        collector: ProcessGpuCollector::default(),
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessResourcesRequest, _>::new(
                    PROCESS_RESOURCES_PROVIDER.clone(),
                    NativeProcessResourcesProvider {
                        collector: ProcessResourcesCollector::default(),
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessIsolationRequest, _>::new(
                    PROCESS_ISOLATION_PROVIDER.clone(),
                    NativeProcessIsolationProvider {
                        collector: ProcessIsolationCollector,
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessThreadsRequest, _>::new(
                    PROCESS_THREADS_PROVIDER.clone(),
                    NativeProcessThreadsProvider {
                        collector: ProcessThreadsCollector::default(),
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessOpenFilesRequest, _>::new(
                    PROCESS_OPEN_FILES_PROVIDER.clone(),
                    NativeProcessOpenFilesProvider {
                        collector: ProcessOpenFilesCollector,
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessEnvironmentRequest, _>::new(
                    PROCESS_ENVIRONMENT_PROVIDER.clone(),
                    NativeProcessEnvironmentProvider {
                        collector: ProcessEnvironmentCollector,
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessAffinityRequest, _>::new(
                    PROCESS_AFFINITY_PROVIDER.clone(),
                    NativeProcessAffinityProvider,
                ),
            ),
            ProcessControlProviders::new(
                ProviderRegistration::<ProcessAffinityControlRequest, _>::new(
                    PROCESS_AFFINITY_CONTROL_PROVIDER.clone(),
                    NativeProcessAffinityControlProvider {
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessControlRequest, _>::new(
                    PROCESS_CONTROL_PROVIDER.clone(),
                    NativeProcessControlProvider {
                        process_manager: ProcessManager::new(),
                    },
                ),
                ProviderRegistration::<ProcessResourceControlRequest, _>::new(
                    PROCESS_RESOURCE_CONTROL_PROVIDER.clone(),
                    NativeProcessCgroupControlProvider::new(),
                ),
                ProviderRegistration::<ProcessNetworkEscalationRequest, _>::new(
                    PROCESS_NETWORK_ESCALATION_PROVIDER.clone(),
                    {
                        // The OS-native prompt (pkexec) is offered only when the
                        // user explicitly requests it via the UI control - the
                        // launcher is injected but never auto-invoked.
                        #[cfg(target_os = "linux")]
                        let iface_index = net_accounting::default_route_iface_index(
                            std::path::Path::new("/proc"),
                        );
                        // No AF_PACKET route on this compile target; the request
                        // path below answers typed Unsupported before ever
                        // touching the index.
                        #[cfg(not(target_os = "linux"))]
                        let iface_index = 0;
                        NativeProcessNetworkEscalationProvider::new(
                            shared_net_accounting.clone(),
                            std::path::PathBuf::from("/proc"),
                            iface_index,
                            new_net_launcher(),
                        )
                    },
                ),
            ),
        ),
        ServiceProviders::new(
            ProviderRegistration::new(
                SERVICE_INVENTORY_PROVIDER.clone(),
                NativeServiceInventoryProvider,
            ),
            ProviderRegistration::new(
                SERVICE_DEPENDENCIES_PROVIDER.clone(),
                NativeServiceDependenciesProvider,
            ),
            ProviderRegistration::new(
                SERVICE_CONTROL_PROVIDER.clone(),
                NativeServiceControlProvider,
            ),
            ProviderRegistration::new(
                SERVICE_LOG_SNAPSHOT_PROVIDER.clone(),
                NativeServiceLogSnapshotProvider,
            ),
            ProviderRegistration::new(
                SERVICE_LOG_STREAM_PROVIDER.clone(),
                NativeServiceLogStreamProvider,
            ),
        ),
        EnvironmentProviders::new(
            ProviderRegistration::<StartupInventoryRequest, _>::new(
                STARTUP_INVENTORY_PROVIDER.clone(),
                NativeStartupProvider {
                    manager: StartupManager::new(),
                },
            ),
            ProviderRegistration::<StartupEvidenceRequest, _>::new(
                STARTUP_EVIDENCE_PROVIDER.clone(),
                NativeStartupEvidenceProvider,
            ),
            ProviderRegistration::<StartupControlRequest, _>::new(
                STARTUP_CONTROL_PROVIDER.clone(),
                NativeStartupProvider {
                    manager: StartupManager::new(),
                },
            ),
            ProviderRegistration::<SessionInventoryRequest, _>::new(
                SESSION_INVENTORY_PROVIDER.clone(),
                NativeSessionProvider {
                    manager: SessionManager::new(),
                },
            ),
            ProviderRegistration::<SessionControlRequest, _>::new(
                SESSION_CONTROL_PROVIDER.clone(),
                NativeSessionProvider {
                    manager: SessionManager::new(),
                },
            ),
        ),
        IntegrationProviders::new(
            ProviderRegistration::new(COMMAND_LAUNCH_PROVIDER.clone(), NativeCommandLaunchProvider),
            ProviderRegistration::new(
                RESOURCE_REVEAL_PROVIDER.clone(),
                NativeResourceRevealProvider {
                    process_manager: ProcessManager::new(),
                },
            ),
            ProviderRegistration::new(URL_OPEN_PROVIDER.clone(), NativeUrlOpenProvider),
            ProviderRegistration::new(
                DESKTOP_APPEARANCE_PROVIDER.clone(),
                NativeDesktopAppearanceProvider,
            ),
        )
        .with_desktop_notification(ProviderRegistration::new(
            DESKTOP_NOTIFICATION_PROVIDER.clone(),
            NativeDesktopNotificationProvider,
        ))
        .with_setup_script(ProviderRegistration::new(
            FIRST_RUN_SETUP_PROVIDER.clone(),
            NativeSetupScriptProvider,
        )),
        StorageProviders::new(
            ProviderRegistration::new(
                FILESYSTEM_HEALTH_PROVIDER.clone(),
                NativeFilesystemHealthProvider,
            ),
            ProviderRegistration::new(
                SMART_OBSERVATION_PROVIDER.clone(),
                NativeSmartSelfTestObservationProvider {
                    target_resolver: storage_target_resolver.clone(),
                    target_verifier: storage_target_verifier.clone(),
                },
            ),
            ProviderRegistration::new(
                SMART_CONTROL_PROVIDER.clone(),
                NativeSmartSelfTestControlProvider {
                    target_resolver: storage_target_resolver,
                    target_verifier: storage_target_verifier,
                },
            ),
            ProviderRegistration::<DirectoryUsageRequest, _>::new(
                DIRECTORY_USAGE_PROVIDER.clone(),
                NativeDirectoryUsageProvider::new(),
            ),
        ),
        SensorProviders::new(ProviderRegistration::new(
            SENSOR_CAPABILITY_PROVIDER.clone(),
            NativeSensorProvider,
        )),
        PowerProviders::new(ProviderRegistration::new(
            POWER_SUPPLY_CAPABILITY_PROVIDER.clone(),
            NativePowerSupplyProvider,
        )),
    )
}

fn log_provider_receipt() {
    let receipt = collect_linux_provider_capability_receipt();
    match serde_json::to_string(&receipt) {
        Ok(json) => info!(
            target: "taskmanager.provider_evidence",
            receipt = %json,
            "LINUX_PROVIDER_CAPABILITY_RECEIPT"
        ),
        Err(error) => warn!(
            target: "taskmanager.provider_evidence",
            %error,
            "could not serialize Linux provider capability receipt"
        ),
    }
}

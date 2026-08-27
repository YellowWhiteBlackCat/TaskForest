//! Windows process-domain providers built on safe wrapper crates plus the
//! audited exact-process boundary for destructive control.
//!
//! List/resources use `sysinfo` (SystemProcessInformation behind a safe API);
//! per-process GPU facts come from the WDDM PDH counters (Task Manager's own
//! source) with `nvml-wrapper` as the fallback; thread details come from the
//! audited ToolHelp32/GetThreadTimes boundary; suspend/resume use the audited
//! per-thread `SuspendThread`/`ResumeThread` boundary. The per-fd open-files
//! insight walks the audited system-handle-table boundary (route B, ADR-018):
//! only File-type objects are reported, and sockets stay to the connections
//! insight, which owns that fact on Windows.

use taskmanager_application::{
    ProcessAffinityControlRequest, ProcessAffinityRequest, ProcessControlRequest,
    ProcessEnvironmentRequest, ProcessGpuRequest, ProcessIsolationRequest, ProcessListRequest,
    ProcessNetworkEscalationRequest, ProcessNetworkRequest, ProcessOpenFilesRequest,
    ProcessResourceControlRequest, ProcessResourcesRequest, ProcessThreadsRequest,
};
use taskmanager_core::{
    DeviceState, FailureKind, FrozenProcessIdentity, PriorityTier, ProcessBatchAction,
    ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult, ProcessGpuDevice,
    ProcessGpuEngines, ProcessGpuSnapshot, ProcessIdentity, ProcessInsightSnapshot,
    ProcessResourceObservations, ProcessResourceSnapshot, ProcessSignal, ProviderId,
    ResourceObservation,
};
use taskmanager_platform_contract::{ProviderFailure, SourceOutcome, SourceStatus};
use taskmanager_platform_provider::{
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessControlProvider,
    ProcessEnvironmentProvider, ProcessGpuProvider, ProcessIsolationProvider, ProcessListProvider,
    ProcessNetworkEscalationProvider, ProcessNetworkProvider, ProcessOpenFilesProvider,
    ProcessResourceControlProvider, ProcessResourcesProvider, ProcessThreadsProvider,
};
use taskmanager_platform_runtime::{
    ProcessControlExecutors, ProcessObservationExecutors, ProcessProviderBindings,
    ProcessProviderBindingsInput, ProviderRegistration,
};

mod control;
mod insights;
mod list;

pub use control::*;
pub use insights::{
    WinProcessEnvironmentProvider, WinProcessOpenFilesProvider, WinProcessThreadsProvider,
};
pub use list::WinProcessListProvider;

const PROCESS_LIST_PROVIDER: ProviderId = ProviderId::borrowed("windows.process.list.sysinfo");
const PROCESS_RESOURCE_MEMORY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.resources.sysinfo");
const PROCESS_RESOURCE_LIMITS_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.resources.job-object");

fn source(provider: ProviderId, item_count: usize) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Available,
        item_count,
    }
}

fn unsupported_resource<T>() -> ResourceObservation<T> {
    ResourceObservation::unavailable(FailureKind::Unsupported)
}

fn resource_snapshot(memory_usage_bytes: u64, observed_at_ms: u64) -> ProcessResourceSnapshot {
    ProcessResourceSnapshot::from_observations(
        DeviceState::healthy(observed_at_ms),
        ProcessResourceObservations {
            limits: unsupported_resource(),
            resource_groups: unsupported_resource(),
            memory_usage_bytes: ResourceObservation::current(memory_usage_bytes, observed_at_ms),
            memory_limit: unsupported_resource(),
            cpu_time_quota_micros: unsupported_resource(),
            cpu_time_period_micros: unsupported_resource(),
            process_count: unsupported_resource(),
            process_limit: unsupported_resource(),
        },
        vec![
            source(PROCESS_RESOURCE_MEMORY_PROVIDER, 1),
            SourceStatus {
                provider: PROCESS_RESOURCE_LIMITS_PROVIDER,
                outcome: SourceOutcome::Unavailable(FailureKind::Unsupported),
                item_count: 0,
            },
        ],
    )
}

/// Resolve and verify the provider-issued process identity before any
/// target-scoped read. A PID is only a locator; the creation-time token is
/// the authority. Callers that perform a multi-step native query validate a
/// second time after the read so PID reuse cannot publish replacement facts.
pub(crate) fn validate_process_target(
    target: &FrozenProcessIdentity,
) -> Result<u64, ProviderFailure> {
    let expected = target
        .authoritative_start_token()
        .ok_or(ProviderFailure::IdentityChanged)?;
    #[cfg(windows)]
    {
        let actual = taskmanager_windows_api::process_creation_time_100ns(target.pid)
            .map_err(map_windows_api_failure)?;
        if actual != expected {
            return Err(ProviderFailure::IdentityChanged);
        }
        Ok(expected)
    }
    #[cfg(not(windows))]
    {
        let _ = expected;
        Err(ProviderFailure::Unsupported)
    }
}

fn validate_process_target_after(
    target: &FrozenProcessIdentity,
    expected: u64,
) -> Result<(), ProviderFailure> {
    #[cfg(windows)]
    {
        let actual = taskmanager_windows_api::process_creation_time_100ns(target.pid)
            .map_err(map_windows_api_failure)?;
        if actual != expected {
            return Err(ProviderFailure::IdentityChanged);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (target, expected);
        Err(ProviderFailure::Unsupported)
    }
}

/// Per-process resources from `sysinfo`: current memory usage is real; job
/// limits/membership have no safe wrapper yet and stay empty.
pub struct WinProcessResourcesProvider {
    system: sysinfo::System,
}

impl WinProcessResourcesProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
        }
    }
}

impl ProcessResourcesProvider for WinProcessResourcesProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure> {
        let expected = validate_process_target(target)?;
        let pid = sysinfo::Pid::from_u32(target.pid);
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        let memory_usage_bytes = self
            .system
            .processes()
            .get(&pid)
            .ok_or(ProviderFailure::IdentityChanged)
            .map(|process| process.memory())?;
        validate_process_target_after(target, expected)?;
        let snapshot = resource_snapshot(memory_usage_bytes, observed_at_ms);
        Ok(ProcessInsightSnapshot {
            identity: snapshot_identity(target),
            value: snapshot,
        })
    }
}

fn snapshot_identity(target: &FrozenProcessIdentity) -> ProcessIdentity {
    ProcessIdentity {
        pid: target.pid,
        start_token: target.authoritative_start_token().unwrap_or(0),
    }
}

/// The DXGI adapter identity for a WDDM adapter LUID, mirroring the identity
/// authority `provider::system::gpu::dxgi_adapter_identity(luid, false)`
/// (which is `pub(super)` to the system module and cannot be imported here).
/// The two formats must stay byte-identical or a frontend device row stops
/// addressing the adapter its per-process rows came from. PDH GPU counters key
/// GPU adapters only, so the `windows:gpu:` prefix never carries an NPU LUID.
#[cfg(windows)]
fn dxgi_gpu_process_identity(luid: u64) -> String {
    format!("windows:gpu:dxgi:{luid:016x}")
}

/// Per-process GPU facts. The WDDM performance counters (PDH `\GPU Engine(*)`
/// and `\GPU Process Memory(*)`, Task Manager's own sources) are primary: they
/// cover every adapter and report utilization as well as memory. NVML stays as
/// the fallback for hosts where the counter set is missing; only a successful
/// source query may authoritatively publish an empty process-device set.
pub struct WinProcessGpuProvider {
    nvml: Option<nvml_wrapper::Nvml>,
}

impl WinProcessGpuProvider {
    pub fn new() -> Self {
        let nvml = nvml_wrapper::Nvml::init().ok();
        Self { nvml }
    }

    /// Per-adapter device rows from the WDDM counters; any counter failure is
    /// a typed error the caller answers with the NVML fallback.
    fn pdh_process_devices(&self, pid: u32) -> Result<Vec<ProcessGpuDevice>, ProviderFailure> {
        #[cfg(windows)]
        {
            use std::collections::BTreeMap;
            use taskmanager_windows_api::{query_gpu_engine_instances, query_gpu_process_memory};

            let engine_rows = query_gpu_engine_instances().map_err(map_windows_api_failure)?;
            let memory_rows = query_gpu_process_memory().map_err(map_windows_api_failure)?;

            // Task Manager's per-process GPU % column sums the process's
            // engine instances and clamps at 100 (the busiest-engine rule
            // governs the system graph, not the per-process column), so a
            // process driving 3D + Copy + VideoDecode concurrently tops out
            // at 100 instead of reporting 200.
            let mut utilization_by_luid: BTreeMap<u64, f32> = BTreeMap::new();
            for sample in engine_rows.iter().filter(|sample| sample.pid == pid) {
                *utilization_by_luid.entry(sample.luid).or_insert(0.0) += sample.utilization_pct;
            }
            let mut memory_by_luid: BTreeMap<u64, u64> = BTreeMap::new();
            for sample in memory_rows.iter().filter(|sample| sample.pid == pid) {
                memory_by_luid.insert(sample.luid, sample.dedicated_bytes);
            }

            let mut devices = Vec::new();
            for (luid, utilization) in utilization_by_luid {
                devices.push(ProcessGpuDevice {
                    device_id: dxgi_gpu_process_identity(luid),
                    utilization_pct: Some(utilization.clamp(0.0, 100.0)),
                    // Dedicated usage is Task Manager's "Dedicated GPU memory"
                    // column; the contract's single memory field carries it,
                    // and shared usage stays unreported until it grows one.
                    memory_bytes: memory_by_luid.remove(&luid),
                    engine_time_ns: None,
                });
            }
            for (luid, dedicated_bytes) in memory_by_luid {
                // Allocations without an engine row: the engine source was
                // queried and reported no activity for this pid, but the
                // boundary drops zero-valued rows, so an explicit 0% would be
                // a guess — utilization stays an honest absence.
                devices.push(ProcessGpuDevice {
                    device_id: dxgi_gpu_process_identity(luid),
                    memory_bytes: Some(dedicated_bytes),
                    utilization_pct: None,
                    engine_time_ns: None,
                });
            }
            Ok(devices)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            Err(ProviderFailure::Unsupported)
        }
    }

    /// NVML-only fallback, verbatim from the pre-PDH provider: memory from
    /// `running_graphics_processes`, utilization unavailable.
    fn nvml_process_devices(&self, pid: u32) -> Result<Vec<ProcessGpuDevice>, ProviderFailure> {
        let nvml = self
            .nvml
            .as_ref()
            .ok_or(ProviderFailure::MissingDependency)?;
        let count = nvml
            .device_count()
            .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
        let mut devices = Vec::new();
        let mut successful_device_queries = 0_usize;
        for index in 0..count {
            let device = nvml
                .device_by_index(index)
                .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
            let graphics = device
                .running_graphics_processes()
                .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
            successful_device_queries += 1;
            if let Some(info) = graphics.iter().find(|info| info.pid == pid)
                    // Under WDDM the NVIDIA driver cannot account per-process
                    // memory (NVML reports NOT_AVAILABLE by design); only a
                    // real `Used(bytes)` reading becomes a device row.
                    && let nvml_wrapper::enums::device::UsedGpuMemory::Used(bytes) =
                        info.used_gpu_memory
            {
                let stable_id = device
                    .uuid()
                    .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
                devices.push(ProcessGpuDevice {
                    device_id: format!("windows:gpu:nvml:{stable_id}"),
                    memory_bytes: Some(bytes),
                    utilization_pct: None,
                    engine_time_ns: None,
                });
            }
        }
        if count > 0 && successful_device_queries == 0 {
            return Err(ProviderFailure::TemporarilyUnavailable);
        }
        Ok(devices)
    }
}

impl ProcessGpuProvider for WinProcessGpuProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure> {
        let expected = validate_process_target(target)?;
        let devices = match self.pdh_process_devices(target.pid) {
            Ok(devices) => devices,
            // A failed WDDM counter query (no GPU / missing counter set, or a
            // transient PDH failure) falls back to the NVML-only source; its
            // own typed failure then surfaces without fabricated rows.
            Err(_) => self.nvml_process_devices(target.pid)?,
        };
        validate_process_target_after(target, expected)?;
        Ok(ProcessInsightSnapshot {
            identity: snapshot_identity(target),
            value: ProcessGpuSnapshot {
                state: DeviceState::healthy(observed_at_ms),
                devices,
                engines: ProcessGpuEngines::default(),
            },
        })
    }
}

#[cfg(windows)]
fn map_windows_api_failure(error: taskmanager_windows_api::WindowsApiError) -> ProviderFailure {
    match error {
        taskmanager_windows_api::WindowsApiError::Unsupported => ProviderFailure::Unsupported,
        taskmanager_windows_api::WindowsApiError::PermissionDenied => {
            ProviderFailure::PermissionDenied
        }
        taskmanager_windows_api::WindowsApiError::IdentityChanged
        | taskmanager_windows_api::WindowsApiError::InvalidInput => {
            ProviderFailure::IdentityChanged
        }
        taskmanager_windows_api::WindowsApiError::ResourceLimit
        | taskmanager_windows_api::WindowsApiError::InvalidText
        | taskmanager_windows_api::WindowsApiError::QueryFailed => {
            ProviderFailure::TemporarilyUnavailable
        }
    }
}

/// Process network connection provider powered by native IP Helper tables.
pub struct WinProcessNetworkProvider;

impl ProcessNetworkProvider for WinProcessNetworkProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<taskmanager_core::ProcessNetworkSnapshot>, ProviderFailure>
    {
        #[cfg(windows)]
        {
            use std::net::SocketAddr;
            use taskmanager_core::core::device_state::DeviceState;
            use taskmanager_core::core::process_telemetry::{
                ConnectionAddressFamily, ConnectionEndpoint, ConnectionState, ConnectionTransport,
                ProcessConnection, ProcessNetworkSnapshot,
            };
            use taskmanager_windows_api::{
                WindowsTcpState, WindowsTransportProtocol, query_process_network_connections,
            };

            let expected = validate_process_target(target)?;
            let target_pid = target.pid;
            let all_connections =
                query_process_network_connections().map_err(map_windows_api_failure)?;

            let mut connections = Vec::new();

            for conn in all_connections.into_iter().filter(|c| c.pid == target_pid) {
                let transport = match conn.protocol {
                    WindowsTransportProtocol::Tcp => ConnectionTransport::Tcp,
                    WindowsTransportProtocol::Udp => ConnectionTransport::Udp,
                };
                let family = match conn.local_addr {
                    SocketAddr::V4(_) => ConnectionAddressFamily::Ipv4,
                    SocketAddr::V6(_) => ConnectionAddressFamily::Ipv6,
                };
                let state = match conn.state {
                    WindowsTcpState::Closed => ConnectionState::Closed,
                    WindowsTcpState::Listen => ConnectionState::Listen,
                    WindowsTcpState::SynSent => ConnectionState::SynSent,
                    WindowsTcpState::SynReceived => ConnectionState::SynReceived,
                    WindowsTcpState::Established => ConnectionState::Established,
                    WindowsTcpState::FinWait1 => ConnectionState::FinWait1,
                    WindowsTcpState::FinWait2 => ConnectionState::FinWait2,
                    WindowsTcpState::CloseWait => ConnectionState::CloseWait,
                    WindowsTcpState::Closing => ConnectionState::Closing,
                    WindowsTcpState::LastAck => ConnectionState::LastAck,
                    WindowsTcpState::TimeWait => ConnectionState::TimeWait,
                    WindowsTcpState::DeleteTcb | WindowsTcpState::Unknown => {
                        if conn.protocol == WindowsTransportProtocol::Udp {
                            ConnectionState::Unconnected
                        } else {
                            ConnectionState::Unknown
                        }
                    }
                };
                let remote = match conn.remote_addr {
                    Some(addr) => ConnectionEndpoint::Ip(addr),
                    None => ConnectionEndpoint::Unspecified,
                };

                connections.push(ProcessConnection {
                    transport,
                    family,
                    local: ConnectionEndpoint::Ip(conn.local_addr),
                    remote,
                    state,
                    provider_key: None,
                });
            }

            let snapshot = ProcessNetworkSnapshot {
                state: DeviceState::healthy(observed_at_ms),
                connections,
                rx_bytes_per_sec: None,
                tx_bytes_per_sec: None,
                traffic_state: DeviceState {
                    status: taskmanager_core::DeviceStatus::Unsupported,
                    last_success_ms: None,
                },
                traffic_failure: Some(FailureKind::Unsupported),
                traffic_provider: None,
            };

            validate_process_target_after(target, expected)?;

            Ok(ProcessInsightSnapshot {
                identity: snapshot_identity(target),
                value: snapshot,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::Unsupported)
        }
    }
}

pub struct PendingProcessNetworkProvider;

impl ProcessNetworkProvider for PendingProcessNetworkProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<taskmanager_core::ProcessNetworkSnapshot>, ProviderFailure>
    {
        WinProcessNetworkProvider.observe(target, observed_at_ms)
    }
}

/// Process security token and isolation facts from OpenProcessToken.
pub struct WinProcessIsolationProvider;

impl ProcessIsolationProvider for WinProcessIsolationProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<taskmanager_core::ProcessIsolation>, ProviderFailure> {
        #[cfg(windows)]
        {
            let expected = validate_process_target(target)?;
            let raw =
                taskmanager_windows_api::process_isolation(target.pid).map_err(
                    |err| match err {
                        taskmanager_windows_api::WindowsApiError::PermissionDenied => {
                            ProviderFailure::PermissionDenied
                        }
                        taskmanager_windows_api::WindowsApiError::IdentityChanged => {
                            ProviderFailure::IdentityChanged
                        }
                        taskmanager_windows_api::WindowsApiError::Unsupported => {
                            ProviderFailure::Unsupported
                        }
                        _ => ProviderFailure::ProviderFault,
                    },
                )?;
            validate_process_target_after(target, expected)?;

            let sandboxed = Some(
                raw.is_app_container
                    || matches!(
                        raw.integrity_level,
                        Some(
                            taskmanager_windows_api::WindowsIntegrityLevel::Untrusted
                                | taskmanager_windows_api::WindowsIntegrityLevel::Low,
                        )
                    ),
            );
            let kind = if raw.is_app_container {
                Some(taskmanager_core::IsolationKind::OtherContainer)
            } else {
                None
            };

            let value = taskmanager_core::ProcessIsolation {
                state: DeviceState::healthy(observed_at_ms),
                kind,
                container_id: None,
                sandboxed,
            };

            Ok(ProcessInsightSnapshot {
                identity: snapshot_identity(target),
                value,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::Unsupported)
        }
    }
}

pub struct PendingProcessIsolationProvider;

impl ProcessIsolationProvider for PendingProcessIsolationProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<taskmanager_core::ProcessIsolation>, ProviderFailure> {
        WinProcessIsolationProvider.observe(target, observed_at_ms)
    }
}

pub struct WinProcessAffinityProvider;

impl ProcessAffinityProvider for WinProcessAffinityProvider {
    fn affinity(&mut self, target: &FrozenProcessIdentity) -> Result<Vec<u32>, ProviderFailure> {
        let expected = validate_process_target(target)?;
        let affinity =
            taskmanager_windows_api::process_affinity(target.pid).map_err(|err| match err {
                taskmanager_windows_api::WindowsApiError::PermissionDenied => {
                    ProviderFailure::PermissionDenied
                }
                taskmanager_windows_api::WindowsApiError::IdentityChanged => {
                    ProviderFailure::IdentityChanged
                }
                taskmanager_windows_api::WindowsApiError::Unsupported => {
                    ProviderFailure::Unsupported
                }
                _ => ProviderFailure::ProviderFault,
            })?;
        validate_process_target_after(target, expected)?;
        Ok(affinity)
    }
}

/// Windows process provider composition grouped by scheduling responsibility.
pub struct WinProcessObservationProviders {
    pub(crate) list: ProviderRegistration<ProcessListRequest, Box<dyn ProcessListProvider>>,
    pub(crate) network:
        ProviderRegistration<ProcessNetworkRequest, Box<dyn ProcessNetworkProvider>>,
    pub(crate) gpu: ProviderRegistration<ProcessGpuRequest, Box<dyn ProcessGpuProvider>>,
    pub(crate) resources:
        ProviderRegistration<ProcessResourcesRequest, Box<dyn ProcessResourcesProvider>>,
    pub(crate) isolation:
        ProviderRegistration<ProcessIsolationRequest, Box<dyn ProcessIsolationProvider>>,
    pub(crate) threads:
        ProviderRegistration<ProcessThreadsRequest, Box<dyn ProcessThreadsProvider>>,
    pub(crate) affinity:
        ProviderRegistration<ProcessAffinityRequest, Box<dyn ProcessAffinityProvider>>,
    pub(crate) open_files:
        Option<ProviderRegistration<ProcessOpenFilesRequest, Box<dyn ProcessOpenFilesProvider>>>,
    pub(crate) environment: Option<
        ProviderRegistration<ProcessEnvironmentRequest, Box<dyn ProcessEnvironmentProvider>>,
    >,
}

impl WinProcessObservationProviders {
    // The seven arguments are the seven independent process-observation
    // capabilities; grouping them would recreate the aggregate provider bag
    // this registry exists to prevent (same tradeoff as the platform
    // registries and the runtime bindings).
    #[must_use]
    pub fn new<L, N, G, R, I, T, A>(
        list: ProviderRegistration<ProcessListRequest, L>,
        network: ProviderRegistration<ProcessNetworkRequest, N>,
        gpu: ProviderRegistration<ProcessGpuRequest, G>,
        resources: ProviderRegistration<ProcessResourcesRequest, R>,
        isolation: ProviderRegistration<ProcessIsolationRequest, I>,
        threads: ProviderRegistration<ProcessThreadsRequest, T>,
        affinity: ProviderRegistration<ProcessAffinityRequest, A>,
    ) -> Self
    where
        L: ProcessListProvider,
        N: ProcessNetworkProvider,
        G: ProcessGpuProvider,
        R: ProcessResourcesProvider,
        I: ProcessIsolationProvider,
        T: ProcessThreadsProvider,
        A: ProcessAffinityProvider,
    {
        Self {
            list: list.map_provider(|provider| Box::new(provider) as Box<dyn ProcessListProvider>),
            network: network
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessNetworkProvider>),
            gpu: gpu.map_provider(|provider| Box::new(provider) as Box<dyn ProcessGpuProvider>),
            resources: resources
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessResourcesProvider>),
            isolation: isolation
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessIsolationProvider>),
            threads: threads
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessThreadsProvider>),
            affinity: affinity
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessAffinityProvider>),
            open_files: None,
            environment: None,
        }
    }

    /// Attach the optional OpenFiles insight facet (the per-fd listing).
    /// Registered as a pending provider on Windows so the capability keeps an
    /// honest catalog descriptor typed `Unsupported` instead of being absent
    /// from enumeration (mirrors `with_open_files` on the Linux/macOS
    /// adapters).
    #[must_use]
    pub fn with_open_files<O>(
        mut self,
        open_files: ProviderRegistration<ProcessOpenFilesRequest, O>,
    ) -> Self
    where
        O: ProcessOpenFilesProvider,
    {
        self.open_files = Some(
            open_files
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessOpenFilesProvider>),
        );
        self
    }

    /// Attach the optional environment insight facet (variables + working
    /// directory). Wired through the runtime's environment facet like the
    /// Linux adapter's `with_environment` composition.
    #[must_use]
    pub fn with_environment<E>(
        mut self,
        environment: ProviderRegistration<ProcessEnvironmentRequest, E>,
    ) -> Self
    where
        E: ProcessEnvironmentProvider,
    {
        self.environment =
            Some(environment.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessEnvironmentProvider>
            }));
        self
    }

    pub(crate) fn into_runtime(self) -> ProcessObservationExecutors {
        let Self {
            list,
            network,
            gpu,
            resources,
            isolation,
            threads,
            affinity,
            open_files,
            environment,
        } = self;
        let mut list = list.into_provider();
        let mut network = network.into_provider();
        let mut gpu = gpu.into_provider();
        let mut resources = resources.into_provider();
        let mut isolation = isolation.into_provider();
        let mut threads = threads.into_provider();
        let mut affinity = affinity.into_provider();
        let executors = ProcessObservationExecutors::new(
            move |observed_at_ms| list.refresh(observed_at_ms),
            move |target, observed_at_ms| network.observe(&target, observed_at_ms),
            move |target, observed_at_ms| gpu.observe(&target, observed_at_ms),
            move |target, observed_at_ms| resources.observe(&target, observed_at_ms),
            move |target, observed_at_ms| isolation.observe(&target, observed_at_ms),
            move |target, observed_at_ms| threads.observe(&target, observed_at_ms),
            move |target| affinity.affinity(&target),
        );
        let executors = match open_files {
            Some(open_files) => {
                let mut open_files = open_files.into_provider();
                executors.with_open_files(move |target, observed_at_ms| {
                    open_files.observe(&target, observed_at_ms)
                })
            }
            None => executors,
        };
        match environment {
            Some(environment) => {
                let mut environment = environment.into_provider();
                executors.with_environment(move |target, observed_at_ms| {
                    environment.observe(&target, observed_at_ms)
                })
            }
            None => executors,
        }
    }
}

/// Windows process provider composition.
pub struct WinProcessProviders {
    observations: WinProcessObservationProviders,
    controls: WinProcessControlProviders,
}

impl WinProcessProviders {
    #[must_use]
    pub const fn new(
        observations: WinProcessObservationProviders,
        controls: WinProcessControlProviders,
    ) -> Self {
        Self {
            observations,
            controls,
        }
    }

    pub(crate) fn runtime_bindings(&self) -> ProcessProviderBindings {
        let bindings = ProcessProviderBindings::new(ProcessProviderBindingsInput {
            list: self.observations.list.binding(),
            control: self.controls.control.binding(),
            network: self.observations.network.binding(),
            gpu: self.observations.gpu.binding(),
            resources: self.observations.resources.binding(),
            isolation: self.observations.isolation.binding(),
            threads: self.observations.threads.binding(),
            affinity: self.observations.affinity.binding(),
            affinity_control: self.controls.affinity_control.binding(),
            resource_control: self.controls.resource_control.binding(),
            network_escalation: self.controls.network_escalation.binding(),
        });
        let bindings = match &self.observations.open_files {
            Some(open_files) => bindings.with_open_files(open_files),
            None => bindings,
        };
        match &self.observations.environment {
            Some(environment) => bindings.with_environment(environment),
            None => bindings,
        }
    }

    pub(crate) fn into_runtime(self) -> taskmanager_platform_runtime::ProcessExecutors {
        taskmanager_platform_runtime::ProcessExecutors::new(
            self.observations.into_runtime(),
            self.controls.into_runtime(),
        )
    }
}

#[cfg(test)]
#[path = "../../tests/headless/provider/process.rs"]
mod tests;

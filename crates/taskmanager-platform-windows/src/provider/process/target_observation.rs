//! Windows process target-scoped observation providers.

use super::*;
use sysinfo::{Pid, ProcessesToUpdate, System};
use taskmanager_core::core::device_state::DeviceState;
#[cfg(windows)]
use taskmanager_core::{DeviceStatus, IsolationKind};
use taskmanager_core::{ProcessIsolation, ProcessNetworkSnapshot};
use taskmanager_windows_api::{WindowsApiError, process_affinity};
#[cfg(windows)]
use taskmanager_windows_api::{WindowsIntegrityLevel, process_isolation};

/// Per-process resources from `sysinfo`: current memory usage is real; job
/// limits/membership have no safe wrapper yet and stay empty.
pub struct WinProcessResourcesProvider {
    system: System,
}

impl WinProcessResourcesProvider {
    pub fn new() -> Self {
        Self {
            system: System::new(),
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
        let pid = Pid::from_u32(target.pid);
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
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
fn map_windows_api_failure(error: WindowsApiError) -> ProviderFailure {
    match error {
        WindowsApiError::Unsupported => ProviderFailure::Unsupported,
        WindowsApiError::PermissionDenied => ProviderFailure::PermissionDenied,
        WindowsApiError::IdentityChanged | WindowsApiError::InvalidInput => {
            ProviderFailure::IdentityChanged
        }
        WindowsApiError::ResourceLimit
        | WindowsApiError::InvalidText
        | WindowsApiError::QueryFailed => ProviderFailure::TemporarilyUnavailable,
    }
}

/// Process network connection provider powered by native IP Helper tables.
pub struct WinProcessNetworkProvider;

impl ProcessNetworkProvider for WinProcessNetworkProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure> {
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
                    status: DeviceStatus::Unsupported,
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
    ) -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure> {
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
    ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure> {
        #[cfg(windows)]
        {
            let expected = validate_process_target(target)?;
            let raw = process_isolation(target.pid).map_err(|err| match err {
                WindowsApiError::PermissionDenied => ProviderFailure::PermissionDenied,
                WindowsApiError::IdentityChanged => ProviderFailure::IdentityChanged,
                WindowsApiError::Unsupported => ProviderFailure::Unsupported,
                _ => ProviderFailure::ProviderFault,
            })?;
            validate_process_target_after(target, expected)?;

            let sandboxed = Some(
                raw.is_app_container
                    || matches!(
                        raw.integrity_level,
                        Some(WindowsIntegrityLevel::Untrusted | WindowsIntegrityLevel::Low,)
                    ),
            );
            let kind = if raw.is_app_container {
                Some(IsolationKind::OtherContainer)
            } else {
                None
            };

            let value = ProcessIsolation {
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
    ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure> {
        WinProcessIsolationProvider.observe(target, observed_at_ms)
    }
}

pub struct WinProcessAffinityProvider;

impl ProcessAffinityProvider for WinProcessAffinityProvider {
    fn affinity(&mut self, target: &FrozenProcessIdentity) -> Result<Vec<u32>, ProviderFailure> {
        let expected = validate_process_target(target)?;
        let affinity = process_affinity(target.pid).map_err(|err| match err {
            WindowsApiError::PermissionDenied => ProviderFailure::PermissionDenied,
            WindowsApiError::IdentityChanged => ProviderFailure::IdentityChanged,
            WindowsApiError::Unsupported => ProviderFailure::Unsupported,
            _ => ProviderFailure::ProviderFault,
        })?;
        validate_process_target_after(target, expected)?;
        Ok(affinity)
    }
}

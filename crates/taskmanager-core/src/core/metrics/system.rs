//! System telemetry assembly: the six independently scheduled domains
//! (host, cpu, memory, storage, network, gpu), per-provider runtime state,
//! and the legacy `SystemSnapshot` compatibility read model.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

mod cpu;
mod domain;
mod gpu;
mod host;
mod memory;
mod network;
mod storage;

pub use cpu::CpuTelemetryObservation;
pub use domain::SystemObservationState;
pub use gpu::GpuTelemetryObservation;
pub use host::{HostRuntimeFacts, HostRuntimeObservation};
pub use memory::MemoryTelemetryObservation;
pub use network::NetworkTelemetryObservation;
pub use storage::StorageTelemetryObservation;

use super::cpu::CpuMetrics;
use super::disk::DiskMetrics;
use super::gpu::GpuMetrics;
use super::memory::MemoryMetrics;
use super::network::NetworkMetrics;
use crate::core::device_state::{DeviceLifecycle, DeviceStatus};
use crate::core::{ProviderId, SourceStatus};

/// Runtime health of one independently selectable telemetry implementation.
///
/// This is separate from device presence: a provider may be missing while a
/// different provider still supplies a healthy baseline for the same device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRuntimeState {
    pub provider: ProviderId,
    pub status: DeviceStatus,
    pub last_success_ms: Option<u64>,
}

/// The six independently scheduled system telemetry domains.
///
/// This bundle is a projection input, not a completion barrier. Every field
/// retains its own observation time and source outcomes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemTelemetryDomains {
    pub host: HostRuntimeObservation,
    pub cpu: CpuTelemetryObservation,
    pub memory: MemoryTelemetryObservation,
    pub storage: StorageTelemetryObservation,
    pub network: NetworkTelemetryObservation,
    pub gpu: GpuTelemetryObservation,
}

/// Compatibility read model assembled from independently sampled domains.
///
/// Native providers must publish the six domain observations instead of
/// producing this aggregate. `timestamp_ms` is the greatest current domain
/// observation time; it does not prove that the fields were sampled atomically.
#[derive(Debug, Clone, Default)]
pub struct SystemSnapshot {
    /// Compatibility projection watermark, not a common sampling instant.
    pub timestamp_ms: u64,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub disks: Vec<DiskMetrics>,
    pub networks: Vec<NetworkMetrics>,
    pub gpu: Vec<GpuMetrics>,
    /// Per-domain source outcomes for the current telemetry assembly. This is
    /// distinct from device lifecycle and from provider registry health:
    /// authoritative empty, partial data, and unavailable collection remain
    /// distinguishable for every top-level domain.
    pub telemetry_sources: Vec<SourceStatus>,
    /// Partial-source status for runtime provider registries. Successful
    /// device data remains present even when one enhancement provider fails.
    pub provider_states: Vec<ProviderRuntimeState>,
    /// Lifecycle sidecar keyed by stable device identity. It retains confirmed
    /// absent devices for the bounded grace period and distinguishes absence
    /// from an unavailable discovery provider.
    pub device_lifecycles: HashMap<String, DeviceLifecycle>,
    /// Native system uptime in seconds.
    pub uptime_secs: u64,
    /// Provider-observed process count and optional execution-thread count.
    ///
    /// Windows can read the process count through the normal safe `sysinfo`
    /// path while a separate native performance query may be unavailable. A
    /// missing thread count is therefore `None`, never a fabricated zero.
    pub processes: usize,
    pub threads: Option<usize>,
}

impl SystemSnapshot {
    /// Build the strict legacy read model only when every domain and host
    /// scalar has a current value.
    ///
    /// A usable partial observation is current, but stale and unavailable
    /// values are rejected. Host scalars are checked independently so unknown
    /// uptime or counts cannot become zero in the compatibility snapshot.
    #[must_use]
    pub fn from_current_domains(domains: &SystemTelemetryDomains) -> Option<Self> {
        Self::assemble(domains, false)
    }

    /// Build the frontend compatibility model from the independently current
    /// core domains. Optional facets (host threads and GPU) do not block the
    /// model: their absence remains visible through `threads == None` and the
    /// GPU source status carried in `telemetry_sources`.
    #[must_use]
    pub fn from_available_domains(domains: &SystemTelemetryDomains) -> Option<Self> {
        Self::assemble(domains, true)
    }

    fn assemble(domains: &SystemTelemetryDomains, allow_optional_facets: bool) -> Option<Self> {
        let host = domains.host.current_value()?;
        let cpu = domains.cpu.current_value()?;
        let memory = domains.memory.current_value()?;
        let disks = domains
            .storage
            .current_value()?
            .iter()
            .cloned()
            .map(|mut disk| {
                disk.project_partition_lifecycle();
                disk
            })
            .collect();
        let networks = domains.network.current_value()?;
        let gpu: &[GpuMetrics] = match domains.gpu.current_value() {
            Some(gpu) => gpu,
            None if allow_optional_facets
                && matches!(
                    domains.gpu.state(),
                    SystemObservationState::Unavailable { .. }
                ) =>
            {
                &[]
            }
            None => return None,
        };

        let uptime_secs = *host.uptime_secs.current_value()?;
        let processes = usize::try_from(*host.processes.current_value()?).ok()?;
        let threads = host
            .threads
            .current_value()
            .and_then(|value| usize::try_from(*value).ok());
        if !allow_optional_facets && threads.is_none() {
            return None;
        }
        let timestamp_ms = [
            domains.host.state().observed_at_ms()?,
            domains.cpu.state().observed_at_ms()?,
            domains.memory.state().observed_at_ms()?,
            domains.storage.state().observed_at_ms()?,
            domains.network.state().observed_at_ms()?,
        ]
        .into_iter()
        .chain(domains.gpu.state().observed_at_ms())
        .max()?;

        let mut telemetry_sources = Vec::new();
        telemetry_sources.extend_from_slice(domains.host.sources());
        telemetry_sources.extend_from_slice(domains.cpu.sources());
        telemetry_sources.extend_from_slice(domains.memory.sources());
        telemetry_sources.extend_from_slice(domains.storage.sources());
        telemetry_sources.extend_from_slice(domains.network.sources());
        telemetry_sources.extend_from_slice(domains.gpu.sources());
        telemetry_sources.sort_by(|left, right| left.provider.cmp(&right.provider));

        let mut provider_states = Vec::new();
        provider_states.extend_from_slice(domains.storage.provider_states());
        provider_states.extend_from_slice(domains.network.provider_states());
        provider_states.extend_from_slice(domains.gpu.provider_states());
        provider_states = sorted_provider_states(provider_states);

        let mut device_lifecycles = HashMap::new();
        for domain_lifecycles in [
            domains.storage.device_lifecycles(),
            domains.network.device_lifecycles(),
            domains.gpu.device_lifecycles(),
        ] {
            for (device_id, lifecycle) in domain_lifecycles {
                if device_lifecycles
                    .insert(device_id.as_str().to_owned(), *lifecycle)
                    .is_some()
                {
                    return None;
                }
            }
        }

        Some(Self {
            timestamp_ms,
            cpu: cpu.clone(),
            memory: memory.clone(),
            disks,
            networks: networks.to_vec(),
            gpu: gpu.to_vec(),
            telemetry_sources,
            provider_states,
            device_lifecycles,
            uptime_secs,
            processes,
            threads,
        })
    }
}

fn sorted_provider_states(
    mut provider_states: Vec<ProviderRuntimeState>,
) -> Vec<ProviderRuntimeState> {
    provider_states.sort_by(|left, right| left.provider.cmp(&right.provider));
    provider_states
}

#[cfg(test)]
#[path = "../../../tests/headless/metrics/system.rs"]
mod tests;

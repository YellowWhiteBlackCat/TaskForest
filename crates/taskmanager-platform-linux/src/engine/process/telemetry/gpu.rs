//! Per-process DRM fdinfo counters and delta-based utilization.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};

use super::{
    ProcessGpuDevice, ProcessGpuEngines, ProcessGpuSnapshot, ProcessIdentity, state_for_status,
    status_from_io_error,
};

#[cfg(feature = "nvidia")]
mod nvidia;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGpuCounter {
    pub device_id: String,
    pub client_id: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub engine_time_ns: Option<u64>,
}

pub(super) trait ProcessGpuEnrichmentProvider: Send {
    fn collect(
        &mut self,
        proc_root: &Path,
        identity: ProcessIdentity,
        now_ms: u64,
    ) -> RawGpuSnapshot;
}

#[cfg(not(feature = "nvidia"))]
#[derive(Debug, Default)]
struct UnsupportedProcessGpuEnrichment;

#[cfg(not(feature = "nvidia"))]
impl ProcessGpuEnrichmentProvider for UnsupportedProcessGpuEnrichment {
    fn collect(
        &mut self,
        _proc_root: &Path,
        _identity: ProcessIdentity,
        now_ms: u64,
    ) -> RawGpuSnapshot {
        RawGpuSnapshot {
            state: state_for_status(DeviceStatus::Unsupported, now_ms),
            counters: Vec::new(),
        }
    }
}

pub(super) fn standard_process_gpu_enrichment() -> Box<dyn ProcessGpuEnrichmentProvider> {
    #[cfg(feature = "nvidia")]
    {
        Box::new(nvidia::NvmlProcessMemoryProvider::new())
    }
    #[cfg(not(feature = "nvidia"))]
    {
        Box::new(UnsupportedProcessGpuEnrichment)
    }
}

#[derive(Debug, Default)]
pub struct ProcessGpuRateTracker {
    previous: HashMap<(ProcessIdentity, String), (u64, u64)>,
}

impl ProcessGpuRateTracker {
    /// Drop baselines whose pid is absent from the authoritative live pid set.
    ///
    /// The per-observe retain above only resets a pid's own generation on
    /// reuse; pids the user once inspected but that have since exited are
    /// never revisited, so without this pass their entries would accumulate
    /// without bound. Driven by the provider layer on the process-list tick;
    /// every currently live pid stays, so concurrent multi-target insights do
    /// not evict each other.
    pub fn retain_live_pids(&mut self, live_pids: &HashSet<u32>) {
        self.previous
            .retain(|(known, _), _| live_pids.contains(&known.pid));
    }

    pub fn observe(
        &mut self,
        identity: ProcessIdentity,
        now_ms: u64,
        counters: RawGpuSnapshot,
    ) -> ProcessGpuSnapshot {
        self.previous
            .retain(|(known, _), _| known.pid != identity.pid || *known == identity);
        let mut devices = Vec::with_capacity(counters.counters.len());
        for counter in counters.counters {
            let key = (identity, counter.device_id.clone());
            let utilization_pct = counter.engine_time_ns.and_then(|current| {
                let rate = self.previous.get(&key).and_then(|(previous, previous_ms)| {
                    let elapsed_ms = now_ms.saturating_sub(*previous_ms);
                    if elapsed_ms == 0 || current < *previous {
                        return None;
                    }
                    let elapsed_ns = elapsed_ms as f64 * 1_000_000.0;
                    Some(
                        ((current - *previous) as f64 / elapsed_ns * 100.0).clamp(0.0, 100.0)
                            as f32,
                    )
                });
                self.previous.insert(key, (current, now_ms));
                rate
            });
            devices.push(ProcessGpuDevice {
                device_id: counter.device_id,
                memory_bytes: counter.memory_bytes,
                utilization_pct,
                engine_time_ns: counter.engine_time_ns,
            });
        }
        ProcessGpuSnapshot {
            state: counters.state,
            devices,
            // Per-engine breakdown is populated by the owning collector through
            // a separate `fd/`→`fdinfo` pass; default to an honest empty here.
            engines: ProcessGpuEngines::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct RawGpuSnapshot {
    pub state: DeviceState,
    pub counters: Vec<RawGpuCounter>,
}

pub(super) fn merge_gpu_enrichment(
    mut baseline: RawGpuSnapshot,
    enrichment: RawGpuSnapshot,
) -> RawGpuSnapshot {
    for observed in enrichment.counters {
        if let Some(existing) = baseline
            .counters
            .iter_mut()
            .find(|counter| counter.device_id == observed.device_id)
        {
            existing.memory_bytes = max_options(existing.memory_bytes, observed.memory_bytes);
        } else {
            baseline.counters.push(observed);
        }
    }
    baseline
        .counters
        .sort_by(|left, right| left.device_id.cmp(&right.device_id));
    if enrichment.state.status == DeviceStatus::Healthy {
        baseline.state = baseline.state.merge_observation(
            enrichment.state,
            enrichment.state.last_success_ms.unwrap_or(0),
        );
    }
    baseline
}

pub(super) fn collect_counters_from_proc_dir(proc_dir: &Path, now_ms: u64) -> RawGpuSnapshot {
    let entries = match std::fs::read_dir(proc_dir.join("fdinfo")) {
        Ok(entries) => entries,
        Err(error) => {
            return RawGpuSnapshot {
                state: state_for_status(status_from_io_error(&error), now_ms),
                counters: Vec::new(),
            };
        }
    };
    let mut aggregated: BTreeMap<String, RawGpuCounter> = BTreeMap::new();
    let mut seen_clients = HashSet::new();
    let mut denied = false;
    for entry in entries.flatten() {
        let text = match std::fs::read_to_string(entry.path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                denied = true;
                continue;
            }
            Err(_) => continue,
        };
        let Some(counter) = parse_drm_fdinfo(&text) else {
            continue;
        };
        if let Some(client_id) = counter.client_id
            && !seen_clients.insert((counter.device_id.clone(), client_id))
        {
            continue;
        }
        aggregated
            .entry(counter.device_id.clone())
            .and_modify(|current| {
                current.memory_bytes = sum_options(current.memory_bytes, counter.memory_bytes);
                current.engine_time_ns =
                    sum_options(current.engine_time_ns, counter.engine_time_ns);
            })
            .or_insert(counter);
    }
    RawGpuSnapshot {
        state: state_for_status(
            if denied && aggregated.is_empty() {
                DeviceStatus::PermissionDenied
            } else {
                DeviceStatus::Healthy
            },
            now_ms,
        ),
        counters: aggregated.into_values().collect(),
    }
}

fn sum_options(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn max_options(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

pub fn parse_drm_fdinfo(text: &str) -> Option<RawGpuCounter> {
    let mut device_id = None;
    let mut client_id = None;
    let mut engine_time_ns = None;
    let mut resident_memory = None;
    let mut allocated_memory = None;
    let mut saw_drm = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.starts_with("drm-") {
            saw_drm = true;
        }
        match key {
            "drm-pdev" => device_id = nonempty(value).map(|value| format!("gpu:pci:{value}")),
            "drm-client-id" => client_id = value.parse().ok(),
            _ if key.starts_with("drm-engine-") => {
                engine_time_ns = sum_options(engine_time_ns, parse_scaled(value, "ns"));
            }
            _ if key.starts_with("drm-resident-") => {
                resident_memory = sum_options(resident_memory, parse_bytes(value));
            }
            _ if key.starts_with("drm-memory-") => {
                allocated_memory = sum_options(allocated_memory, parse_bytes(value));
            }
            _ => {}
        }
    }
    if !saw_drm {
        return None;
    }
    Some(RawGpuCounter {
        // Driver names and fd enumeration order are not device identities.
        // Without the provider-issued PCI device key this sample cannot be
        // joined safely to system GPU inventory.
        device_id: device_id?,
        client_id,
        memory_bytes: resident_memory.or(allocated_memory),
        engine_time_ns,
    })
}

fn nonempty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn parse_scaled(value: &str, expected_unit: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number = parts.next()?.parse::<u64>().ok()?;
    (parts.next()? == expected_unit).then_some(number)
}

fn parse_bytes(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number = parts.next()?.parse::<u64>().ok()?;
    let multiplier = match parts.next()? {
        "B" => 1,
        "KiB" | "kB" => 1024,
        "MiB" => 1024 * 1024,
        "GiB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    number.checked_mul(multiplier)
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_gpu_tests.rs"]
mod tests;

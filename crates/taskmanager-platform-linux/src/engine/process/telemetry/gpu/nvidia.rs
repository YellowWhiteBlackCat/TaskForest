//! NVML per-process memory enrichment with PID-reuse protection.
//! This is the process-memory portion of `LINUX-NVIDIA-02`.
//!
//! NVML's running-process APIs expose current memory and PID but no Linux
//! start token. Reads are accepted only when `/proc/<pid>/stat` reports the
//! exact expected start token both before and after all NVML calls. The
//! buffered utilization API cannot provide that guarantee and is intentionally
//! left open (`NVML_PROCESS_UTILIZATION_OPEN_START_TOKEN`).
//!
//! Process enrichment also requires normalized PCI identity so it can merge
//! with `drm-pdev` without inventing a UUID-to-DRM association. UUID-only
//! process attribution remains open until the shared device registry exposes
//! an observed PCI↔UUID identity edge.

use std::collections::BTreeMap;
use std::path::Path;

use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::{Nvml, struct_wrappers::device::ProcessInfo};
use taskmanager_core::{DeviceState, ProcessIdentity};

use super::{ProcessGpuEnrichmentProvider, RawGpuCounter, RawGpuSnapshot};
use crate::engine::hardware::gpu::normalize_pci_slot;
use crate::engine::nvml::{NvmlFailureKind, classify_error};
use crate::engine::process::telemetry::{parse_start_time_ticks, state_for_status};

pub(super) struct NvmlProcessMemoryProvider {
    nvml: Option<Nvml>,
    init_failure: Option<NvmlFailureKind>,
}

impl NvmlProcessMemoryProvider {
    pub(super) fn new() -> Self {
        match Nvml::init() {
            Ok(nvml) => Self {
                nvml: Some(nvml),
                init_failure: None,
            },
            Err(error) => Self {
                nvml: None,
                init_failure: Some(classify_error(&error)),
            },
        }
    }
}

impl ProcessGpuEnrichmentProvider for NvmlProcessMemoryProvider {
    fn collect(
        &mut self,
        proc_root: &Path,
        identity: ProcessIdentity,
        now_ms: u64,
    ) -> RawGpuSnapshot {
        let Some(nvml) = self.nvml.as_ref() else {
            return failed_snapshot(
                self.init_failure.unwrap_or(NvmlFailureKind::MissingLibrary),
                now_ms,
            );
        };
        if !process_identity_matches(proc_root, identity) {
            return failed_snapshot(NvmlFailureKind::Transient, now_ms);
        }

        let count = match nvml.device_count() {
            Ok(0) => return failed_snapshot(NvmlFailureKind::Unsupported, now_ms),
            Ok(count) => count,
            Err(error) => return failed_snapshot(classify_error(&error), now_ms),
        };
        let mut counters = BTreeMap::<String, u64>::new();
        let mut failures = Vec::new();
        let mut successful_query = false;
        for index in 0..count {
            let device = match nvml.device_by_index(index) {
                Ok(device) => device,
                Err(error) => {
                    failures.push(classify_error(&error));
                    continue;
                }
            };
            let device_id = match device
                .pci_info()
                .map_err(|error| classify_error(&error))
                .and_then(|pci| {
                    normalize_pci_slot(&pci.bus_id)
                        .map(|slot| format!("gpu:pci:{slot}"))
                        .ok_or(NvmlFailureKind::Transient)
                }) {
                Ok(device_id) => device_id,
                Err(failure) => {
                    failures.push(failure);
                    continue;
                }
            };
            for processes in [
                device.running_compute_processes(),
                device.running_graphics_processes(),
            ] {
                match processes {
                    Ok(processes) => {
                        successful_query = true;
                        merge_process_memory(&mut counters, &device_id, identity.pid, &processes);
                    }
                    Err(error) => failures.push(classify_error(&error)),
                }
            }
        }

        if !process_identity_matches(proc_root, identity) {
            return failed_snapshot(NvmlFailureKind::Transient, now_ms);
        }
        if !successful_query {
            return failed_snapshot(preferred_failure(&failures), now_ms);
        }

        RawGpuSnapshot {
            state: DeviceState::healthy(now_ms),
            counters: counters
                .into_iter()
                .map(|(device_id, memory_bytes)| RawGpuCounter {
                    device_id,
                    client_id: None,
                    memory_bytes: Some(memory_bytes),
                    engine_time_ns: None,
                })
                .collect(),
        }
    }
}

fn merge_process_memory(
    counters: &mut BTreeMap<String, u64>,
    device_id: &str,
    expected_pid: u32,
    processes: &[ProcessInfo],
) {
    for process in processes
        .iter()
        .filter(|process| process.pid == expected_pid)
    {
        if let UsedGpuMemory::Used(memory_bytes) = &process.used_gpu_memory {
            counters
                .entry(device_id.to_string())
                .and_modify(|current| *current = (*current).max(*memory_bytes))
                .or_insert(*memory_bytes);
        }
    }
}

fn process_identity_matches(proc_root: &Path, identity: ProcessIdentity) -> bool {
    read_start_token(proc_root, identity.pid) == Some(identity.start_token)
}

fn read_start_token(proc_root: &Path, pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    parse_start_time_ticks(&stat)
}

fn failed_snapshot(failure: NvmlFailureKind, now_ms: u64) -> RawGpuSnapshot {
    RawGpuSnapshot {
        state: state_for_status(failure.device_status(), now_ms),
        counters: Vec::new(),
    }
}

fn preferred_failure(failures: &[NvmlFailureKind]) -> NvmlFailureKind {
    [
        NvmlFailureKind::PermissionDenied,
        NvmlFailureKind::MissingLibrary,
        NvmlFailureKind::Transient,
        NvmlFailureKind::NotSupported,
        NvmlFailureKind::Unsupported,
    ]
    .into_iter()
    .find(|candidate| failures.contains(candidate))
    .unwrap_or(NvmlFailureKind::Unsupported)
}

#[cfg(test)]
#[path = "../../../../../tests/headless/linux_engine_process_telemetry_gpu_nvidia_tests.rs"]
mod tests;

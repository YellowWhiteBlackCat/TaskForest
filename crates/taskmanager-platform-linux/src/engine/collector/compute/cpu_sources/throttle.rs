//! The single Linux reader for cumulative CPU thermal-throttle trigger
//! counters.
//!
//! Both carriers of the counter fact call [`collect_package_counters_at`]: the
//! periodic CPU package observation (`diagnostics::observe_cpu_packages_at`)
//! and the `telemetry.cpu.throttle` lane provider. Raw per-logical-CPU reads
//! are handed to
//! [`aggregate_package_throttle_counters`](taskmanager_core::aggregate_package_throttle_counters),
//! the one aggregation rule, so neither path can grow a second口径 of its own.
//! A counter no read could observe stays `None`; it is never a fabricated
//! zero.

use std::path::Path;

use taskmanager_core::{
    CpuThrottleCounterSample, CpuThrottleSnapshot, FailureKind, aggregate_package_throttle_counters,
};

/// Read every exposed `cpuN` directory once and aggregate the cumulative
/// thermal-throttle counters per physical package.
///
/// A directory whose `physical_package_id` is unreadable carries no package
/// identity and is skipped; a missing `core_id` still contributes its package
/// counter. An unreadable `cpu` root is the only hard failure, because then no
/// counter could be looked at at all.
pub(crate) fn collect_package_counters_at(cpu_root: &Path) -> CpuThrottleSnapshot {
    let entries = match std::fs::read_dir(cpu_root) {
        Ok(entries) => entries,
        Err(error) => {
            return CpuThrottleSnapshot::failed(
                io_failure(&error),
                format!("cannot enumerate {}: {error}", cpu_root.display()),
            );
        }
    };
    let mut cpu_dirs = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_prefix("cpu")?
                .parse::<u32>()
                .ok()
                .map(|cpu| (cpu, entry.path()))
        })
        .collect::<Vec<_>>();
    cpu_dirs.sort_by_key(|(cpu, _)| *cpu);
    let samples = cpu_dirs
        .iter()
        .filter_map(|(_, path)| sample_at(path))
        .collect::<Vec<_>>();
    CpuThrottleSnapshot::success(aggregate_package_throttle_counters(&samples))
}

/// One logical CPU's counter read; `None` when the kernel exposed no package
/// identity for it.
fn sample_at(cpu_path: &Path) -> Option<CpuThrottleCounterSample> {
    let topology = cpu_path.join("topology");
    let package_id = read_u32(&topology.join("physical_package_id"))?;
    let core_id = read_u32(&topology.join("core_id"));
    let throttle = cpu_path.join("thermal_throttle");
    Some(CpuThrottleCounterSample {
        package_id,
        core_id,
        package_throttle_count: read_u64(&throttle.join("package_throttle_count")),
        core_throttle_count: core_id.and_then(|_| read_u64(&throttle.join("core_throttle_count"))),
    })
}

pub(super) fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub(super) fn read_u32(path: &Path) -> Option<u32> {
    read_trimmed(path)?.parse().ok()
}
pub(super) fn read_u64(path: &Path) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}

pub(super) fn io_failure(error: &std::io::Error) -> FailureKind {
    match error.kind() {
        std::io::ErrorKind::NotFound => FailureKind::Unsupported,
        std::io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        std::io::ErrorKind::TimedOut => FailureKind::TimedOut,
        _ => FailureKind::TemporarilyUnavailable,
    }
}

#[cfg(test)]
#[path = "../../../../../tests/headless/engine/collector/compute/cpu_sources/throttle.rs"]
mod tests;

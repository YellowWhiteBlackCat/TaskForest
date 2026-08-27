//! Intel i915/xe GT frequency, RC6 and per-engine-busy sysfs readers.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use taskmanager_core::FailureKind;

use super::{GpuFieldRead, gpu_io_failure, preferred_gpu_failure};

mod pmu;
pub(crate) use pmu::{discover_intel_pmu_layout_with_receipt, discover_xe_pmu_layout};

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(dead_code))]
pub(super) fn read_intel_gt_rc6_residency_ms(device_path: &Path) -> Option<u64> {
    read_intel_gt_rc6_residency(device_path).value
}

#[cfg(any(test, feature = "test-support"))]
pub(super) fn read_intel_gt_freq_mhz(device_path: &Path) -> Option<u64> {
    read_intel_gt_frequency(device_path).value
}

#[cfg(any(test, feature = "test-support"))]
pub(super) fn read_intel_gt_max_freq_mhz(device_path: &Path) -> Option<u64> {
    read_intel_gt_max_frequency(device_path).value
}

pub(super) fn read_intel_gt_rc6_residency(device_path: &Path) -> GpuFieldRead<u64> {
    let gt_directories = discover_gt_directories(device_path);
    let mut failure = gt_directories.failure;
    let directories = gt_directories.value.unwrap_or_default();
    if directories.is_empty() {
        return GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported));
    }

    let mut total = 0_u64;
    let mut found = false;
    for gt in directories {
        let read = read_u64(
            &gt.join("gtidle").join("idle_residency_ms"),
            FailureKind::Unsupported,
        );
        failure = preferred_gpu_failure(failure, read.failure);
        if let Some(value) = read.value {
            total = total.saturating_add(value);
            found = true;
        }
    }
    finish_read(found.then_some(total), failure)
}

pub(super) fn read_intel_gt_frequency(device_path: &Path) -> GpuFieldRead<u64> {
    read_highest_gt_frequency(device_path, "act_freq", Some("cur_freq"))
}

pub(super) fn read_intel_gt_max_frequency(device_path: &Path) -> GpuFieldRead<u64> {
    read_highest_gt_frequency(device_path, "max_freq", Some("rp0_freq"))
}

/// One Intel GT engine's raw `busy` sample before rate conversion.
///
/// `busy` carries the engine's cumulative counter WITH its unit, so the rate
/// tracker picks the correct branch: nanoseconds for the sysfs `busy` node and
/// the i915 PMU (rate = `delta_busy_ns / elapsed_ns`), or xe two-counter ticks
/// (rate = `active_delta / total_delta`). `name` is already the stable display
/// label (e.g. "Render/3D"), collapsed across instances and tiles.
///
/// [`IntelEngineTracker`]: super::provider::intel::engines::IntelEngineTracker
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IntelEngineRead {
    pub(super) name: String,
    pub(super) busy: EngineBusySource,
}

/// Cumulative busy counter units for one Intel engine.
///
/// The two arms drive the two rate-math branches in the tracker:
/// * `NanoSeconds` — the sysfs `busy` node and the i915 PMU both return
///   cumulative busy nanoseconds; the rate is `delta_busy_ns / elapsed_ns`.
/// * `Ticks` — the xe PMU returns cumulative ACTIVE and TOTAL ticks; the rate
///   is `active_delta / total_delta` (wall-elapsed is irrelevant for xe — it
///   returns TICKS, not nanoseconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EngineBusySource {
    NanoSeconds(u64),
    Ticks { active: u64, total: u64 },
}

/// Read raw per-engine busy samples from the Intel GT `engines` sysfs tree.
///
/// Verified layout for the `xe` driver (`drivers/gpu/drm/xe/xe_hw_engine_class_sysfs.c`):
/// engine classes are grouped under `<gt>/engines/<class>` and each may carry a
/// `busy` node. The legacy `i915` driver registers per-engine sysfs too but, on
/// mainline ~6.x, exposes only scheduling properties there and no `busy` node;
/// in that case this read returns `None`/`Unsupported` and the caller leaves
/// `engines` empty (no fabrication — the typed-None convention).
///
/// Multiple instances of one class and multiple tiles collapse to a single
/// display label; the busiest instance wins so a multi-tile aggregate stays
/// inside `[0, 100]` after rate conversion.
pub(super) fn read_intel_gt_engines(device_path: &Path) -> GpuFieldRead<Vec<IntelEngineRead>> {
    let gt_directories = discover_gt_directories(device_path);
    let mut failure = gt_directories.failure;
    let directories = gt_directories.value.unwrap_or_default();

    // label → busiest raw sample seen across every GT/instance of that class.
    let mut by_label: BTreeMap<String, u64> = BTreeMap::new();
    for gt in directories {
        let entries = match fs::read_dir(gt.join("engines")) {
            Ok(entries) => entries,
            Err(error) => {
                // An absent `engines/` tree is the normal mainline i915 case;
                // keep it soft (Unsupported) so a sibling GT that does expose
                // engines is not erased by one that does not.
                failure = preferred_gpu_failure(
                    failure,
                    Some(gpu_io_failure(&error, FailureKind::Unsupported)),
                );
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                failure = preferred_gpu_failure(failure, Some(FailureKind::TemporarilyUnavailable));
                continue;
            };
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            // xe tucks a `.defaults` metadata kobject under engines/.
            if name.starts_with('.') {
                continue;
            }
            let read = read_u64(&entry.path().join("busy"), FailureKind::Unsupported);
            failure = preferred_gpu_failure(failure, read.failure);
            if let Some(busy) = read.value {
                let label = intel_engine_label(name);
                by_label
                    .entry(label)
                    .and_modify(|current| *current = (*current).max(busy))
                    .or_insert(busy);
            }
        }
    }

    let engines = by_label
        .into_iter()
        .map(|(name, busy)| IntelEngineRead {
            name,
            busy: EngineBusySource::NanoSeconds(busy),
        })
        .collect::<Vec<_>>();
    finish_read((!engines.is_empty()).then_some(engines), failure)
}

/// Map an Intel engine sysfs directory name to the provider-neutral display
/// vocabulary shared with the AMD path. Tolerates both the `xe` per-class names
/// (`render`, `copy`, `compute`, `video`, `video-enhance`) and the legacy `i915`
/// per-instance names (`rcs0`, `bcs0`, `vcs0`, `vecs0`, `ccs0`). Unknown future
/// engines pass through upper-cased rather than being dropped behind a list.
///
/// Order matters: the encode buckets (`vecs`, `video-enhance`) are matched
/// before the decode buckets (`vcs`, `video`) so an encode engine is never
/// swallowed by the decode label.
fn intel_engine_label(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("vecs") || lower.starts_with("video-enhance") {
        "Video Encode".to_string()
    } else if lower.starts_with("vcs") || lower == "video" || lower.starts_with("video-decode") {
        "Video Decode".to_string()
    } else if lower.starts_with("rcs") || lower == "render" {
        "Render/3D".to_string()
    } else if lower.starts_with("bcs") || lower == "copy" || lower == "blitter" {
        "Copy".to_string()
    } else if lower.starts_with("ccs") || lower == "compute" {
        "Compute".to_string()
    } else {
        name.replace(['-', '_'], " ").to_ascii_uppercase()
    }
}

fn read_highest_gt_frequency(
    device_path: &Path,
    primary_node: &str,
    fallback_node: Option<&str>,
) -> GpuFieldRead<u64> {
    let gt_directories = discover_gt_directories(device_path);
    let mut failure = gt_directories.failure;
    let directories = gt_directories.value.unwrap_or_default();
    if directories.is_empty() {
        return GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported));
    }

    let mut best = None;
    for gt in directories {
        let frequency_dir = gt.join("freq0");
        let read = read_alternative_u64(&frequency_dir, primary_node, fallback_node);
        failure = preferred_gpu_failure(failure, read.failure);
        if let Some(value) = read.value.filter(|value| *value > 0) {
            best = Some(best.map_or(value, |current: u64| current.max(value)));
        } else if read.value == Some(0) {
            failure = preferred_gpu_failure(failure, Some(FailureKind::ProviderFault));
        }
    }
    finish_read(best, failure)
}

fn discover_gt_directories(device_path: &Path) -> GpuFieldRead<Vec<PathBuf>> {
    let tiles = match fs::read_dir(device_path) {
        Ok(entries) => entries,
        Err(error) => {
            return GpuFieldRead::unavailable(gpu_io_failure(&error, FailureKind::IdentityChanged));
        }
    };
    let mut directories = Vec::new();
    let mut failure = None;
    for tile in tiles {
        let tile = match tile {
            Ok(tile) => tile,
            Err(error) => {
                failure = preferred_gpu_failure(
                    failure,
                    Some(gpu_io_failure(&error, FailureKind::TemporarilyUnavailable)),
                );
                continue;
            }
        };
        if !tile.file_name().to_string_lossy().starts_with("tile") {
            continue;
        }
        let gts = match fs::read_dir(tile.path()) {
            Ok(entries) => entries,
            Err(error) => {
                failure = preferred_gpu_failure(
                    failure,
                    Some(gpu_io_failure(&error, FailureKind::TemporarilyUnavailable)),
                );
                continue;
            }
        };
        for gt in gts {
            match gt {
                Ok(gt) if gt.file_name().to_string_lossy().starts_with("gt") => {
                    directories.push(gt.path());
                }
                Ok(_) => {}
                Err(error) => {
                    failure = preferred_gpu_failure(
                        failure,
                        Some(gpu_io_failure(&error, FailureKind::TemporarilyUnavailable)),
                    );
                }
            }
        }
    }
    directories.sort();
    finish_read(Some(directories), failure)
}

fn read_alternative_u64(
    directory: &Path,
    primary_node: &str,
    fallback_node: Option<&str>,
) -> GpuFieldRead<u64> {
    let primary = read_u64(&directory.join(primary_node), FailureKind::Unsupported);
    if let Some(value) = primary.value.filter(|value| *value > 0) {
        return GpuFieldRead::available(value);
    }
    let primary_failure = if primary.value == Some(0) {
        Some(FailureKind::ProviderFault)
    } else {
        primary.failure
    };
    let Some(fallback_node) = fallback_node else {
        return GpuFieldRead::unavailable(primary_failure.unwrap_or(FailureKind::Unsupported));
    };
    let fallback = read_u64(&directory.join(fallback_node), FailureKind::Unsupported);
    if let Some(value) = fallback.value {
        return GpuFieldRead::available(value);
    }
    GpuFieldRead::unavailable(
        preferred_gpu_failure(primary_failure, fallback.failure)
            .unwrap_or(FailureKind::Unsupported),
    )
}

fn read_u64(path: &Path, missing: FailureKind) -> GpuFieldRead<u64> {
    match fs::read_to_string(path) {
        Ok(value) => value.trim().parse().map_or_else(
            |_| GpuFieldRead::unavailable(FailureKind::ProviderFault),
            GpuFieldRead::available,
        ),
        Err(error) => GpuFieldRead::unavailable(gpu_io_failure(&error, missing)),
    }
}

fn finish_read<T>(value: Option<T>, failure: Option<FailureKind>) -> GpuFieldRead<T> {
    match (value, failure) {
        (Some(value), Some(failure)) => GpuFieldRead::partial(value, failure),
        (Some(value), None) => GpuFieldRead::available(value),
        (None, Some(failure)) => GpuFieldRead::unavailable(failure),
        (None, None) => GpuFieldRead::unavailable(FailureKind::Unsupported),
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_hardware_gpu_intel_tests.rs"]
mod tests;

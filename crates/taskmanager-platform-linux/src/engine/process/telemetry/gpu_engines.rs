//! Per-process GPU engine utilization via `/proc/<pid>/fd` + `fdinfo`.
//!
//! Each open DRM file descriptor (a `/proc/<pid>/fd/<n>` symlink that resolves
//! to `/dev/dri/renderD###` or `/dev/dri/card#`) owns a
//! `/proc/<pid>/fdinfo/<n>` text file. The kernel DRM usage-stats spec
//! ([DRM client usage stats](https://docs.kernel.org/gpu/drm-usage-stats.html))
//! exposes one `drm-engine-<name>: <ns>` line per engine class, where the value
//! is the
//! cumulative nanoseconds that DRM client (fd) spent executing work on that
//! engine.
//!
//! A process may hold several DRM descriptors, each an independent DRM client
//! with its own cumulative counter. This module enumerates `fd/`, keeps only
//! the descriptors that resolve to `/dev/dri/`, and **sums** each engine's
//! nanoseconds across the process's DRM descriptors before rate-converting.
//!
//! Honesty contract: a cumulative counter is only useful as a delta over a
//! measured interval, so the per-engine rate is a typed [`ScalarObservation`].
//! The first sighting seeds the baseline (a typed gap, never a fabricated
//! zero), a counter rollback resets the baseline, and a permission-denied or
//! vanished procfs tree produces a typed [`DeviceState`] with an empty engine
//! list — never an invented engine or zero.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::{
    FailureKind, ProcessGpuEngineUsage, ProcessGpuEngines, ProcessIdentity,
};

use super::{state_for_status, status_from_io_error};

/// Aggregated cumulative engine counters for one process, before rate
/// conversion. `engines` is `(name, counters)` summed across every DRM
/// descriptor the process holds, ordered by ascending name for stable diffing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawGpuEngineSnapshot {
    pub state: DeviceState,
    pub engines: Vec<(String, RawEngineCounters)>,
}

/// True when a `/proc/<pid>/fd/<n>` readlink target is a DRM descriptor.
///
/// The kernel exposes render nodes as `/dev/dri/renderD###` and primary nodes as
/// `/dev/dri/card#`; both account GPU work through fdinfo. `by-path`/`by-name`
/// symlinks under `/dev/dri/` are also matched because they resolve to the same
/// DRM devices.
#[must_use]
pub fn is_drm_render_target(target: &str) -> bool {
    target.contains("/dev/dri/")
}

/// One engine's aggregated counters from an fdinfo blob. `ns` is cumulative
/// busy time (i915 `drm-engine-<name>` / xe `drm-total-busy-<name>`); `cycles`
/// is the cumulative cycle counter (xe `drm-total-cycles-<name>` /
/// `drm-cycles-<name>`). A driver exposes one or the other — never both on a
/// single kernel, but the model is additive for forward compatibility.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawEngineCounters {
    pub ns: Option<u64>,
    pub cycles: Option<u64>,
}

/// Parse every engine counter ABI out of an fdinfo blob, keyed by engine name:
///
///   * `drm-engine-<name>: <ns>` — i915 busy nanoseconds;
///   * `drm-total-busy-<name>: <ns>` — xe busy nanoseconds (6.10+);
///   * `drm-total-cycles-<name>: <n>` — xe cumulative cycles;
///   * `drm-cycles-<name>: <n>` — xe session cycles (older kernels).
///
/// The canonical kernel unit is enforced per ABI (`ns` for time, bare integer
/// for cycles) so a non-time value cannot masquerade as busy time. Non-engine
/// lines (`drm-pdev`, `drm-client-id`, `drm-total-vram0`, ...) are ignored.
#[must_use]
pub fn parse_drm_engine_counters(text: &str) -> BTreeMap<String, RawEngineCounters> {
    let mut engines: BTreeMap<String, RawEngineCounters> = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if let Some(name) = key.strip_prefix("drm-engine-")
            && let Some(ns) = parse_engine_ns(value)
        {
            let entry = engines.entry(name.to_string()).or_default();
            entry.ns = Some(entry.ns.unwrap_or(0).saturating_add(ns));
        } else if let Some(name) = key.strip_prefix("drm-total-busy-")
            && let Some(ns) = parse_engine_ns(value)
        {
            let entry = engines.entry(name.to_string()).or_default();
            entry.ns = Some(entry.ns.unwrap_or(0).saturating_add(ns));
        } else if let Some(name) = key.strip_prefix("drm-total-cycles-")
            && let Some(cycles) = parse_plain_u64(value)
        {
            let entry = engines.entry(name.to_string()).or_default();
            entry.cycles = Some(entry.cycles.unwrap_or(0).saturating_add(cycles));
        } else if let Some(name) = key.strip_prefix("drm-cycles-")
            && let Some(cycles) = parse_plain_u64(value)
        {
            let entry = engines.entry(name.to_string()).or_default();
            entry.cycles = Some(entry.cycles.unwrap_or(0).saturating_add(cycles));
        }
    }
    engines
}

/// Parse `<number> ns` strictly. The unit is mandatory and must be `ns` so a
/// non-time value cannot masquerade as busy nanoseconds.
fn parse_engine_ns(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number = parts.next()?.parse::<u64>().ok()?;
    match parts.next() {
        Some(unit) if unit.eq_ignore_ascii_case("ns") => Some(number),
        Some(_) | None => None,
    }
}

/// Parse a bare integer (cycle counters carry no unit suffix).
fn parse_plain_u64(value: &str) -> Option<u64> {
    value.parse::<u64>().ok()
}

/// Collect per-engine cumulative counters for `proc_dir` (`/proc/<pid>`).
///
/// Enumerates `fd/`, keeps descriptors whose readlink target contains
/// `/dev/dri/`, reads `fdinfo/<fd>` for each, and sums each engine's ns across
/// the process's DRM descriptors. Typed outcomes:
///   * `read_dir(fd/)` `NotFound` → [`DeviceStatus::Stale`] (vanished pid),
///     `PermissionDenied` → [`DeviceStatus::PermissionDenied`];
///   * a live process with no DRM descriptors, or DRM descriptors whose fdinfo
///     carries no `drm-engine-` lines, is a [`DeviceStatus::Healthy`] empty list
///     (an honest empty, not an unknown);
///   * a per-fdinfo `PermissionDenied` is recorded; if no engine could be read
///     at all the snapshot is [`DeviceStatus::PermissionDenied`], mirroring the
///     existing `collect_counters_from_proc_dir` denial rule.
pub fn collect_gpu_engines_from_proc_dir(proc_dir: &Path, now_ms: u64) -> RawGpuEngineSnapshot {
    let fd_dir = proc_dir.join("fd");
    let entries = match std::fs::read_dir(&fd_dir) {
        Ok(entries) => entries,
        Err(error) => {
            return RawGpuEngineSnapshot {
                state: state_for_status(status_from_io_error(&error), now_ms),
                engines: Vec::new(),
            };
        }
    };

    let mut aggregated: BTreeMap<String, RawEngineCounters> = BTreeMap::new();
    let mut denied_fdinfo = false;
    for entry in entries.flatten() {
        let Some(fd) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            // Non-numeric fd entries are not real descriptors; skip.
            continue;
        };
        let target = match std::fs::read_link(entry.path()) {
            Ok(target) => target.to_string_lossy().to_string(),
            Err(_) => {
                // A single unreadable link (privileged fd, or a descriptor that
                // vanished between enumeration and readlink) does not fail the
                // whole facet; it just cannot be classified.
                continue;
            }
        };
        if !is_drm_render_target(&target) {
            continue;
        }
        let fdinfo_path = proc_dir.join("fdinfo").join(fd.to_string());
        let text = match std::fs::read_to_string(&fdinfo_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                denied_fdinfo = true;
                continue;
            }
            Err(_) => continue,
        };
        for (name, counters) in parse_drm_engine_counters(&text) {
            let entry = aggregated.entry(name).or_default();
            if let Some(ns) = counters.ns {
                entry.ns = Some(entry.ns.unwrap_or(0).saturating_add(ns));
            }
            if let Some(cycles) = counters.cycles {
                entry.cycles = Some(entry.cycles.unwrap_or(0).saturating_add(cycles));
            }
        }
    }

    let status = if denied_fdinfo && aggregated.is_empty() {
        DeviceStatus::PermissionDenied
    } else {
        // A live process with no DRM descriptors, or DRM descriptors whose
        // fdinfo exposes no engine lines, is a healthy empty list — an honest
        // empty rather than an unknown.
        DeviceStatus::Healthy
    };

    RawGpuEngineSnapshot {
        state: state_for_status(status, now_ms),
        engines: aggregated.into_iter().collect(),
    }
}

/// Per-engine delta→rate state, generation-scoped by [`ProcessIdentity`].
///
/// Mirrors the established baseline contract of the container CPU tracker and
/// the existing `ProcessGpuRateTracker`: the first sighting of an engine seeds
/// its baseline (no rate yet), every later tick converts the monotonic delta
/// into a 0–100% single-core-equivalent rate, a counter rollback resets the
/// baseline, and a PID reuse clears the prior identity's baselines.
#[derive(Debug, Default)]
pub struct ProcessGpuEngineRateTracker {
    previous: HashMap<(ProcessIdentity, String), (u64, u64)>,
}

impl ProcessGpuEngineRateTracker {
    /// Drop baselines whose pid is absent from the authoritative live pid set.
    ///
    /// The per-observe retain inside [`Self::observe`] only resets a pid's own
    /// generation on reuse; pids the user once inspected but that have since
    /// exited are never revisited, so without this pass their entries would
    /// accumulate without bound. Driven by the provider layer on the
    /// process-list tick; every currently live pid stays, so concurrent
    /// multi-target insights do not evict each other.
    pub fn retain_live_pids(&mut self, live_pids: &HashSet<u32>) {
        self.previous
            .retain(|(known, _), _| live_pids.contains(&known.pid));
    }

    /// Rate-convert a raw aggregated snapshot into a typed engine breakdown.
    ///
    /// `now_ms` is the collection wall clock already pinned by the owning
    /// collector; no new clock source is introduced. Engines that vanish
    /// between samples have their baselines dropped automatically because they
    /// simply do not reappear in `raw.engines`.
    #[must_use]
    pub fn observe(
        &mut self,
        identity: ProcessIdentity,
        now_ms: u64,
        raw: RawGpuEngineSnapshot,
    ) -> ProcessGpuEngines {
        // Drop baselines whose pid was reused for a different start token so the
        // new generation cannot inherit the prior process's rates.
        self.previous
            .retain(|(known, _), _| known.pid != identity.pid || *known == identity);

        let mut engines = Vec::with_capacity(raw.engines.len());
        for (name, counters) in raw.engines {
            // The cumulative ns counter is observable on every healthy read, so
            // it stays Available from the first sighting — an honest cold-start
            // reading even before a rate can be computed. A cycles-only source
            // (xe) keeps it Unknown and the raw cycles become the observable.
            let engine_time_ns = counters.ns.map_or_else(ScalarObservation::default, |ns| {
                ScalarObservation::available(ns, now_ms)
            });
            let engine_cycles = counters
                .cycles
                .map_or_else(ScalarObservation::default, |cycles| {
                    ScalarObservation::available(cycles, now_ms)
                });
            let usage_pct = match counters.ns {
                Some(current_ns) => self.engine_rate(identity, &name, current_ns, now_ms),
                // Cycles cannot be converted to a percentage without the GT
                // clock; the typed gap stays honest instead of guessing.
                None => ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            };
            engines.push(ProcessGpuEngineUsage {
                name,
                usage_pct,
                engine_time_ns,
                engine_cycles,
            });
        }
        ProcessGpuEngines {
            state: raw.state,
            engines,
        }
    }

    /// Convert one cumulative ns sample into a typed 0–100% rate.
    ///
    /// First sample → [`FailureKind::TemporarilyUnavailable`] (typed gap);
    /// zero-elapsed, clock-rollback, or counter-rollback → baseline reset +
    /// [`FailureKind::IdentityChanged`]; otherwise the clamped single-core-
    /// equivalent percentage. Mirrors `ContainerCpuRateTracker::percentage`.
    fn engine_rate(
        &mut self,
        identity: ProcessIdentity,
        name: &str,
        current_ns: u64,
        now_ms: u64,
    ) -> ScalarObservation<f32> {
        let key = (identity, name.to_owned());
        let baseline = self.previous.entry(key).or_insert((current_ns, now_ms));
        if baseline.1 == now_ms {
            return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
        }
        let elapsed_ms = match now_ms.checked_sub(baseline.1) {
            Some(elapsed) => elapsed,
            None => {
                *baseline = (current_ns, now_ms);
                return ScalarObservation::unavailable(FailureKind::IdentityChanged);
            }
        };
        let delta_ns = match current_ns.checked_sub(baseline.0) {
            Some(delta) => delta,
            None => {
                *baseline = (current_ns, now_ms);
                return ScalarObservation::unavailable(FailureKind::IdentityChanged);
            }
        };
        *baseline = (current_ns, now_ms);
        let elapsed_ns = elapsed_ms as f64 * 1_000_000.0;
        let percentage = (delta_ns as f64 / elapsed_ns * 100.0).clamp(0.0, 100.0) as f32;
        if percentage.is_finite() && percentage >= 0.0 {
            ScalarObservation::available(percentage, now_ms)
        } else {
            ScalarObservation::unavailable(FailureKind::ProviderFault)
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_gpu_engines_tests.rs"]
mod tests;

//! One-shot per-process GPU engine bulk wiring for the CLI `--json` snapshot.
//!
//! The `--json` snapshot is a single point in time, so the per-engine **rate**
//! — a delta of the cumulative DRM busy counter over a measured interval — is
//! honestly `TemporarilyUnavailable` for every engine on the only tick the CLI
//! ever takes. Only the cumulative `engine_time_ns` is observed. This matches
//! the streaming provider's cold-start contract exactly: never a fabricated 0%
//! to fill the gap.
//!
//! Layering: the `/proc/<pid>/fdinfo` walk is plain safe `std::fs` over a path
//! (no platform-adapter crate, no `unsafe`), but the dependency firewall keeps
//! `std::fs` and `/proc` out of the shared core/application layers. It therefore
//! lives here in the binary composition edge — the same layer `src/main.rs`
//! occupies — which is allowed host filesystem access. On a host without a
//! `/proc` fdinfo source, no GPU, or only foreign-uid processes whose `fdinfo`
//! is `EACCES`, the result is an honest empty array — never a fabricated row and
//! never a fabricated zero. Threshold suggestions are intentionally NOT emitted
//! here; they live on the dedicated `--suggest-thresholds` path.

#![forbid(unsafe_code)]

use taskmanager_core::core::ContainerSummary;
use taskmanager_core::core::export::{
    ExportExtras, ProcessGpuEnginesEntry, snapshot_to_json_with_extras,
};
use taskmanager_core::core::process_telemetry::ProcessGpuEngines;
use taskmanager_core::{ProcessItem, SystemSnapshot};
// The four contracts below are referenced only by the Linux /proc bulk reader
// (and its unit tests). On macOS/Windows that reader is cfg'd out, so these
// imports must follow it — otherwise `-D unused-imports` fires on those targets.
#[cfg(any(target_os = "linux", test))]
use taskmanager_core::core::FailureKind;
#[cfg(any(target_os = "linux", test))]
use taskmanager_core::core::metrics::ScalarObservation;
#[cfg(any(target_os = "linux", test))]
use taskmanager_core::core::process_telemetry::ProcessGpuEngineUsage;
// DeviceState is referenced only by the Linux /proc bulk reader and its
// Linux-only tests, never by the cross-platform tests (one_gpu_proc uses
// FailureKind, not DeviceState). Keep it Linux-gated rather than
// `any(linux, test)` so it is not an unused import on macOS/Windows test
// builds under -D warnings.
#[cfg(target_os = "linux")]
use taskmanager_core::core::DeviceState;

/// Hard cap on how many processes the one-shot bulk GPU scan opens. Keeps the
/// stateless CLI fast on hosts with thousands of processes; a GPU process past
/// the cap is an honest absence, not a fabricated zero. GPU work tends to be
/// concentrated in a handful of long-lived clients, so a thousand-process window
/// covers realistic hosts with ample headroom.
#[cfg(target_os = "linux")]
const MAX_BULK_GPU_PROCESSES: usize = 1024;

/// Per-pid owned GPU engine breakdown. Owned (no borrow) so the CLI can build
/// short-lived [`ProcessGpuEnginesEntry`] views for the export envelope without
/// a self-referential struct.
pub type OwnedProcessGpuEngines = (u32, ProcessGpuEngines);

// ── Linux /proc fdinfo bulk reader ─────────────────────────────────────────
//
// The helpers below mirror the linux provider's authoritative fdinfo reader
// (crates/taskmanager-platform-linux/.../gpu_engines.rs). They are duplicated
// here deliberately: the provider's reader is wired for the streaming per-pid
// path and carries rate-tracking state the one-shot CLI does not need, while the
// shared core is firewall-forbidden from touching `std::fs` or `/proc`. This
// binary-edge copy is the minimal cold-start one-shot reader; the parse rule is
// kept bit-identical to the provider's so the two paths agree on the kernel DRM
// usage-stats spec.

#[cfg(target_os = "linux")]
fn is_drm_render_target(target: &str) -> bool {
    // Render nodes (/dev/dri/renderD###), primary nodes (/dev/dri/card#), and
    // the by-path/by-name symlinks under /dev/dri/ all account GPU work
    // through fdinfo, so any target containing /dev/dri/ qualifies. Linux
    // 6.3+ exposes NPU/AI accelerators through the same DRM fdinfo ABI under
    // /dev/accel/accel#, so those descriptors qualify too — a process holding
    // an ivpu client reports its engine time through the same parser.
    target.contains("/dev/dri/") || target.contains("/dev/accel/")
}

#[cfg(target_os = "linux")]
fn parse_engine_ns(value: &str) -> Option<u64> {
    // Strict `<number> ns`: the unit is mandatory and must be `ns` so a non-time
    // value cannot masquerade as busy nanoseconds.
    let mut parts = value.split_whitespace();
    let number = parts.next()?.parse::<u64>().ok()?;
    match parts.next() {
        Some(unit) if unit.eq_ignore_ascii_case("ns") => Some(number),
        Some(_) | None => None,
    }
}

/// One engine's counters parsed out of an fdinfo blob, mirroring the shared
/// adapter's `RawEngineCounters` (this composition edge keeps its own walk per
/// the layering note above, but the ABI knowledge must not diverge).
#[cfg(target_os = "linux")]
#[derive(Default)]
struct CliEngineCounters {
    ns: Option<u64>,
    cycles: Option<u64>,
}

/// Parse every engine counter ABI: i915 `drm-engine-<name>: <ns>`, xe
/// `drm-total-busy-<name>: <ns>` (busy time) and xe `drm-total-cycles-<name>` /
/// `drm-cycles-<name>` (cumulative cycles). Cycles carry no unit suffix; time
/// values require the `ns` suffix so a non-time value cannot pose as busy time.
#[cfg(target_os = "linux")]
fn parse_drm_engine_counters(text: &str) -> std::collections::BTreeMap<String, CliEngineCounters> {
    let mut engines: std::collections::BTreeMap<String, CliEngineCounters> =
        std::collections::BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let entry = if let Some(name) = key.strip_prefix("drm-engine-") {
            parse_engine_ns(value).map(|ns| {
                (
                    name,
                    CliEngineCounters {
                        ns: Some(ns),
                        ..Default::default()
                    },
                )
            })
        } else if let Some(name) = key.strip_prefix("drm-total-busy-") {
            parse_engine_ns(value).map(|ns| {
                (
                    name,
                    CliEngineCounters {
                        ns: Some(ns),
                        ..Default::default()
                    },
                )
            })
        } else if let Some(name) = key.strip_prefix("drm-total-cycles-") {
            value.trim().parse::<u64>().ok().map(|cycles| {
                (
                    name,
                    CliEngineCounters {
                        cycles: Some(cycles),
                        ..Default::default()
                    },
                )
            })
        } else if let Some(name) = key.strip_prefix("drm-cycles-") {
            value.trim().parse::<u64>().ok().map(|cycles| {
                (
                    name,
                    CliEngineCounters {
                        cycles: Some(cycles),
                        ..Default::default()
                    },
                )
            })
        } else {
            None
        };
        let Some((name, counters)) = entry else {
            continue;
        };
        let slot = engines.entry(name.to_string()).or_default();
        if let Some(ns) = counters.ns {
            slot.ns = Some(slot.ns.unwrap_or(0).saturating_add(ns));
        }
        if let Some(cycles) = counters.cycles {
            slot.cycles = Some(slot.cycles.unwrap_or(0).saturating_add(cycles));
        }
    }
    engines
}

/// Aggregate each engine's counters (busy ns and/or cycles) across every DRM
/// descriptor `proc_dir` (`/proc/<pid>`) holds.
///
/// Returns an empty map when the fd directory cannot be read (vanished pid,
/// `EACCES` on `/proc/<pid>/fd`, any other I/O failure) OR when the process
/// holds no DRM descriptor whose fdinfo carries engine lines. Both cases
/// are an honest "no reading" — the caller skips them rather than fabricating a
/// zero-valued breakdown.
#[cfg(target_os = "linux")]
fn read_proc_drm_engine_counters(
    proc_dir: &std::path::Path,
) -> std::collections::BTreeMap<String, CliEngineCounters> {
    use std::collections::BTreeMap;
    let mut aggregated: BTreeMap<String, CliEngineCounters> = BTreeMap::new();
    let fd_dir = proc_dir.join("fd");
    let Ok(entries) = std::fs::read_dir(&fd_dir) else {
        // Vanished pid, EACCES on /proc/<pid>/fd, or any other read failure: no
        // honest engine reading is possible.
        return aggregated;
    };
    for entry in entries.flatten() {
        let Some(fd) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            // Non-numeric fd entries are not real descriptors; skip.
            continue;
        };
        let Ok(target) = std::fs::read_link(entry.path()) else {
            // A single unreadable link (privileged fd, or a descriptor that
            // vanished between enumeration and readlink) does not fail the
            // whole facet; it just cannot be classified.
            continue;
        };
        if !is_drm_render_target(&target.to_string_lossy()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(proc_dir.join("fdinfo").join(fd.to_string())) else {
            // fdinfo absent (race) or denied: skip this descriptor, do not
            // fabricate its engines.
            continue;
        };
        for (name, counters) in parse_drm_engine_counters(&text) {
            let slot = aggregated.entry(name).or_default();
            if let Some(ns) = counters.ns {
                slot.ns = Some(slot.ns.unwrap_or(0).saturating_add(ns));
            }
            if let Some(cycles) = counters.cycles {
                slot.cycles = Some(slot.cycles.unwrap_or(0).saturating_add(cycles));
            }
        }
    }
    aggregated
}

/// One-shot bulk collection of per-process GPU engine breakdowns for the
/// stateless CLI snapshot.
///
/// Walks at most `max_processes` pids from `pids`, reading
/// `/proc/<pid>/fd` + `fdinfo` for each via [`read_proc_drm_engine_ns`]. Only
/// processes that hold at least one DRM descriptor with readable `drm-engine-`
/// lines produce an entry; a non-GPU process, a vanished pid, or a foreign-uid
/// process whose `fdinfo` is `EACCES` is skipped — an honest absence, never a
/// fabricated zero-valued row.
///
/// Because the CLI takes exactly one sample, the per-engine **rate** is the
/// typed cold-start gap (`TemporarilyUnavailable`) — there is no prior
/// cumulative counter to delta against. The cumulative `engine_time_ns` IS
/// observed on this single tick, giving an honest reading even before any rate
/// could be computed.
#[cfg(target_os = "linux")]
fn collect_process_gpu_engines_bulk(
    proc_root: &std::path::Path,
    pids: &[u32],
    now_ms: u64,
    max_processes: usize,
) -> Vec<OwnedProcessGpuEngines> {
    let mut out: Vec<OwnedProcessGpuEngines> = Vec::new();
    for (scanned, &pid) in pids.iter().enumerate() {
        if scanned >= max_processes {
            break;
        }
        let engines_map = read_proc_drm_engine_counters(&proc_root.join(pid.to_string()));
        if engines_map.is_empty() {
            // No DRM descriptor with readable engine lines: a live non-GPU
            // process, a vanished pid, or an EACCES fd tree. Skip — never emit
            // an empty-or-fabricated row.
            continue;
        }
        let engines: Vec<ProcessGpuEngineUsage> = engines_map
            .into_iter()
            .map(|(name, counters)| ProcessGpuEngineUsage {
                name,
                // Single-tick cold start: honest typed gap, never 0%.
                usage_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                engine_time_ns: counters.ns.map_or_else(ScalarObservation::default, |ns| {
                    ScalarObservation::available(ns, now_ms)
                }),
                engine_cycles: counters
                    .cycles
                    .map_or_else(ScalarObservation::default, |cycles| {
                        ScalarObservation::available(cycles, now_ms)
                    }),
            })
            .collect();
        out.push((
            pid,
            ProcessGpuEngines {
                state: DeviceState::healthy(now_ms),
                engines,
            },
        ));
    }
    out
}

/// Collect per-process GPU engine breakdowns for the one-shot JSON snapshot,
/// best-effort and bounded.
///
/// On Linux this scans `/proc/<pid>/fdinfo` for every pid in `processes` (up to
/// `MAX_BULK_GPU_PROCESSES`). On hosts without a `/proc` fdinfo source it
/// returns an honest empty vec — never a fabricated row. A process that holds no
/// DRM descriptor, whose `fdinfo` is `EACCES`, or that has vanished between the
/// process-list and GPU scans is skipped (an honest absence), never rendered as
/// a zero-valued breakdown.
#[must_use]
pub fn collect_bulk_process_gpu_engines(
    processes: &[ProcessItem],
    now_ms: u64,
) -> Vec<OwnedProcessGpuEngines> {
    #[cfg(target_os = "linux")]
    {
        let pids: Vec<u32> = processes.iter().map(|item| item.pid).collect();
        collect_process_gpu_engines_bulk(
            std::path::Path::new("/proc"),
            &pids,
            now_ms,
            MAX_BULK_GPU_PROCESSES,
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        // No /proc fdinfo source on this host: an honest empty, not a fabricated
        // breakdown. `processes` and `now_ms` are unused off-Linux.
        let _ = processes;
        let _ = now_ms;
        Vec::new()
    }
}

/// Shape owned bulk breakdowns into the borrowed envelope entries the export
/// serializer expects. The entries borrow from `owned`, so `owned` must outlive
/// every reader of the returned vec.
#[must_use]
pub fn build_process_gpu_entries(
    owned: &[(u32, ProcessGpuEngines)],
) -> Vec<ProcessGpuEnginesEntry<'_>> {
    owned
        .iter()
        .map(|(pid, engines)| ProcessGpuEnginesEntry { pid: *pid, engines })
        .collect()
}

/// Render the one-shot CLI JSON snapshot: the six-domain snapshot, the process
/// list, the container rollup, and the bulk per-process GPU engine breakdowns.
///
/// Kept as a single seam so `src/cli.rs` stays small and the GPU bulk wiring is
/// unit-testable in isolation. Reuses the shared pure formatter so the JSON
/// layout is byte-identical to the GUI "export snapshot" artifact — no forked
/// data model, no duplicated envelope shape. `now_ms` is the CLI's pinned wall
/// clock passed down so no new clock source is introduced.
pub fn render_json_snapshot(
    snapshot: &SystemSnapshot,
    processes: &[ProcessItem],
    containers: &[ContainerSummary],
    hardware: Option<&taskmanager_core::core::hardware::HardwareInfo>,
    npu_inventory: Option<&taskmanager_core::core::npu::NpuInventorySnapshot>,
    now_ms: u64,
) -> String {
    let gpu_owned = collect_bulk_process_gpu_engines(processes, now_ms);
    let gpu_entries = build_process_gpu_entries(&gpu_owned);
    let extras = ExportExtras {
        containers,
        process_gpu_engines: &gpu_entries,
        suggested_thresholds: &[],
        hardware,
        npu_inventory,
    };
    snapshot_to_json_with_extras(snapshot, processes, extras)
}

#[cfg(test)]
#[path = "../tests/logic/cli_process_gpu_tests.rs"]
mod tests;

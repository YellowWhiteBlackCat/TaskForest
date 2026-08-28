//! Per-engine sampling — opens counters through the audited boundary crate's
//! SAFE API, sleeps the sample window, reads the deltas, and computes the busy
//! ratio. The `perf_event_open` syscall and its ioctls live ONLY in
//! `taskmanager-perf-ioctl` (one of the workspace's four `unsafe` trust roots,
//! ADR-022);
//! this module never writes `unsafe` and touches perf exclusively through
//! [`GpuEngineCounter::open_enabled`] / [`GpuEngineCounter::read_counter`].
//!
//! Rate math mirrors the product's existing tracker
//! (`crates/taskmanager-platform-linux`'s `provider/intel/engines.rs`):
//! * xe — cumulative TICKS, rate = `active_delta / total_delta` (wall-elapsed
//!   is irrelevant; a `total_delta == 0` interval is a typed skip, never a
//!   divide-by-zero);
//! * i915 — cumulative busy NANoseconds, rate = `busy_ns / elapsed_ns`.
//!
//! Honesty: an open failure for ALL engines due to `EACCES`/`EPERM` becomes
//! [`SampleError::PermissionDenied`] (the escalatable `perf_event_paranoid`
//! denial); a read failure for ALL engines becomes [`SampleError::ReadFailed`];
//! an EMPTY engine list (no open attempted) becomes [`SampleError::NoEngines`].
//! Partial failures keep the engines that produced data — never a fabricated
//! number for an engine that did not read.

use std::io;
use std::thread;
use std::time::{Duration, Instant};

use taskmanager_perf_ioctl::GpuEngineCounter;

use crate::discovery::{I915EngineCfg, PmuLayout, XeEngineCfg};
use crate::json::EngineJson;

/// One sampled engine's busy percentage plus its identifying fields (re-joined
/// to the config in the sample loop so a read failure on one engine does not
/// erase the labels of the engines that succeeded).
struct SampledEngine {
    label: String,
    class_name: String,
    busy_pct: f32,
}

/// Why sampling failed, already mapped to the contract's typed error kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleError {
    PermissionDenied(String),
    OpenFailed(String),
    /// The engine list was empty — no counter open was even attempted. The
    /// discovery layer normally guards this; the seam stays typed anyway.
    NoEngines(String),
    ReadFailed(String),
}

/// Open, sample and compute the per-engine busy ratios for one discovered PMU
/// layout. Returns the JSON engine array on success.
pub fn sample(layout: PmuLayout, sample_ms: u32) -> Result<Vec<EngineJson>, SampleError> {
    match layout {
        PmuLayout::Xe {
            pmu_type,
            cpu,
            engines,
        } => sample_xe(pmu_type, cpu, &engines, sample_ms),
        PmuLayout::I915 {
            pmu_type,
            cpu,
            engines,
        } => sample_i915(pmu_type, cpu, &engines, sample_ms),
    }
}

/// xe: open an active+total counter pair per engine, sleep, read both, and form
/// `active_delta / total_delta * 100`. A `total_delta == 0` interval skips that
/// engine (a typed gap, never a divide-by-zero).
fn sample_xe(
    pmu_type: u32,
    cpu: i32,
    engines: &[XeEngineCfg],
    sample_ms: u32,
) -> Result<Vec<EngineJson>, SampleError> {
    // Open + reset + enable both counters per engine up front so the measurement
    // window is shared. A failed open is retained; if NO pair opened, the
    // dominant open error classifies the whole sample — or, when nothing was
    // even attempted (an empty engine list), a typed `NoEngines` gap.
    let mut pairs: Vec<(&XeEngineCfg, GpuEngineCounter, GpuEngineCounter)> = Vec::new();
    let mut first_open_error: Option<io::Error> = None;
    for engine in engines {
        let active = GpuEngineCounter::open_enabled(pmu_type, engine.active_config, cpu);
        let total = GpuEngineCounter::open_enabled(pmu_type, engine.total_config, cpu);
        match (active, total) {
            (Ok(active), Ok(total)) => pairs.push((engine, active, total)),
            (Err(error), _) | (_, Err(error)) => {
                first_open_error = Some(first_open_error.unwrap_or(error));
            }
        }
    }
    if pairs.is_empty() {
        return Err(empty_pairs_error(first_open_error));
    }

    thread::sleep(Duration::from_millis(u64::from(sample_ms)));

    let mut sampled: Vec<SampledEngine> = Vec::new();
    let mut first_read_error: Option<io::Error> = None;
    for (engine, mut active, mut total) in pairs {
        let active_value = active.read_counter();
        let total_value = total.read_counter();
        match (active_value, total_value) {
            (Ok(active_delta), Ok(total_delta)) => {
                if total_delta == 0 {
                    // No elapsed ticks in the interval — a typed gap. The
                    // engine is skipped rather than emitting 0% or NaN.
                    continue;
                }
                let busy_pct = (active_delta as f32 / total_delta as f32 * 100.0).clamp(0.0, 100.0);
                sampled.push(SampledEngine {
                    label: engine.label.clone(),
                    class_name: engine.class_name.clone(),
                    busy_pct,
                });
            }
            (Err(error), _) | (_, Err(error)) => {
                first_read_error = Some(first_read_error.unwrap_or(error));
            }
        }
    }

    finalize(sampled, first_read_error)
}

/// i915: open one cumulative-busy-ns counter per engine, sleep, read, and form
/// `busy_ns / elapsed_ns * 100` over the measured wall window.
fn sample_i915(
    pmu_type: u32,
    cpu: i32,
    engines: &[I915EngineCfg],
    sample_ms: u32,
) -> Result<Vec<EngineJson>, SampleError> {
    let mut counters: Vec<(&I915EngineCfg, GpuEngineCounter)> = Vec::new();
    let mut first_open_error: Option<io::Error> = None;
    for engine in engines {
        match GpuEngineCounter::open_enabled(pmu_type, engine.config, cpu) {
            Ok(counter) => counters.push((engine, counter)),
            Err(error) => first_open_error = Some(first_open_error.unwrap_or(error)),
        }
    }
    if counters.is_empty() {
        return Err(empty_pairs_error(first_open_error));
    }

    let started = Instant::now();
    thread::sleep(Duration::from_millis(u64::from(sample_ms)));
    let elapsed_ns = started.elapsed().as_nanos();

    let mut sampled: Vec<SampledEngine> = Vec::new();
    let mut first_read_error: Option<io::Error> = None;
    for (engine, mut counter) in counters {
        match counter.read_counter() {
            Ok(busy_ns) => {
                if elapsed_ns == 0 {
                    continue;
                }
                let busy_pct = (busy_ns as f32 / elapsed_ns as f32 * 100.0).clamp(0.0, 100.0);
                sampled.push(SampledEngine {
                    label: engine.label.clone(),
                    class_name: engine.class_name.clone(),
                    busy_pct,
                });
            }
            Err(error) => first_read_error = Some(first_read_error.unwrap_or(error)),
        }
    }

    finalize(sampled, first_read_error)
}

/// Fold the sampled engines into the JSON array, or — if none produced data —
/// surface the dominant read error as a typed [`SampleError::ReadFailed`].
fn finalize(
    sampled: Vec<SampledEngine>,
    first_read_error: Option<io::Error>,
) -> Result<Vec<EngineJson>, SampleError> {
    if sampled.is_empty() {
        return Err(SampleError::ReadFailed(
            first_read_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "no engine produced a reading".to_string()),
        ));
    }
    Ok(sampled
        .into_iter()
        .map(|engine| EngineJson {
            name: engine.label,
            class: engine.class_name,
            busy_pct: engine.busy_pct,
        })
        .collect())
}

/// `EACCES` — Linux errno 13. `perf_event_open` returns it under a restrictive
/// `perf_event_paranoid` (e.g. paranoid = 2 denies unprivileged perf). Stable
/// Linux errno value, never renumbered; named here so the helper does not pull
/// `libc` as a direct dependency (keeps the privileged surface minimal).
const ERR_EACCES: i32 = 13;
/// `EPERM` — Linux errno 1. Some kernels return it instead of `EACCES` for the
/// same denial; treated identically (the escalatable permission-denied path).
const ERR_EPERM: i32 = 1;

/// Map a `perf_event_open` failure to the contract's typed error kind:
/// `EACCES`/`EPERM` (a restrictive `perf_event_paranoid`) is
/// [`SampleError::PermissionDenied`] — the escalatable denial the OS-native
/// prompt reaches; anything else is [`SampleError::OpenFailed`].
fn classify_open_error(error: io::Error) -> SampleError {
    match error.raw_os_error() {
        Some(ERR_EACCES) | Some(ERR_EPERM) => SampleError::PermissionDenied(error.to_string()),
        _ => SampleError::OpenFailed(error.to_string()),
    }
}

/// Fold an all-failed open phase into a typed error: with at least one open
/// failure the dominant error classifies the sample; with NONE recorded the
/// engine list was empty — a typed gap, never a panic.
fn empty_pairs_error(first_open_error: Option<io::Error>) -> SampleError {
    match first_open_error {
        Some(error) => classify_open_error(error),
        None => {
            SampleError::NoEngines("no engines to sample: the engine list was empty".to_string())
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/privilege_sample.rs"]
mod tests;

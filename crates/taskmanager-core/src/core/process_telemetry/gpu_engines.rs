//! Per-process GPU engine utilization, typed and never fabricated.
//!
//! Each open DRM file descriptor exposes a cumulative `drm-engine-<name>: <ns>`
//! busy counter in `/proc/<pid>/fdinfo/<fd>` (kernel DRM usage stats). A
//! *cumulative* nanosecond counter is only meaningful as a delta over a measured
//! interval, so the per-engine [`ProcessGpuEngineUsage::usage_pct`] is a typed
//! [`ScalarObservation`]: the first sighting seeds the baseline (a typed gap,
//! never a fabricated zero), and only a later tick produces a current 0–100%
//! single-core-equivalent rate.
//!
//! Honesty contract (the project red line): a process with no DRM render/card
//! descriptors, a vanished pid, or a permission-denied `/proc/<pid>/fd` is a
//! typed [`DeviceState`] with an empty `engines` list — never an invented
//! engine, never a fabricated zero percentage.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;
use crate::core::metrics::ScalarObservation;

/// One GPU engine's utilization for a single process.
///
/// `name` is the engine class parsed verbatim from the fdinfo key (for example
/// `render`, `video`, `copy`, or a vendor-specific identifier like `rcs`) so
/// the caller never loses information. `usage_pct` is single-core-equivalent —
/// the same convention the container CPU rollup and the system-wide Intel engine
/// tracker use — and is `Unavailable` on the first sample (no delta yet) or when
/// the cumulative counter rolled back (driver reset / fd recycled).
///
/// `engine_time_ns` is the cumulative busy time observed this tick, aggregated
/// across every DRM descriptor the process holds. Unlike the rate, the
/// cumulative counter is observable on every healthy read, so it stays
/// `Available` from the first sighting — giving an honest cold-start reading
/// even before a rate can be computed.
///
/// `engine_cycles` carries the xe-driver cycle counter when the kernel exposes
/// cycles instead of busy nanoseconds (`drm-total-cycles-<class>` /
/// `drm-cycles-<class>`, kernel DRM usage stats). Cycles alone cannot be
/// converted to a utilization percentage without the GT clock, so on a
/// cycles-only source `usage_pct` stays a typed gap and the raw cycle count is
/// the honest observable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessGpuEngineUsage {
    /// Engine class as it appeared in the fdinfo key.
    pub name: String,
    /// Single-core-equivalent 0–100% utilization, typed for the cold-start gap
    /// and counter rollback. A cycles-only source (xe) keeps this a typed gap.
    pub usage_pct: ScalarObservation<f32>,
    /// Cumulative DRM busy time for this engine across the process's DRM
    /// descriptors, in nanoseconds. `None`-typed when the driver exposes
    /// cycles instead of busy time (xe).
    pub engine_time_ns: ScalarObservation<u64>,
    /// Cumulative engine cycle counter (xe `drm-total-cycles-<class>` /
    /// `drm-cycles-<class>`). `Unknown` when the driver reports busy ns (i915).
    #[serde(default)]
    pub engine_cycles: ScalarObservation<u64>,
}

/// The per-process GPU engine breakdown plus a typed collection state.
///
/// `state` describes the `/proc/<pid>/fd` + `fdinfo` collection as a whole: a
/// permission-denied descriptor directory is `PermissionDenied`, a vanished pid
/// is `Stale`, and a healthy process with no DRM descriptors is `Healthy` with
/// an empty `engines` list (an honest empty, not an unknown).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProcessGpuEngines {
    /// Typed health of the fd/fdinfo collection that produced this breakdown.
    pub state: DeviceState,
    /// Per-engine entries, ordered by ascending engine name for stable diffing.
    pub engines: Vec<ProcessGpuEngineUsage>,
}

impl ProcessGpuEngines {
    /// A healthy breakdown over no engines — the honest representation of a
    /// live, non-GPU process (no `/dev/dri/` descriptors open).
    #[must_use]
    pub fn empty_healthy(now_ms: u64) -> Self {
        Self {
            state: DeviceState::healthy(now_ms),
            engines: Vec::new(),
        }
    }

    /// A breakdown whose source was typed-unavailable (EACCES on
    /// `/proc/<pid>/fd`, a vanished pid, ...). The engine list is always empty
    /// here: a failed source must never retain fabricated rows.
    #[must_use]
    pub fn unavailable(state: DeviceState) -> Self {
        Self {
            state,
            engines: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_telemetry_gpu_engines_tests.rs"]
mod tests;

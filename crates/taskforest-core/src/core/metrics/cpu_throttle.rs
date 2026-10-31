//! Cumulative CPU thermal-throttle trigger counters (the CpuThrottle request
//! lane) and the single aggregation rule shared by every read path.
//!
//! The per-package `package_throttle_count`/`core_throttle_count` fields of
//! [`CpuPackageMetrics`] are the delivery authority: the periodic CPU
//! projection carries them to every frontend, and the
//! `power.thermal-throttle-events` definition names them. [`CpuThrottleSnapshot`]
//! is the same fact projected for the `telemetry.cpu.throttle` lane answer.
//!
//! Both carriers are produced by [`aggregate_package_throttle_counters`], the
//! ONE authority for the aggregation rules: sibling hyperthreads of one
//! physical core are de-duplicated by `(package, core)` (a readable sibling
//! supplies the core's value), core events are summed per package, and the
//! package counter is the maximum across the logical CPUs repeating it. No
//! second aggregation exists, and a counter that no read could observe stays
//! `None` on both paths instead of becoming a fabricated zero.
//!
//! [`CpuPackageMetrics`]: super::CpuPackageMetrics

use serde::{Deserialize, Serialize};

use crate::core::FailureKind;

/// One logical CPU's raw counter read plus the physical identity needed to
/// attribute and de-duplicate it.
///
/// The native adapter owns reading sysfs (`/sys/devices/system/cpu/cpuN/...`);
/// this value is the provider-neutral input of the one aggregation rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuThrottleCounterSample {
    /// Zero-based physical socket/package index (`physical_package_id`).
    pub package_id: u32,
    /// Physical core identity within the package (`core_id`), when the kernel
    /// exposed it. `None` means the core counter cannot be attributed or
    /// de-duplicated, so the sample contributes only its package counter.
    pub core_id: Option<u32>,
    /// The counter read on this logical CPU, `None` when unreadable.
    pub package_throttle_count: Option<u64>,
    /// The per-physical-core counter read on this logical CPU, `None` when
    /// unreadable or when [`Self::core_id`] is `None`.
    pub core_throttle_count: Option<u64>,
}

/// Cumulative thermal-throttle trigger counters for one physical package.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuThrottlePackageCounters {
    /// Zero-based physical socket or package index.
    pub package_id: u32,
    /// Maximum package-level event counter observed on this package's logical
    /// CPUs; `None` when no read observed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_throttle_count: Option<u64>,
    /// Sum of the package's physical-core event counters; `None` when no core
    /// counter was readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_throttle_count: Option<u64>,
}

/// The answer to one `telemetry.cpu.throttle` request: per-package cumulative
/// trigger counters plus a typed reason when the read itself failed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuThrottleSnapshot {
    /// The per-package counters, sorted by package index.
    pub packages: Vec<CpuThrottlePackageCounters>,
    /// Typed reason the counter read could not run at all; per-counter absences
    /// stay `None` on the rows instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<CpuThrottleFailure>,
}

/// One typed reason a CPU throttle-counter request could not produce readings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuThrottleFailure {
    pub kind: FailureKind,
    pub detail: String,
}

impl CpuThrottleSnapshot {
    /// Successful read: real counter rows (a row may still carry `None` for a
    /// counter this host does not expose), never a failure tag.
    #[must_use]
    pub const fn success(packages: Vec<CpuThrottlePackageCounters>) -> Self {
        Self {
            packages,
            failure: None,
        }
    }

    /// Failed read: a typed reason, never a fabricated zero.
    #[must_use]
    pub fn failed(kind: FailureKind, detail: impl Into<String>) -> Self {
        Self {
            packages: Vec::new(),
            failure: Some(CpuThrottleFailure {
                kind,
                detail: detail.into(),
            }),
        }
    }

    /// True when this snapshot carries a real (possibly empty) counter read.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.failure.is_none()
    }
}

/// The single aggregation rule for cumulative CPU throttle counters.
///
/// Input order does not matter: samples are grouped by package and physical
/// core, so every caller that read the same kernel counters derives the same
/// rows. Packages without any observed counter still produce a row so a caller
/// can tell "this package exposes no counters" from "this package is absent".
#[must_use]
pub fn aggregate_package_throttle_counters(
    samples: &[CpuThrottleCounterSample],
) -> Vec<CpuThrottlePackageCounters> {
    use std::collections::BTreeMap;

    let mut packages = BTreeMap::<u32, (Option<u64>, BTreeMap<u32, u64>)>::new();
    for sample in samples {
        let entry = packages.entry(sample.package_id).or_default();
        if let Some(value) = sample.package_throttle_count {
            entry.0 = Some(entry.0.map_or(value, |current| current.max(value)));
        }
        if let (Some(core_id), Some(value)) = (sample.core_id, sample.core_throttle_count) {
            entry.1.insert(core_id, value);
        }
    }
    packages
        .into_iter()
        .map(|(package_id, (package_events, cores))| {
            let core_events = cores.into_values().fold(None, |total: Option<u64>, value| {
                Some(total.map_or(value, |sum| sum.saturating_add(value)))
            });
            CpuThrottlePackageCounters {
                package_id,
                package_throttle_count: package_events,
                core_throttle_count: core_events,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_cpu_throttle_tests.rs"]
mod tests;

//! On-demand CPU package-power snapshots (the PackagePowerRapl request lane).
//!
//! The periodic [`CpuMetrics`] `power_w` projection stays on the unprivileged
//! refresh tick; these snapshots answer a frontend-paced request/response
//! lane backed by the privileged RAPL helper (ADR-023, permission-model
//! Boundary 2), which samples the root-only `energy_uj` counters over one
//! fixed window and derives per-package watts. A typed failure never carries a
//! fabricated watt figure.

use serde::{Deserialize, Serialize};

use crate::core::FailureKind;

/// One typed reason a package-power request could not produce readings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaplPowerFailure {
    pub kind: FailureKind,
    pub detail: String,
}

/// One package's average power over the helper's sample window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RaplPackageRow {
    /// Package label from sysfs, e.g. `"package-1"`.
    pub name: String,
    /// Average package power over the sample window, in watts.
    pub power_w: f32,
    /// The raw energy delta over the sample window, in microjoules.
    pub energy_delta_uj: u64,
}

/// The answer to one package-power request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RaplPowerSnapshot {
    /// The measurement window in milliseconds the helper sampled.
    pub sample_ms: u32,
    /// The per-package power readings, sorted by package index.
    pub packages: Vec<RaplPackageRow>,
    pub failure: Option<RaplPowerFailure>,
}

impl RaplPowerSnapshot {
    /// Successful read: real watt figures, never a failure tag.
    #[must_use]
    pub fn success(sample_ms: u32, packages: Vec<RaplPackageRow>) -> Self {
        Self {
            sample_ms,
            packages,
            failure: None,
        }
    }

    /// Failed read: a typed reason, never a fabricated watt figure.
    #[must_use]
    pub fn failed(kind: FailureKind, detail: impl Into<String>) -> Self {
        Self {
            sample_ms: 0,
            packages: Vec::new(),
            failure: Some(RaplPowerFailure {
                kind,
                detail: detail.into(),
            }),
        }
    }

    /// True when this snapshot carries real package readings.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.failure.is_none()
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_rapl_power_tests.rs"]
mod tests;

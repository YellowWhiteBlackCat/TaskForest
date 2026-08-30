//! On-demand CPU MSR readout snapshots (the CpuMsr request lane).
//!
//! The periodic [`CpuMetrics`] projections stay on the unprivileged refresh
//! tick; these snapshots answer a frontend-paced request/response lane backed
//! by the privileged MSR helper (ADR-023/048, permission-model Boundary 2),
//! which reads the root-only `/dev/cpu/N/msr` registers once per request.
//! Every register field the CPU does not implement stays `None` — a typed
//! absence, never a fabricated zero.

use serde::{Deserialize, Serialize};

use crate::core::FailureKind;

/// One typed reason an MSR-readout request could not produce readings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MsrReadoutFailure {
    pub kind: FailureKind,
    pub detail: String,
}

/// One CPU node's MSR readout, copied field-by-field from the helper's
/// contract. Register fields the CPU does not implement stay `None`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MsrPackageReadout {
    /// The numeric suffix `N` of the `/dev/cpu/N` node.
    pub cpu: u32,
    /// Base clock in MHz; `None` until a verified derivation exists.
    pub bclk_mhz: Option<f32>,
    /// Package temperature in °C.
    pub temperature_c: Option<f32>,
    /// Current performance ratio.
    pub multiplier: Option<f32>,
    /// Maximum efficiency ratio (minimum multiplier).
    pub multiplier_min: Option<f32>,
    /// Maximum 1-core turbo ratio.
    pub multiplier_max: Option<f32>,
    /// P-state core voltage in volts.
    pub vcore_v: Option<f32>,
}

/// The answer to one MSR-readout request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MsrReadoutSnapshot {
    /// The per-node readouts, sorted by node index.
    pub packages: Vec<MsrPackageReadout>,
    pub failure: Option<MsrReadoutFailure>,
}

impl MsrReadoutSnapshot {
    /// Successful read: real register rows, never a failure tag.
    #[must_use]
    pub fn success(packages: Vec<MsrPackageReadout>) -> Self {
        Self {
            packages,
            failure: None,
        }
    }

    /// Failed read: a typed reason, never a fabricated register value.
    #[must_use]
    pub fn failed(kind: FailureKind, detail: impl Into<String>) -> Self {
        Self {
            packages: Vec::new(),
            failure: Some(MsrReadoutFailure {
                kind,
                detail: detail.into(),
            }),
        }
    }

    /// True when this snapshot carries real MSR readouts.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.failure.is_none()
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_msr_readout_tests.rs"]
mod tests;

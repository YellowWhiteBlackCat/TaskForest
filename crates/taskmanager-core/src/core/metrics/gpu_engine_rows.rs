//! On-demand per-engine GPU utilization snapshots (the PMU request lane).
//!
//! Unlike the periodic [`GpuEngineMetricPoint`] history (fed by the unprivileged
//! `drm-engine` fdinfo path), these snapshots answer a frontend-paced
//! request/response lane backed by the privileged Intel PMU helper
//! (ADR-023, permission-model Boundary 2). The request targets one GPU device;
//! the provider answers with exactly one snapshot — real rows on success, a
//! typed failure otherwise — so no consumer can mistake a denied or missing
//! helper for zero-valued engines.

use serde::{Deserialize, Serialize};

use crate::core::{DeviceGeneration, DeviceId, FailureKind, GpuEngineMetric};

/// One typed reason an engine-rows request could not produce live rows.
///
/// `detail` is a host-specific diagnostic for logs/diagnostics panels; `kind`
/// alone drives every state-machine decision so consumers never parse text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuEngineRowsFailure {
    pub kind: FailureKind,
    pub detail: String,
}

/// A device-scoped answer to one engine-rows request.
///
/// `engines` is non-empty only on a successful helper read: an empty list plus
/// `failure: None` is the honest "helper ran but reported no engines" case
/// (mirroring the panel's contract), while any `failure` means no row in this
/// snapshot is real. `device_generation` is zero (pre-lifecycle, see the
/// periodic `GpuEngineMetricPoint` contract) because this lane answers a UI
/// request rather than joining the periodic device lifecycle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GpuEngineRowsSnapshot {
    pub device_id: DeviceId,
    pub device_generation: DeviceGeneration,
    pub engines: Vec<GpuEngineMetric>,
    pub failure: Option<GpuEngineRowsFailure>,
}

impl GpuEngineRowsSnapshot {
    /// Successful read: real rows, never a failure tag.
    #[must_use]
    pub fn success(device_id: DeviceId, engines: Vec<GpuEngineMetric>) -> Self {
        Self {
            device_id,
            device_generation: DeviceGeneration::default(),
            engines,
            failure: None,
        }
    }

    /// Failed read: a typed reason, never a fabricated row.
    #[must_use]
    pub fn failed(device_id: DeviceId, kind: FailureKind, detail: impl Into<String>) -> Self {
        Self {
            device_id,
            device_generation: DeviceGeneration::default(),
            engines: Vec::new(),
            failure: Some(GpuEngineRowsFailure {
                kind,
                detail: detail.into(),
            }),
        }
    }

    /// True when this snapshot carries real engine rows.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.failure.is_none()
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_gpu_engine_rows_tests.rs"]
mod tests;

//! Platform-neutral NPU (neural processing unit) accelerator inventory.
//!
//! Discovery-first model: the native adapter enumerates
//! accelerator devices (Linux `/sys/class/accel`, Windows MCDM adapters,
//! macOS ANE) and reports each device's identity with typed availability.
//! Live utilization stays a typed observation — providers that cannot read a
//! stable kernel interface report `Unavailable(Unsupported)` instead of a
//! fabricated curve, and an empty device list with no failure is the honest
//! "this host has no NPU" state, never an error.

use serde::{Deserialize, Serialize};

use crate::core::{DeviceGeneration, DeviceId, FailureKind, ScalarObservation};

/// Compute-engine classes an NPU can report utilization for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum NpuEngineKind {
    /// General compute tiles.
    Compute,
    /// Matrix-multiply units (the dominant NPU workload class).
    Matrix,
    /// Vector units.
    Vector,
    /// Video/image pre/post-processing blocks.
    Video,
    /// Copy/DMA engines.
    Copy,
    /// Engine reported by the native source under an unmapped name.
    #[default]
    Unknown,
}

impl NpuEngineKind {
    /// Complete variant list; consumers enumerate this instead of keeping a
    /// duplicated list.
    pub const ALL: &'static [Self] = &[
        Self::Compute,
        Self::Matrix,
        Self::Vector,
        Self::Video,
        Self::Copy,
        Self::Unknown,
    ];
}

/// One engine's utilization with typed availability.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct NpuEngineUsage {
    pub kind: NpuEngineKind,
    pub utilization_pct: ScalarObservation<f32>,
}

/// Accelerator memory facts split by hardware ownership.
///
/// Both fields stay typed: a device without dedicated memory reports
/// `Unavailable(Unsupported)` or a measured zero, never an inferred size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct NpuMemoryReport {
    /// Dedicated on-package memory (e.g. NPU RAM) in bytes.
    pub dedicated_total_bytes: ScalarObservation<u64>,
    /// Shared system-memory commitment in bytes.
    pub shared_total_bytes: ScalarObservation<u64>,
}

/// One discovered NPU accelerator device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NpuDevice {
    pub device_id: DeviceId,
    /// Hotplug generation; request/response inventory lanes leave this at the
    /// default pre-lifecycle value until the device joins a lifecycle
    /// registry.
    pub device_generation: DeviceGeneration,
    /// Marketing brand (e.g. "Intel AI Boost"). `None` when only a raw
    /// vendor/device id is known; frontends must not invent one.
    pub brand: Option<String>,
    /// Native driver name (e.g. `intel_vpu`), a provider-reported fact.
    pub driver: Option<String>,
    /// Aggregate utilization. Until a stable kernel interface exists this is
    /// `Unavailable(Unsupported)` — discovery never fabricates a curve.
    pub utilization_pct: ScalarObservation<f32>,
    /// Per-engine rows; empty means the provider reports none.
    pub engines: Vec<NpuEngineUsage>,
    pub memory: NpuMemoryReport,
}

/// One typed reason an inventory request could not enumerate accelerators.
///
/// `detail` is a host-specific diagnostic; `kind` alone drives state-machine
/// decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpuInventoryFailure {
    pub kind: FailureKind,
    pub detail: String,
}

/// Host-scoped answer to one NPU inventory request.
///
/// `devices` is non-empty only on a successful enumeration: an empty list with
/// `failure: None` is the honest "enumeration ran, no NPU present" state,
/// while any `failure` means no device row in this snapshot is real.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NpuInventorySnapshot {
    pub observed_at_ms: u64,
    pub devices: Vec<NpuDevice>,
    pub failure: Option<NpuInventoryFailure>,
}

impl NpuInventorySnapshot {
    /// Successful enumeration. Devices are sorted by id so UI order is
    /// deterministic regardless of native enumeration order.
    #[must_use]
    pub fn discovered(mut devices: Vec<NpuDevice>, observed_at_ms: u64) -> Self {
        devices.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        Self {
            observed_at_ms,
            devices,
            failure: None,
        }
    }

    /// Failed enumeration: a typed reason, never a fabricated device.
    #[must_use]
    pub fn failed(kind: FailureKind, detail: impl Into<String>, observed_at_ms: u64) -> Self {
        Self {
            observed_at_ms,
            devices: Vec::new(),
            failure: Some(NpuInventoryFailure {
                kind,
                detail: detail.into(),
            }),
        }
    }

    /// True when this snapshot carries a real enumeration (including the
    /// honest empty host).
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.failure.is_none()
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_npu_tests.rs"]
mod tests;

//! GPU telemetry metrics: utilization, memory (dedicated/shared), frequency,
//! temperature, fan, power, idle-residency scalar observations, and optional
//! runtime graphics API facts with availability, per-field provenance,
//! per-engine usage, and typed throttle reasons.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;
use crate::core::{DeviceGeneration, DeviceId, FailureKind, ProviderId};

use super::{ScalarAvailability, ScalarObservation};

mod wire;

/// Provider-proven semantic class for one GPU engine.
///
/// `Unknown` is intentional: a provider may expose a real named engine while
/// not proving its class. Consumers must not infer encode/decode semantics
/// from an arbitrary future display name.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum GpuEngineKind {
    Render,
    Compute,
    Copy,
    VideoDecode,
    VideoEncode,
    #[default]
    Unknown,
}

impl GpuEngineKind {
    /// Map stable provider-neutral display labels emitted by native providers.
    /// Unknown labels remain `Unknown` rather than receiving a guessed meaning.
    #[must_use]
    pub fn from_display_name(name: &str) -> Self {
        if name.eq_ignore_ascii_case("Render/3D")
            || name.eq_ignore_ascii_case("Graphics (3D)")
            || name.eq_ignore_ascii_case("Render")
            || name.eq_ignore_ascii_case("3D")
        {
            Self::Render
        } else if name.eq_ignore_ascii_case("Compute") {
            Self::Compute
        } else if name.eq_ignore_ascii_case("Copy")
            || name.eq_ignore_ascii_case("Memory (Copy)")
            || name.eq_ignore_ascii_case("Blitter")
        {
            Self::Copy
        } else if name.eq_ignore_ascii_case("Video Decode") {
            Self::VideoDecode
        } else if name.eq_ignore_ascii_case("Video Encode")
            || name.eq_ignore_ascii_case("Video Processing")
        {
            Self::VideoEncode
        } else {
            Self::Unknown
        }
    }
}

/// One GPU engine's instantaneous utilization.
///
/// Native providers may name graphics, compute, copy, video, or other engines
/// using their stable display vocabulary. `usage_pct` is 0.0–100.0; an empty
/// engine list means the selected providers supplied no per-engine facts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuEngine {
    pub name: String,
    /// Provider-confirmed semantic class; `Unknown` preserves a real but
    /// unclassified engine without guessing its role.
    #[serde(default)]
    pub kind: GpuEngineKind,
    pub usage_pct: f32,
}

/// One finite per-engine utilization value retained by correlated history.
/// This is separate from [`GpuEngine`] so history owns an immutable,
/// provider-neutral point rather than retaining a mutable snapshot row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuEngineMetric {
    pub name: String,
    #[serde(default)]
    pub kind: GpuEngineKind,
    pub utilization_pct: f32,
}

/// A device- and generation-scoped per-engine GPU point.
///
/// The identity is carried in the point as well as in the history map. This
/// makes accidental cross-device or cross-generation reuse detectable by a
/// consumer instead of relying on a parallel key convention. Generation zero
/// remains representable for pre-lifecycle snapshots, but
/// [`is_generation_scoped`](Self::is_generation_scoped) reports that such a
/// point has not yet passed the lifecycle assembler.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GpuEngineMetricPoint {
    #[serde(default)]
    pub device_id: DeviceId,
    #[serde(default)]
    pub device_generation: DeviceGeneration,
    /// `None` means no engine failure receipt. A non-empty engine list plus a
    /// failure is a partial capability read; an empty list plus a failure is
    /// an unavailable capability. This keeps the wire contract toolkit- and
    /// provider-neutral without turning an unavailable engine into zero.
    #[serde(default)]
    pub engine_failure: Option<FailureKind>,
    #[serde(default)]
    pub engine_provider: Option<ProviderId>,
    pub engines: Vec<GpuEngineMetric>,
}

impl GpuEngineMetricPoint {
    /// Retain only finite, named engine values. An empty result is `None` so
    /// an absent provider field becomes a graph gap instead of an empty or
    /// fabricated engine series. An explicit provider failure is retained as
    /// an empty typed point so the history can distinguish a denied/unsupported
    /// capability from a snapshot that never made an engine claim.
    #[must_use]
    pub fn from_metrics(metrics: &GpuMetrics) -> Option<Self> {
        if metrics.device_id.trim().is_empty() {
            return None;
        }
        let mut engines = metrics
            .engines
            .iter()
            .filter(|engine| {
                !engine.name.trim().is_empty()
                    && engine.usage_pct.is_finite()
                    && (0.0..=100.0).contains(&engine.usage_pct)
            })
            .map(|engine| GpuEngineMetric {
                name: engine.name.clone(),
                kind: engine.kind,
                utilization_pct: engine.usage_pct,
            })
            .collect::<Vec<_>>();
        engines.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.kind.cmp(&right.kind))
        });
        let mut unique: Vec<GpuEngineMetric> = Vec::with_capacity(engines.len());
        for engine in engines {
            if let Some(previous) = unique.last()
                && previous.name == engine.name
            {
                if previous.kind != engine.kind {
                    // The same display identity cannot safely carry two
                    // semantic classes in one point. Drop the point
                    // rather than selecting one meaning arbitrarily.
                    return None;
                }
                if previous.utilization_pct != engine.utilization_pct {
                    // Duplicate values with the same label are also
                    // ambiguous: choosing one would make a multi-provider or
                    // multi-instance merge look deterministic while hiding a
                    // real disagreement.
                    return None;
                }
                continue;
            }
            unique.push(engine);
        }
        if unique.is_empty() && metrics.engine_failure.is_none() && metrics.engines.is_empty() {
            return None;
        }
        Some(Self {
            device_id: DeviceId::new(metrics.device_id.clone()),
            device_generation: metrics.device_generation,
            engine_failure: metrics.engine_failure,
            engine_provider: metrics.engine_provider.clone(),
            engines: unique,
        })
    }

    /// Check that a point belongs to the exact device generation requested by
    /// a consumer. A stable device ID alone is insufficient after hotplug.
    #[must_use]
    pub fn scope_matches(&self, device_id: &str, generation: DeviceGeneration) -> bool {
        self.device_id.as_str() == device_id && self.device_generation == generation
    }

    #[must_use]
    pub const fn is_generation_scoped(&self) -> bool {
        self.device_generation.is_valid()
    }
}

/// Stable GPU field vocabulary used by native provider registries to record
/// which runtime source supplied each merged value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuMetricField {
    Identity,
    Brand,
    GraphicsApi,
    Utilization,
    IdleResidency,
    Memory,
    DedicatedVram,
    SharedVram,
    Engines,
    Temperature,
    Power,
    Fan,
    Frequency,
    Throttle,
    Driver,
    DriverVersion,
}

/// Provider-neutral reasons why a GPU is currently operating below its
/// unrestricted clock target. Vendor bitmasks remain inside native providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuThrottleReason {
    /// No work is currently eligible to run at higher clocks.
    Idle,
    /// An application-requested clock ceiling is active.
    ApplicationClockLimit,
    /// A software power-management policy is reducing clocks.
    SoftwarePowerLimit,
    /// A generic hardware protection path is reducing clocks. Use the more
    /// specific thermal or power-brake reasons when the provider proves them.
    HardwareSlowdown,
    /// A board-reliability policy is currently constraining clocks.
    ///
    /// Providers must not infer this from a historical cumulative counter.
    ReliabilityLimit,
    /// Another device in a synchronized boost group constrains this device.
    SyncBoost,
    /// A software thermal controller is reducing clocks.
    SoftwareThermalLimit,
    /// A hardware thermal protection path is reducing clocks.
    HardwareThermalLimit,
    /// An external hardware power-brake signal is reducing clocks.
    ExternalPowerBrake,
    /// The configured display-clock ceiling constrains GPU clocks.
    DisplayClockLimit,
    /// A provider reported at least one active limiter that this application
    /// version cannot classify yet.
    Other,
}

/// Per-field provenance for one merged GPU read model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuMetricProvenance {
    pub field: GpuMetricField,
    pub provider: ProviderId,
}

/// Runtime graphics API versions proven for one GPU by a platform provider.
///
/// These are capability observations, not driver-name guesses. A provider may
/// leave either field absent when the loader/tool/context is unavailable, and
/// consumers must omit that row rather than infer a version from the driver.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpuGraphicsApi {
    pub opengl_version: Option<String>,
    pub vulkan_version: Option<String>,
}

/// Independently fallible provider-neutral GPU scalar measurements.
///
/// Historical numeric and optional fields exist only in the private
/// `GpuMetricsWire` compatibility boundary. Providers assemble this group
/// once and consumers read it through [`GpuMetrics`] accessors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GpuScalarObservations {
    pub utilization_pct: ScalarObservation<f32>,
    pub temperature_c: ScalarObservation<f32>,
    pub memory_used_bytes: ScalarObservation<u64>,
    pub memory_total_bytes: ScalarObservation<u64>,
    /// Dedicated on-card VRAM usage. `Available(0)` is a measured idle board;
    /// a missing vendor counter is `Unavailable`, never a believable zero.
    pub dedicated_vram_used_bytes: ScalarObservation<u64>,
    /// Dedicated on-card VRAM capacity. Absent on unified-memory iGPUs.
    pub dedicated_vram_total_bytes: ScalarObservation<u64>,
    /// Provider-defined shared GTT/system-memory aperture usage.
    pub shared_vram_used_bytes: ScalarObservation<u64>,
    /// Provider-defined shared GTT/system-memory aperture capacity.
    pub shared_vram_total_bytes: ScalarObservation<u64>,
    pub frequency_mhz: ScalarObservation<u64>,
    pub max_frequency_mhz: ScalarObservation<u64>,
    pub fan_speed_rpm: ScalarObservation<u64>,
    pub fan_speed_pct: ScalarObservation<f32>,
    pub power_w: ScalarObservation<f32>,
    pub idle_residency_pct: ScalarObservation<f32>,
}

impl GpuScalarObservations {
    /// Retain prior successful values only as stale when a field fails.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        Self {
            utilization_pct: self
                .utilization_pct
                .retain_previous(previous.utilization_pct),
            temperature_c: self.temperature_c.retain_previous(previous.temperature_c),
            memory_used_bytes: self
                .memory_used_bytes
                .retain_previous(previous.memory_used_bytes),
            memory_total_bytes: self
                .memory_total_bytes
                .retain_previous(previous.memory_total_bytes),
            dedicated_vram_used_bytes: self
                .dedicated_vram_used_bytes
                .retain_previous(previous.dedicated_vram_used_bytes),
            dedicated_vram_total_bytes: self
                .dedicated_vram_total_bytes
                .retain_previous(previous.dedicated_vram_total_bytes),
            shared_vram_used_bytes: self
                .shared_vram_used_bytes
                .retain_previous(previous.shared_vram_used_bytes),
            shared_vram_total_bytes: self
                .shared_vram_total_bytes
                .retain_previous(previous.shared_vram_total_bytes),
            frequency_mhz: self.frequency_mhz.retain_previous(previous.frequency_mhz),
            max_frequency_mhz: self
                .max_frequency_mhz
                .retain_previous(previous.max_frequency_mhz),
            fan_speed_rpm: self.fan_speed_rpm.retain_previous(previous.fan_speed_rpm),
            fan_speed_pct: self.fan_speed_pct.retain_previous(previous.fan_speed_pct),
            power_w: self.power_w.retain_previous(previous.power_w),
            idle_residency_pct: self
                .idle_residency_pct
                .retain_previous(previous.idle_residency_pct),
        }
    }

    #[must_use]
    pub fn unavailable(failure: FailureKind) -> Self {
        Self {
            utilization_pct: ScalarObservation::unavailable(failure),
            temperature_c: ScalarObservation::unavailable(failure),
            memory_used_bytes: ScalarObservation::unavailable(failure),
            memory_total_bytes: ScalarObservation::unavailable(failure),
            dedicated_vram_used_bytes: ScalarObservation::unavailable(failure),
            dedicated_vram_total_bytes: ScalarObservation::unavailable(failure),
            shared_vram_used_bytes: ScalarObservation::unavailable(failure),
            shared_vram_total_bytes: ScalarObservation::unavailable(failure),
            frequency_mhz: ScalarObservation::unavailable(failure),
            max_frequency_mhz: ScalarObservation::unavailable(failure),
            fan_speed_rpm: ScalarObservation::unavailable(failure),
            fan_speed_pct: ScalarObservation::unavailable(failure),
            power_w: ScalarObservation::unavailable(failure),
            idle_residency_pct: ScalarObservation::unavailable(failure),
        }
    }
}

/// Detailed provider-neutral GPU telemetry.
///
/// Live scalars and throttle state are private canonical observations. Static
/// identity, per-engine facts, failures and provenance remain independent
/// typed facts; schema-v1 scalar mirrors exist only in `GpuMetricsWire`.
#[derive(Debug, Clone, Default)]
pub struct GpuMetrics {
    /// Stable native device identity. A hardware address or vendor UUID is
    /// preferred over an attachment-scoped display name.
    pub device_id: String,
    /// Confirmed hot-plug generation for this stable identity. Zero means the
    /// metric has not yet passed through a lifecycle assembler.
    pub device_generation: DeviceGeneration,
    pub device_state: DeviceState,
    /// Provider selected for every field contributed to this merged device.
    /// The list is deterministic and contains at most one entry per field.
    pub provenance: Vec<GpuMetricProvenance>,
    /// Authoritative typed truth for live GPU scalar fields.
    scalar_observations: GpuScalarObservations,
    /// Authoritative availability-bearing throttle fact. `Available([])` is a
    /// confirmed absence of active limiters; `Unavailable` is not the same as
    /// an idle/unthrottled device.
    throttle_observation: ScalarObservation<Vec<GpuThrottleReason>>,
    pub brand: String,
    /// Marketing/product name resolved from a native identity database. This
    /// remains separate from `brand`, which is a stable vendor/driver label.
    pub marketing_name: Option<String>,
    /// Raw PCI identity when the GPU is attached to a PCI function.
    pub pci_vendor_id: Option<u16>,
    pub pci_device_id: Option<u16>,
    pub pci_subsystem_vendor_id: Option<u16>,
    pub pci_subsystem_device_id: Option<u16>,
    pub pci_slot: Option<String>,
    pub pci_modalias: Option<String>,
    /// Per-engine utilization. Empty means no provider supplied engine facts.
    pub engines: Vec<GpuEngine>,
    /// Typed failure receipt for the current engine capability. `None` with an
    /// empty engine list means no provider made an engine claim; a failure
    /// preserves `PermissionDenied`, `RequiresEscalation`, or `Unsupported`
    /// instead of making the absence look like a measured zero.
    pub engine_failure: Option<FailureKind>,
    /// Provider that emitted `engine_failure`, when the failure is attributable
    /// to one provider after deterministic merge.
    pub engine_provider: Option<ProviderId>,
    /// Native driver or runtime implementation name, when exposed.
    pub driver: Option<String>,
    /// Native driver version string (e.g. `32.0.101.8974`, NVML `566.36`),
    /// when a provider proves it. A driver name alone never implies a
    /// version; absence stays `None` instead of a guessed release.
    pub driver_version: Option<String>,
    /// Optional runtime graphics API capabilities bound to this GPU.
    pub graphics_api: Option<GpuGraphicsApi>,
}

impl GpuMetrics {
    #[must_use]
    pub fn new(device_id: impl Into<String>, brand: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            brand: brand.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn scalar_observations(&self) -> &GpuScalarObservations {
        &self.scalar_observations
    }

    #[must_use]
    pub const fn throttle_observation(&self) -> &ScalarObservation<Vec<GpuThrottleReason>> {
        &self.throttle_observation
    }

    #[must_use]
    pub fn from_observations(observations: GpuScalarObservations) -> Self {
        Self {
            scalar_observations: observations,
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn current_utilization_pct(&self) -> Option<f32> {
        self.scalar_observations
            .utilization_pct
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_temperature_c(&self) -> Option<f32> {
        self.scalar_observations
            .temperature_c
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_memory_used_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .memory_used_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_memory_total_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .memory_total_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_dedicated_vram_used_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .dedicated_vram_used_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_dedicated_vram_total_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .dedicated_vram_total_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_shared_vram_used_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .shared_vram_used_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_shared_vram_total_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .shared_vram_total_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_frequency_mhz(&self) -> Option<u64> {
        self.scalar_observations
            .frequency_mhz
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_max_frequency_mhz(&self) -> Option<u64> {
        self.scalar_observations
            .max_frequency_mhz
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_fan_speed_rpm(&self) -> Option<u64> {
        self.scalar_observations
            .fan_speed_rpm
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_fan_speed_pct(&self) -> Option<f32> {
        self.scalar_observations
            .fan_speed_pct
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_power_w(&self) -> Option<f32> {
        self.scalar_observations.power_w.current_value().copied()
    }

    #[must_use]
    pub const fn current_idle_residency_pct(&self) -> Option<f32> {
        self.scalar_observations
            .idle_residency_pct
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_throttle_reasons(&self) -> Option<&[GpuThrottleReason]> {
        self.throttle_observation.current_value().map(Vec::as_slice)
    }

    #[must_use]
    pub fn current_throttle_reason_text(&self) -> Option<String> {
        self.current_throttle_reasons().map(throttle_reason_text)
    }

    /// Replace the complete canonical scalar group atomically.
    pub fn apply_scalar_observations(&mut self, observations: GpuScalarObservations) {
        self.scalar_observations = observations;
    }

    /// Replace the independent availability-bearing throttle fact.
    pub fn apply_throttle_observation(
        &mut self,
        observation: ScalarObservation<Vec<GpuThrottleReason>>,
    ) {
        self.throttle_observation = observation;
    }
}

fn legacy_scalar_projection<T: Clone>(observation: &ScalarObservation<T>) -> Option<T> {
    (observation.availability() == ScalarAvailability::Available)
        .then(|| observation.current_value().cloned())
        .flatten()
}

fn hydrate_legacy_scalar<T>(
    observation: ScalarObservation<T>,
    legacy: Option<T>,
    observed_at_ms: u64,
) -> ScalarObservation<T> {
    if observation.availability() == ScalarAvailability::Unknown
        && let Some(value) = legacy
    {
        ScalarObservation::available(value, observed_at_ms)
    } else {
        observation
    }
}

#[must_use]
pub const fn gpu_throttle_reason_label(reason: GpuThrottleReason) -> &'static str {
    match reason {
        GpuThrottleReason::Idle => "idle",
        GpuThrottleReason::ApplicationClockLimit => "application clock limit",
        GpuThrottleReason::SoftwarePowerLimit => "software power limit",
        GpuThrottleReason::HardwareSlowdown => "hardware slowdown",
        GpuThrottleReason::ReliabilityLimit => "reliability limit",
        GpuThrottleReason::SyncBoost => "sync boost",
        GpuThrottleReason::SoftwareThermalLimit => "software thermal limit",
        GpuThrottleReason::HardwareThermalLimit => "hardware thermal limit",
        GpuThrottleReason::ExternalPowerBrake => "external power brake",
        GpuThrottleReason::DisplayClockLimit => "display clock limit",
        GpuThrottleReason::Other => "other",
    }
}

fn throttle_reason_text(reasons: &[GpuThrottleReason]) -> String {
    reasons
        .iter()
        .map(|reason| gpu_throttle_reason_label(*reason))
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_throttle_text(value: &str) -> Vec<GpuThrottleReason> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "idle" => GpuThrottleReason::Idle,
            "application clock limit" => GpuThrottleReason::ApplicationClockLimit,
            "software power limit" => GpuThrottleReason::SoftwarePowerLimit,
            "hardware slowdown" => GpuThrottleReason::HardwareSlowdown,
            "reliability limit" => GpuThrottleReason::ReliabilityLimit,
            "sync boost" => GpuThrottleReason::SyncBoost,
            "software thermal limit" => GpuThrottleReason::SoftwareThermalLimit,
            "hardware thermal limit" => GpuThrottleReason::HardwareThermalLimit,
            "external power brake" => GpuThrottleReason::ExternalPowerBrake,
            "display clock limit" => GpuThrottleReason::DisplayClockLimit,
            "other" => GpuThrottleReason::Other,
            _ => GpuThrottleReason::Other,
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_gpu_tests.rs"]
mod tests;

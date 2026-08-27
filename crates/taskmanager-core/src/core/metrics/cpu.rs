//! CPU telemetry metrics: global and per-core utilization, frequency,
//! temperature, and power scalar observations with availability, plus the
//! active performance-policy projection and schema-v1 compatibility fields.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::availability::hydrate_legacy_group;
use super::{ScalarAvailability, ScalarObservation, ScalarObservationGroup};
use crate::core::FailureKind;

/// Hard ceiling for per-logical-CPU derived histories and projections.
///
/// The authoritative [`CpuMetrics`] snapshot is not truncated: this bound
/// prevents a malformed provider-sized vector from being multiplied into
/// several long-lived rolling histories. It is deliberately far above current
/// workstation and server topology while keeping memory use provably finite.
pub const MAX_TRACKED_LOGICAL_CPUS: usize = 4_096;

/// Identifies how the live CPU frequency readout was obtained.
///
/// BogoMIPS is a Linux boot-time calibration value, not a normal clock
/// measurement. It is retained as an explicit source marker so a fallback can
/// remain useful without being presented as native frequency truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CpuFrequencySource {
    #[default]
    Native,
    BogoMips,
}

impl CpuFrequencySource {
    #[must_use]
    pub const fn is_native(&self) -> bool {
        matches!(self, Self::Native)
    }

    #[must_use]
    pub const fn is_bogomips(self) -> bool {
        matches!(self, Self::BogoMips)
    }
}

/// Identifies which provider produced the live CPU package-temperature
/// readout.
///
/// `Coretemp`, `K10temp`, and `Zenpower` are the dedicated CPU sensor chip
/// drivers and count as native temperature truth. [`Self::PackageHwmon`] is
/// a labeled fallback: a temperature channel on any *other* hwmon chip whose
/// effective label carries CPU-package semantics (`Tctl` / `Tdie` /
/// `Package` / `APU` / `CPU`) — the Steam-Deck-class hosts whose package
/// temperature lives outside coretemp/k10temp. [`Self::ThermalZone`] is the
/// ACPI thermal-zone last resort. The two fallback tiers stay explicit on
/// the wire and carry a visible UI qualifier so a derived reading never
/// masquerades as a dedicated CPU sensor chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CpuTemperatureSource {
    #[default]
    Coretemp,
    K10temp,
    Zenpower,
    PackageHwmon,
    ThermalZone,
}

impl CpuTemperatureSource {
    /// True for the default chip tier. Used as the wire omission predicate:
    /// snapshots from the default dedicated-chip path stay byte-compatible
    /// with payloads written before typed temperature provenance existed.
    #[must_use]
    pub const fn is_coretemp(&self) -> bool {
        matches!(self, Self::Coretemp)
    }

    /// True for the fallback tiers that must carry a visible UI qualifier
    /// (a CPU-package-labeled channel on another hwmon chip, or an ACPI
    /// thermal zone).
    #[must_use]
    pub const fn is_labeled_fallback(&self) -> bool {
        matches!(self, Self::PackageHwmon | Self::ThermalZone)
    }
}

/// Width of the saturation band above 100% still accepted from a provider
/// CPU-usage percentage. sysinfo's tick arithmetic can round a fully busy
/// core to a hair above 100; anything at `100.0 + this` or beyond is a
/// phantom value.
const CPU_USAGE_PCT_TOLERANCE: f32 = 0.5;

/// Gate one raw provider CPU-usage percentage into a typed observation.
///
/// Idle-phantom-spike guard (the Mission Center v1.2.0 !484 failure class):
/// a provider can surface NaN, negative, or impossible `> 100%` percentages
/// while the host is actually idle. Such a sample becomes a typed gap,
/// never a pass-through — a single phantom percentage would otherwise
/// render as a full-scale spike in graphs and poison rolling histories.
/// Percentages inside `(100.0, 100.0 + tolerance)` are saturation rounding
/// and clamp to 100.0; measured zeros stay real zeros. This gate validates
/// only the value itself: zero-window and rollback discipline for counter
/// deltas belongs to the counter-delta layer.
pub fn cpu_usage_pct_observation(usage_pct: f32, observed_at_ms: u64) -> ScalarObservation<f32> {
    if !usage_pct.is_finite() || !(0.0..100.0 + CPU_USAGE_PCT_TOLERANCE).contains(&usage_pct) {
        return ScalarObservation::unavailable(FailureKind::ProviderFault);
    }
    ScalarObservation::available(usage_pct.min(100.0), observed_at_ms)
}

/// Independently fallible live CPU measurements.
///
/// Schema-v1 and schema-v2 compatibility values exist only in private wire
/// DTOs. Consumers use the typed accessors so a missing provider cannot look
/// like a measured zero and a retained value cannot look current.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CpuScalarObservations {
    pub global_usage_pct: ScalarObservation<f32>,
    pub core_usage_group: ScalarObservationGroup<f32>,
    pub frequency_mhz: ScalarObservation<u64>,
    pub max_frequency_mhz: ScalarObservation<u64>,
    pub per_core_frequency_group: ScalarObservationGroup<u64>,
    pub temperature_c: ScalarObservation<f32>,
    pub per_core_temperature_group: ScalarObservationGroup<f32>,
    pub power_w: ScalarObservation<f32>,
}

impl CpuScalarObservations {
    /// Retain prior successful values only as stale when an individual field
    /// fails. Vector slots are matched by the native adapter's stable logical
    /// index for the lifetime of one device generation.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        let core_usage_group = self
            .core_usage_group
            .retain_previous(previous.core_usage_group);
        let per_core_frequency_group = self
            .per_core_frequency_group
            .retain_previous(previous.per_core_frequency_group);
        let per_core_temperature_group = self
            .per_core_temperature_group
            .retain_previous(previous.per_core_temperature_group);
        Self {
            global_usage_pct: self
                .global_usage_pct
                .retain_previous(previous.global_usage_pct),
            core_usage_group,
            frequency_mhz: self.frequency_mhz.retain_previous(previous.frequency_mhz),
            max_frequency_mhz: self
                .max_frequency_mhz
                .retain_previous(previous.max_frequency_mhz),
            per_core_frequency_group,
            temperature_c: self.temperature_c.retain_previous(previous.temperature_c),
            per_core_temperature_group,
            power_w: self.power_w.retain_previous(previous.power_w),
        }
    }

    #[must_use]
    pub fn unavailable(failure: FailureKind) -> Self {
        Self {
            global_usage_pct: ScalarObservation::unavailable(failure),
            core_usage_group: ScalarObservationGroup::unavailable(failure),
            frequency_mhz: ScalarObservation::unavailable(failure),
            max_frequency_mhz: ScalarObservation::unavailable(failure),
            per_core_frequency_group: ScalarObservationGroup::unavailable(failure),
            temperature_c: ScalarObservation::unavailable(failure),
            per_core_temperature_group: ScalarObservationGroup::unavailable(failure),
            power_w: ScalarObservation::unavailable(failure),
        }
    }
}

/// Compatibility-only shape for the three schema-v2 per-core item vectors.
/// Canonical code owns only the typed groups.
#[derive(Serialize, Deserialize, Default)]
struct CpuScalarObservationsWire {
    #[serde(default)]
    global_usage_pct: ScalarObservation<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    core_usage_pct: Option<Vec<ScalarObservation<f32>>>,
    #[serde(default)]
    core_usage_group: ScalarObservationGroup<f32>,
    #[serde(default)]
    frequency_mhz: ScalarObservation<u64>,
    #[serde(default)]
    max_frequency_mhz: ScalarObservation<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    per_core_frequency_mhz: Option<Vec<ScalarObservation<u64>>>,
    #[serde(default)]
    per_core_frequency_group: ScalarObservationGroup<u64>,
    #[serde(default)]
    temperature_c: ScalarObservation<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    per_core_temperature_c: Option<Vec<ScalarObservation<f32>>>,
    #[serde(default)]
    per_core_temperature_group: ScalarObservationGroup<f32>,
    #[serde(default)]
    power_w: ScalarObservation<f32>,
}

impl Serialize for CpuScalarObservations {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CpuScalarObservationsWire {
            global_usage_pct: self.global_usage_pct,
            core_usage_pct: legacy_observation_group_projection(&self.core_usage_group),
            core_usage_group: self.core_usage_group.clone(),
            frequency_mhz: self.frequency_mhz,
            max_frequency_mhz: self.max_frequency_mhz,
            per_core_frequency_mhz: legacy_observation_group_projection(
                &self.per_core_frequency_group,
            ),
            per_core_frequency_group: self.per_core_frequency_group.clone(),
            temperature_c: self.temperature_c,
            per_core_temperature_c: legacy_observation_group_projection(
                &self.per_core_temperature_group,
            ),
            per_core_temperature_group: self.per_core_temperature_group.clone(),
            power_w: self.power_w,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CpuScalarObservations {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CpuScalarObservationsWire::deserialize(deserializer)?;
        Ok(Self {
            global_usage_pct: wire.global_usage_pct,
            core_usage_group: hydrate_legacy_group(
                wire.core_usage_group,
                wire.core_usage_pct.unwrap_or_default(),
            ),
            frequency_mhz: wire.frequency_mhz,
            max_frequency_mhz: wire.max_frequency_mhz,
            per_core_frequency_group: hydrate_legacy_group(
                wire.per_core_frequency_group,
                wire.per_core_frequency_mhz.unwrap_or_default(),
            ),
            temperature_c: wire.temperature_c,
            per_core_temperature_group: hydrate_legacy_group(
                wire.per_core_temperature_group,
                wire.per_core_temperature_c.unwrap_or_default(),
            ),
            power_w: wire.power_w,
        })
    }
}

/// Platform-neutral description of the CPU's active performance policy.
///
/// Native adapters map their own vocabulary into these semantic slots. For
/// example, Linux cpufreq supplies its scaling driver, governor, and
/// energy-performance preference without making those Linux names part of the
/// Rust API consumed by other platforms.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CpuPerformancePolicy {
    /// Native implementation responsible for CPU frequency/performance control.
    #[serde(default, rename = "cpufreq_driver", alias = "frequency_implementation")]
    pub frequency_implementation: Option<String>,
    /// Currently selected native performance policy.
    #[serde(default, rename = "cpufreq_governor", alias = "active_policy")]
    pub active_policy: Option<String>,
    /// Active native energy-versus-performance preference, when exposed.
    #[serde(default, rename = "power_preference", alias = "energy_preference")]
    pub energy_preference: Option<String>,
}

/// Global CPU Metrics & Cache Info
#[derive(Debug, Clone, Default)]
pub struct CpuMetrics {
    /// Authoritative typed truth for independently fallible live CPU scalars.
    scalar_observations: CpuScalarObservations,
    /// Provider-reported processor identity. `None` means no provider supplied
    /// a non-empty value; frontends must not invent an "unknown CPU" model.
    pub brand: Option<String>,
    /// Source of the current frequency readout. Native is omitted from JSON for
    /// compatibility; BogoMIPS is explicit because it needs a visible UI
    /// qualifier and must never masquerade as a native clock measurement.
    pub frequency_source: CpuFrequencySource,
    /// Source of the current package-temperature readout. The default chip
    /// (`Coretemp`) is omitted from JSON for compatibility; every other tier —
    /// including the other native chips — is explicit so the provenance of a
    /// CPU temperature claim survives a wire round-trip, and the labeled
    /// fallback tiers drive a visible UI qualifier.
    pub temperature_source: CpuTemperatureSource,
    /// Physical-core topology, when the selected native provider exposes it.
    pub physical_cores: Option<usize>,
    /// Logical-processor topology, distinct from the number of utilization
    /// samples that happened to arrive during this tick.
    pub logical_cores: Option<usize>,
    /// Aggregate cache capacities in KiB. Missing topology is not encoded as
    /// zero; an observed zero would remain `Some(0)`.
    pub l1_cache_kb: Option<u64>,
    pub l2_cache_kb: Option<u64>,
    pub l3_cache_kb: Option<u64>,
    /// Active CPU performance-policy metadata from the selected native adapter.
    ///
    /// Flattening preserves the legacy top-level JSON keys while keeping the
    /// Rust model free of Linux cpufreq terminology.
    pub performance_policy: CpuPerformancePolicy,
}

/// Compatibility-only outer CPU shape. Live values are always projected from
/// canonical typed observations; these fields never exist in `CpuMetrics`.
#[derive(Serialize, Deserialize, Default)]
struct CpuMetricsWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    global_usage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    core_usages: Option<Vec<f32>>,
    #[serde(default)]
    scalar_observations: CpuScalarObservations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    frequency_mhz: Option<u64>,
    #[serde(default, skip_serializing_if = "CpuFrequencySource::is_native")]
    frequency_source: CpuFrequencySource,
    #[serde(default, skip_serializing_if = "CpuTemperatureSource::is_coretemp")]
    temperature_source: CpuTemperatureSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_freq_mhz: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    per_core_freq_mhz: Option<Vec<Option<u64>>>,
    #[serde(default)]
    physical_cores: Option<usize>,
    #[serde(default)]
    logical_cores: Option<usize>,
    #[serde(default)]
    l1_cache_kb: Option<u64>,
    #[serde(default)]
    l2_cache_kb: Option<u64>,
    #[serde(default)]
    l3_cache_kb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    temperature_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    per_core_temps_c: Option<Vec<f32>>,
    #[serde(default, flatten)]
    performance_policy: CpuPerformancePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpu_power_w: Option<f32>,
}

impl Serialize for CpuMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CpuMetricsWire {
            global_usage: legacy_scalar_projection(&self.scalar_observations.global_usage_pct),
            core_usages: legacy_complete_group_projection(
                &self.scalar_observations.core_usage_group,
            ),
            scalar_observations: self.scalar_observations.clone(),
            brand: self.brand.clone(),
            frequency_mhz: legacy_scalar_projection(&self.scalar_observations.frequency_mhz),
            frequency_source: self.frequency_source,
            temperature_source: self.temperature_source,
            max_freq_mhz: legacy_scalar_projection(&self.scalar_observations.max_frequency_mhz),
            per_core_freq_mhz: legacy_optional_group_projection(
                &self.scalar_observations.per_core_frequency_group,
            ),
            physical_cores: self.physical_cores,
            logical_cores: self.logical_cores,
            l1_cache_kb: self.l1_cache_kb,
            l2_cache_kb: self.l2_cache_kb,
            l3_cache_kb: self.l3_cache_kb,
            temperature_c: legacy_scalar_projection(&self.scalar_observations.temperature_c),
            per_core_temps_c: legacy_complete_group_projection(
                &self.scalar_observations.per_core_temperature_group,
            ),
            performance_policy: self.performance_policy.clone(),
            cpu_power_w: legacy_scalar_projection(&self.scalar_observations.power_w),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CpuMetrics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CpuMetricsWire::deserialize(deserializer)?;
        let trustworthy_identity = wire
            .brand
            .as_deref()
            .is_some_and(|brand| !brand.trim().is_empty())
            || wire.physical_cores.is_some_and(|count| count > 0)
            || wire.logical_cores.is_some_and(|count| count > 0)
            || wire
                .core_usages
                .as_ref()
                .is_some_and(|values| !values.is_empty())
            || wire
                .per_core_freq_mhz
                .as_ref()
                .is_some_and(|values| !values.is_empty())
            || wire
                .per_core_temps_c
                .as_ref()
                .is_some_and(|values| !values.is_empty());
        let mut observations = wire.scalar_observations;
        observations.global_usage_pct = hydrate_legacy_scalar(
            observations.global_usage_pct,
            trustworthy_identity.then_some(wire.global_usage).flatten(),
        );
        observations.frequency_mhz = hydrate_legacy_scalar(
            observations.frequency_mhz,
            trustworthy_identity.then_some(wire.frequency_mhz).flatten(),
        );
        observations.max_frequency_mhz = hydrate_legacy_scalar(
            observations.max_frequency_mhz,
            trustworthy_identity.then_some(wire.max_freq_mhz).flatten(),
        );
        observations.temperature_c = hydrate_legacy_scalar(
            observations.temperature_c,
            trustworthy_identity.then_some(wire.temperature_c).flatten(),
        );
        observations.power_w = hydrate_legacy_scalar(
            observations.power_w,
            trustworthy_identity.then_some(wire.cpu_power_w).flatten(),
        );
        if trustworthy_identity {
            observations.core_usage_group = hydrate_outer_group(
                observations.core_usage_group,
                wire.core_usages
                    .unwrap_or_default()
                    .into_iter()
                    .map(|value| ScalarObservation::available(value, 0))
                    .collect(),
            );
            observations.per_core_frequency_group = hydrate_outer_optional_group(
                observations.per_core_frequency_group,
                wire.per_core_freq_mhz.unwrap_or_default(),
            );
            observations.per_core_temperature_group = hydrate_outer_group(
                observations.per_core_temperature_group,
                wire.per_core_temps_c
                    .unwrap_or_default()
                    .into_iter()
                    .map(|value| ScalarObservation::available(value, 0))
                    .collect(),
            );
        }
        Ok(Self {
            scalar_observations: observations,
            brand: wire.brand,
            frequency_source: wire.frequency_source,
            temperature_source: wire.temperature_source,
            physical_cores: wire.physical_cores,
            logical_cores: wire.logical_cores,
            l1_cache_kb: wire.l1_cache_kb,
            l2_cache_kb: wire.l2_cache_kb,
            l3_cache_kb: wire.l3_cache_kb,
            performance_policy: wire.performance_policy,
        })
    }
}

impl CpuMetrics {
    #[must_use]
    pub const fn scalar_observations(&self) -> &CpuScalarObservations {
        &self.scalar_observations
    }

    #[must_use]
    pub fn from_observations(observations: CpuScalarObservations) -> Self {
        Self {
            scalar_observations: observations,
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn current_global_usage_pct(&self) -> Option<f32> {
        self.scalar_observations
            .global_usage_pct
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_core_usage_pct(&self, index: usize) -> Option<f32> {
        self.scalar_observations
            .core_usage_group
            .current_observations()?
            .get(index)
            .and_then(ScalarObservation::current_value)
            .copied()
    }

    #[must_use]
    pub fn current_core_usage_len(&self) -> usize {
        self.scalar_observations
            .core_usage_group
            .current_observations()
            .map_or(0, <[ScalarObservation<f32>]>::len)
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
    pub fn current_core_frequency_mhz(&self, index: usize) -> Option<u64> {
        self.scalar_observations
            .per_core_frequency_group
            .current_observations()?
            .get(index)
            .and_then(ScalarObservation::current_value)
            .copied()
    }

    /// Number of logical-core frequency slots in the current typed group.
    /// Failed, stale, and unavailable groups contribute no current slots.
    #[must_use]
    pub fn current_core_frequency_len(&self) -> usize {
        self.scalar_observations
            .per_core_frequency_group
            .current_observations()
            .map_or(0, <[ScalarObservation<u64>]>::len)
    }

    #[must_use]
    pub const fn current_temperature_c(&self) -> Option<f32> {
        self.scalar_observations
            .temperature_c
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_core_temperature_c(&self, index: usize) -> Option<f32> {
        self.scalar_observations
            .per_core_temperature_group
            .current_observations()?
            .get(index)
            .and_then(ScalarObservation::current_value)
            .copied()
    }

    #[must_use]
    pub fn current_core_temperature_len(&self) -> usize {
        self.scalar_observations
            .per_core_temperature_group
            .current_observations()
            .map_or(0, <[ScalarObservation<f32>]>::len)
    }

    #[must_use]
    pub const fn current_power_w(&self) -> Option<f32> {
        self.scalar_observations.power_w.current_value().copied()
    }

    /// Replace canonical live truth in one operation.
    pub fn apply_scalar_observations(&mut self, observations: CpuScalarObservations) {
        self.scalar_observations = observations;
    }

    pub fn retain_previous_observations(&mut self, previous: &Self) {
        self.scalar_observations = self
            .scalar_observations
            .clone()
            .retain_previous(previous.scalar_observations.clone());
    }
}

const fn legacy_scalar_projection<T: Copy>(observation: &ScalarObservation<T>) -> Option<T> {
    if matches!(observation.availability(), ScalarAvailability::Available) {
        observation.current_value().copied()
    } else {
        None
    }
}

fn legacy_complete_group_projection<T: Copy>(group: &ScalarObservationGroup<T>) -> Option<Vec<T>> {
    if !matches!(group.availability(), ScalarAvailability::Available) {
        return None;
    }
    Some(
        group
            .last_known_observations()
            .iter()
            .filter_map(legacy_scalar_projection)
            .collect(),
    )
}

fn legacy_optional_group_projection<T: Copy>(
    group: &ScalarObservationGroup<T>,
) -> Option<Vec<Option<T>>> {
    if !matches!(group.availability(), ScalarAvailability::Available) {
        return None;
    }
    Some(
        group
            .last_known_observations()
            .iter()
            .map(legacy_scalar_projection)
            .collect(),
    )
}

fn legacy_observation_group_projection<T: Clone>(
    group: &ScalarObservationGroup<T>,
) -> Option<Vec<ScalarObservation<T>>> {
    matches!(group.availability(), ScalarAvailability::Available)
        .then(|| group.last_known_observations().to_vec())
}

fn hydrate_legacy_scalar<T: Copy>(
    observation: ScalarObservation<T>,
    legacy: Option<T>,
) -> ScalarObservation<T> {
    if matches!(observation.availability(), ScalarAvailability::Unknown)
        && let Some(value) = legacy
    {
        return ScalarObservation::available(value, 0);
    }
    observation
}

fn hydrate_outer_group<T>(
    group: ScalarObservationGroup<T>,
    legacy_items: Vec<ScalarObservation<T>>,
) -> ScalarObservationGroup<T> {
    hydrate_legacy_group(group, legacy_items)
}

fn hydrate_outer_optional_group<T>(
    group: ScalarObservationGroup<T>,
    legacy: Vec<Option<T>>,
) -> ScalarObservationGroup<T> {
    hydrate_legacy_group(
        group,
        legacy
            .into_iter()
            .map(|value| {
                value.map_or_else(
                    || ScalarObservation::unavailable(FailureKind::Unsupported),
                    |value| ScalarObservation::available(value, 0),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_cpu_tests.rs"]
mod tests;

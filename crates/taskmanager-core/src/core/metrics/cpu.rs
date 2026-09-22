//! CPU telemetry metrics: global and per-core utilization, frequency,
//! temperature, and power scalar observations with availability, plus the
//! active performance-policy projection and schema-v1 compatibility fields.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::availability::hydrate_legacy_group;
use super::{ScalarAvailability, ScalarObservation, ScalarObservationGroup};
use crate::core::FailureKind;

mod legacy;

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
            core_usage_pct: legacy::observation_group_projection(&self.core_usage_group),
            core_usage_group: self.core_usage_group.clone(),
            frequency_mhz: self.frequency_mhz,
            max_frequency_mhz: self.max_frequency_mhz,
            per_core_frequency_mhz: legacy::observation_group_projection(
                &self.per_core_frequency_group,
            ),
            per_core_frequency_group: self.per_core_frequency_group.clone(),
            temperature_c: self.temperature_c,
            per_core_temperature_c: legacy::observation_group_projection(
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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
    /// Whether the native boost/turbo policy is currently enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boost_enabled: Option<bool>,
    /// Highest per-CPU turbo-cap frequency observed from the native policy
    /// files, in MHz. This is separate from the live current clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boost_max_frequency_mhz: Option<u64>,
    /// Long-term package power limit (PL1), in watts, when the native power
    /// controller exposes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_limit_1_w: Option<f32>,
    /// Short-term package power limit (PL2), in watts, when exposed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_limit_2_w: Option<f32>,
    /// The controller's observed PL1 time window (Tau), in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_time_window_ms: Option<u64>,
}

/// Telemetry and topology metrics for an individual CPU package (physical socket),
/// including thermal throttling indicators and NUMA topology awareness.
///
/// Multi-socket server configurations, multi-die chiplet packages, and NUMA nodes
/// partition hardware resources (cores, caches, and memory channels) into distinct
/// domains. This struct models per-package telemetry with explicit differentiation
/// between normal unthrottled operation, active thermal throttling trips, and
/// unobserved or unsupported capabilities (following ADR-016 scalar authenticity).
///
/// This type is the single authority for the cumulative trigger counters: the
/// `power.thermal-throttle-events` delivery names them, and the
/// `telemetry.cpu.throttle` lane answer is a projection of the same rows produced
/// by
/// [`aggregate_package_throttle_counters`](super::aggregate_package_throttle_counters).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CpuPackageMetrics {
    /// Zero-based physical socket or package index (e.g. 0 for Socket 0).
    pub package_id: u32,
    /// Primary NUMA memory node index associated with this physical socket, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numa_node_id: Option<u32>,
    /// All NUMA node IDs associated with this physical package.
    ///
    /// Multi-NUMA-per-socket architectures (e.g. Sub-NUMA Clustering / SNC on Intel Xeon
    /// or NUMA nodes per socket / NPS on AMD EPYC) partition a single physical socket
    /// into multiple memory nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub numa_node_ids: Vec<u32>,
    /// Logical core IDs belonging to this physical package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logical_core_ids: Vec<usize>,
    /// Kernel-reported die/cluster identifiers associated with this package.
    /// On AMD this is the closest safe generic representation of CCD/CCX
    /// topology; an empty vector means the kernel did not expose a chiplet
    /// identifier, not that the package has zero chiplets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chiplet_ids: Vec<u32>,
    /// Maximum number of logical threads in one physical-core sibling group.
    /// `Some(2)` is the common SMT case; `None` means sibling topology was not
    /// readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smt_threads_per_core: Option<usize>,
    /// Stable logical-CPU groups that share one physical execution core.
    ///
    /// Each inner vector is a kernel-reported `thread_siblings_list`, sorted
    /// and de-duplicated by the native adapter. An empty vector means the
    /// kernel did not expose pairing; it is not a claim that no SMT exists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub smt_sibling_groups: Vec<Vec<usize>>,
    /// Physical execution core count present in this package, distinct from logical threads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_core_count: Option<usize>,
    /// Total local physical memory capacity attached to this package's NUMA node(s), in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_memory_bytes: Option<u64>,
    /// Local NUMA memory allocation hit ratio percentage in the range `0.0..=100.0`.
    ///
    /// Derived from platform numastat (`numa_hit / (numa_hit + numa_miss + numa_foreign)`).
    /// `None` when NUMA telemetry is unavailable or unexposed by the host kernel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numa_hit_ratio_pct: Option<f32>,
    /// Real-time thermal throttling indicator for this CPU package.
    ///
    /// `Some(true)` denotes that the package is currently actively throttled due to thermal
    /// limits (e.g. PROCHOT assertion or digital thermal sensor trip status). `Some(false)`
    /// denotes verified unthrottled operation. `None` indicates the provider cannot observe
    /// real-time throttle status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_throttled: Option<bool>,
    /// Cumulative count of package-level thermal throttling events since boot or counter reset.
    ///
    /// Sourced from Linux sysfs `/sys/devices/system/cpu/cpu*/thermal_throttle/package_throttle_count`
    /// or x86 MSR `IA32_PACKAGE_THERM_STATUS` (0x1B1).
    ///
    /// This field is the delivery authority for the `power.thermal-throttle-events`
    /// fact; the `telemetry.cpu.throttle` lane answer is the same per-package
    /// fact projected by
    /// [`aggregate_package_throttle_counters`](super::aggregate_package_throttle_counters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_throttle_count: Option<u64>,
    /// Cumulative count of core-level thermal throttling events across cores within this package.
    ///
    /// Sourced from Linux sysfs `/sys/devices/system/cpu/cpu*/thermal_throttle/core_throttle_count`
    /// or x86 MSR `IA32_THERM_STATUS` (0x19C). Sibling hyperthreads of one
    /// physical core report the same kernel counter and are counted once; the
    /// value is `None` when no core counter in this package was readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_throttle_count: Option<u64>,
    /// Thermal headroom in degrees Celsius below the critical maximum junction temperature ($T_j\text{Max}$).
    ///
    /// Positive values denote available thermal margin before throttling occurs. Values
    /// at or below zero denote active thermal saturation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_margin_c: Option<f32>,
    /// Package temperature in degrees Celsius, when reported by a dedicated sensor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f32>,
    /// Package power consumption in watts (e.g. derived from RAPL `energy_uj` differential counters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_w: Option<f32>,
    /// Average or representative clock frequency in MHz across cores in this package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_mhz: Option<u64>,
}

impl CpuPackageMetrics {
    /// Create a new package metrics descriptor for a physical socket index.
    #[must_use]
    pub const fn new(package_id: u32) -> Self {
        Self {
            package_id,
            numa_node_id: None,
            numa_node_ids: Vec::new(),
            logical_core_ids: Vec::new(),
            chiplet_ids: Vec::new(),
            smt_threads_per_core: None,
            smt_sibling_groups: Vec::new(),
            physical_core_count: None,
            local_memory_bytes: None,
            numa_hit_ratio_pct: None,
            is_throttled: None,
            package_throttle_count: None,
            core_throttle_count: None,
            thermal_margin_c: None,
            temperature_c: None,
            power_w: None,
            frequency_mhz: None,
        }
    }

    /// True if this package is actively thermal throttling.
    #[must_use]
    pub const fn is_currently_throttled(&self) -> bool {
        matches!(self.is_throttled, Some(true))
    }

    /// Whether this package contains or is associated with the given NUMA node.
    #[must_use]
    pub fn contains_numa_node(&self, node_id: u32) -> bool {
        self.numa_node_id == Some(node_id) || self.numa_node_ids.contains(&node_id)
    }

    /// Whether this package contains the given logical CPU processor index.
    #[must_use]
    pub fn contains_logical_core(&self, core_id: usize) -> bool {
        self.logical_core_ids.contains(&core_id)
    }
}

/// One Linux cpuidle state aggregated from the kernel's cumulative counters.
///
/// The counters are intentionally kept as cumulative microseconds and entry
/// counts. A frontend can derive a residency percentage only when it has two
/// samples from the same CPU generation; it must not turn one cumulative
/// counter into a percentage by guessing a denominator.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CpuIdleState {
    /// Stable kernel state name such as `POLL`, `C1`, or `C6`.
    pub name: String,
    /// Human-readable kernel description, when exposed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Cumulative time spent in this state, in microseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residency_us: Option<u64>,
    /// Percentage of the measured interval spent in this state. This is only
    /// populated after two samples with the same state identity; a first
    /// sample or counter rollback remains `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residency_pct: Option<f32>,
    /// Cumulative number of entries into this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_count: Option<u64>,
    /// Exit latency advertised by the kernel, in microseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_us: Option<u64>,
    /// Whether the state is disabled by the kernel policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

impl CpuIdleState {
    /// Derive a bounded residency percentage from two monotonic samples.
    ///
    /// The denominator is wall time, not a guessed boot-time value. A missing
    /// counter, zero interval, reset, or impossible counter delta returns
    /// `None` so a provider cannot turn an invalid sample into `0%`.
    #[must_use]
    pub fn residency_percentage_since(&self, previous: &Self, elapsed_ms: u64) -> Option<f32> {
        let current = self.residency_us?;
        let before = previous.residency_us?;
        if elapsed_ms == 0 || current < before {
            return None;
        }
        let elapsed_us = elapsed_ms.checked_mul(1_000)?;
        let delta = current - before;
        if delta > elapsed_us {
            return None;
        }
        Some((delta as f64 / elapsed_us as f64 * 100.0) as f32)
    }
}

/// Cumulative interrupt counts grouped by logical processor. These are raw
/// kernel counters, not rates; a rate requires two observations and belongs
/// to the history layer. A missing vector means the source was unavailable or
/// exposed no parseable CPU columns.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CpuInterruptSnapshot {
    pub total: Option<u64>,
    pub per_logical_cpu: Vec<u64>,
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
    /// Aggregate cache capacities in KiB. L1 is split by kind — data and
    /// instruction — because x86 L1 is always split and a unified sum would
    /// hide which side is missing. A unified L1 (rare, non-x86) reports into
    /// the data slot. Missing topology is not encoded as zero; an observed
    /// zero would remain `Some(0)`.
    pub l1d_cache_kb: Option<u64>,
    pub l1i_cache_kb: Option<u64>,
    pub l2_cache_kb: Option<u64>,
    pub l3_cache_kb: Option<u64>,
    /// Active CPU performance-policy metadata from the selected native adapter.
    ///
    /// Flattening preserves the legacy top-level JSON keys while keeping the
    /// Rust model free of Linux cpufreq terminology.
    pub performance_policy: CpuPerformancePolicy,
    /// Per-package (physical socket) metrics including thermal throttling
    /// indicators and NUMA topology awareness. Empty on single-package hosts
    /// where package topology is not exposed or when package enumeration fails.
    pub packages: Vec<CpuPackageMetrics>,
    /// Cumulative cpuidle state counters from a representative logical CPU.
    /// Empty means cpuidle is unavailable or the provider exposed no states.
    pub idle_states: Vec<CpuIdleState>,
    /// Cumulative interrupt distribution by logical CPU, when exposed.
    pub interrupts: Option<CpuInterruptSnapshot>,
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
    l1d_cache_kb: Option<u64>,
    #[serde(default)]
    l1i_cache_kb: Option<u64>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    packages: Vec<CpuPackageMetrics>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    idle_states: Vec<CpuIdleState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    interrupts: Option<CpuInterruptSnapshot>,
}

impl Serialize for CpuMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CpuMetricsWire {
            global_usage: legacy::scalar_projection(&self.scalar_observations.global_usage_pct),
            core_usages: legacy::complete_group_projection(
                &self.scalar_observations.core_usage_group,
            ),
            scalar_observations: self.scalar_observations.clone(),
            brand: self.brand.clone(),
            frequency_mhz: legacy::scalar_projection(&self.scalar_observations.frequency_mhz),
            frequency_source: self.frequency_source,
            temperature_source: self.temperature_source,
            max_freq_mhz: legacy::scalar_projection(&self.scalar_observations.max_frequency_mhz),
            per_core_freq_mhz: legacy::optional_group_projection(
                &self.scalar_observations.per_core_frequency_group,
            ),
            physical_cores: self.physical_cores,
            logical_cores: self.logical_cores,
            l1d_cache_kb: self.l1d_cache_kb,
            l1i_cache_kb: self.l1i_cache_kb,
            l2_cache_kb: self.l2_cache_kb,
            l3_cache_kb: self.l3_cache_kb,
            temperature_c: legacy::scalar_projection(&self.scalar_observations.temperature_c),
            per_core_temps_c: legacy::complete_group_projection(
                &self.scalar_observations.per_core_temperature_group,
            ),
            performance_policy: self.performance_policy.clone(),
            cpu_power_w: legacy::scalar_projection(&self.scalar_observations.power_w),
            packages: self.packages.clone(),
            idle_states: self.idle_states.clone(),
            interrupts: self.interrupts.clone(),
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
        observations.global_usage_pct = legacy::hydrate_scalar(
            observations.global_usage_pct,
            trustworthy_identity.then_some(wire.global_usage).flatten(),
        );
        observations.frequency_mhz = legacy::hydrate_scalar(
            observations.frequency_mhz,
            trustworthy_identity.then_some(wire.frequency_mhz).flatten(),
        );
        observations.max_frequency_mhz = legacy::hydrate_scalar(
            observations.max_frequency_mhz,
            trustworthy_identity.then_some(wire.max_freq_mhz).flatten(),
        );
        observations.temperature_c = legacy::hydrate_scalar(
            observations.temperature_c,
            trustworthy_identity.then_some(wire.temperature_c).flatten(),
        );
        observations.power_w = legacy::hydrate_scalar(
            observations.power_w,
            trustworthy_identity.then_some(wire.cpu_power_w).flatten(),
        );
        if trustworthy_identity {
            observations.core_usage_group = legacy::hydrate_group(
                observations.core_usage_group,
                wire.core_usages
                    .unwrap_or_default()
                    .into_iter()
                    .map(|value| ScalarObservation::available(value, 0))
                    .collect(),
            );
            observations.per_core_frequency_group = legacy::hydrate_optional_group(
                observations.per_core_frequency_group,
                wire.per_core_freq_mhz.unwrap_or_default(),
            );
            observations.per_core_temperature_group = legacy::hydrate_group(
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
            l1d_cache_kb: wire.l1d_cache_kb,
            l1i_cache_kb: wire.l1i_cache_kb,
            l2_cache_kb: wire.l2_cache_kb,
            l3_cache_kb: wire.l3_cache_kb,
            performance_policy: wire.performance_policy,
            packages: wire.packages,
            idle_states: wire.idle_states,
            interrupts: wire.interrupts,
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

    /// Current core-clock multiplier against the advertised static base
    /// clock (the CPU-Z "base × multiplier" decomposition): current
    /// frequency ÷ base. Guarded — an absent live frequency or a non-positive
    /// base yields `None` rather than a fabricated ratio.
    #[must_use]
    pub fn clock_multiplier(&self, base_frequency_mhz: Option<u64>) -> Option<f32> {
        let base = base_frequency_mhz.filter(|base| *base > 0)?;
        let current = self.current_frequency_mhz()?;
        Some(current as f32 / base as f32)
    }

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

    /// Read-only access to per-package telemetry and topology metrics.
    #[must_use]
    pub fn packages(&self) -> &[CpuPackageMetrics] {
        &self.packages
    }

    /// Find a package's metrics by physical package / socket ID.
    #[must_use]
    pub fn package(&self, package_id: u32) -> Option<&CpuPackageMetrics> {
        self.packages
            .iter()
            .find(|pkg| pkg.package_id == package_id)
    }

    /// Read-only access to cumulative kernel cpuidle state counters.
    #[must_use]
    pub fn idle_states(&self) -> &[CpuIdleState] {
        &self.idle_states
    }

    /// Read-only access to cumulative per-logical-CPU interrupt counters.
    #[must_use]
    pub const fn interrupts(&self) -> Option<&CpuInterruptSnapshot> {
        self.interrupts.as_ref()
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

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_cpu_tests.rs"]
mod tests;

//! Memory telemetry metrics: total/used/available and swap scalar observations
//! with availability, plus optional page-state composition, virtual-memory
//! commitment, and compressed-memory/swap accounting.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{OptionalObservation, ScalarAvailability, ScalarObservation};
use crate::core::FailureKind;

mod optional;
pub use optional::{
    MemoryCompositionObservations, MemoryCompressionObservations, MemoryModuleObservations,
    MemoryOptionalObservations, VirtualMemoryCommitObservations,
};

#[cfg(test)]
#[path = "../../../tests/headless/metrics/memory.rs"]
mod tests;

/// Optional operating-system memory-state composition.
///
/// Each slot is independently optional because native providers do not share
/// one universal page-state taxonomy. A platform should populate only facts
/// whose semantics match these categories. This is the frozen schema-v1
/// flattened wire shape; the ZFS ARC fact is a v2 addition and exists only in
/// the typed `optional_observations.composition` group.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct MemoryComposition {
    /// Memory currently classified as active/recently used.
    #[serde(default)]
    pub active_bytes: Option<u64>,
    /// Memory currently classified as inactive or an early reclaim candidate.
    #[serde(default)]
    pub inactive_bytes: Option<u64>,
    /// Completely free physical memory, distinct from broadly available memory.
    #[serde(default, rename = "mem_free_bytes", alias = "free_bytes")]
    pub free_bytes: Option<u64>,
    /// Provider-classified reclaimable kernel or system memory.
    #[serde(
        default,
        rename = "slab_reclaimable_bytes",
        alias = "reclaimable_bytes"
    )]
    pub reclaimable_bytes: Option<u64>,
}

/// Optional virtual-address commitment accounting.
///
/// This can represent Linux committed address space and the equivalent Windows
/// commit charge/limit without naming either provider in the shared API.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct VirtualMemoryCommit {
    /// Virtual memory for which backing has been committed.
    #[serde(default)]
    pub committed_bytes: Option<u64>,
    /// Current provider-reported commitment ceiling.
    #[serde(default, rename = "commit_limit_bytes", alias = "limit_bytes")]
    pub limit_bytes: Option<u64>,
}

/// Optional compressed-memory and compressed-swap facts.
///
/// Resident compressed memory is deliberately separate from compressed swap:
/// Windows memory compression is not Linux zram, and neither should masquerade
/// as the other in a native adapter.
///
/// This is the frozen schema-v1 flattened wire shape. The zram `mm_stat`
/// depth facts (original/compressed/resident bytes) are v2 additions and
/// exist only in the typed `optional_observations.compression` group.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct MemoryCompression {
    /// Resident memory held in compressed form by the operating system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed_memory_used_bytes: Option<u64>,
    /// Used bytes in a compressed swap device or equivalent store.
    #[serde(
        default,
        rename = "zram_swap_used_bytes",
        alias = "compressed_swap_used_bytes"
    )]
    pub compressed_swap_used_bytes: Option<u64>,
    /// Configured capacity of that compressed swap store.
    #[serde(
        default,
        rename = "zram_total_bytes",
        alias = "compressed_swap_capacity_bytes"
    )]
    pub compressed_swap_capacity_bytes: Option<u64>,
    /// Whether an in-memory compressed cache fronts the swap backing store.
    #[serde(
        default,
        rename = "zswap_enabled",
        alias = "compressed_swap_cache_enabled"
    )]
    pub compressed_swap_cache_enabled: Option<bool>,
}

/// Independently fallible live memory measurements.
///
/// Schema-v1 compatibility values exist only in the private wire DTO. Consumers
/// use the typed accessors so an unavailable source cannot look like a measured
/// zero and a retained value cannot look current.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct MemoryScalarObservations {
    pub total_bytes: ScalarObservation<u64>,
    pub used_bytes: ScalarObservation<u64>,
    pub available_bytes: ScalarObservation<u64>,
    pub swap_total_bytes: ScalarObservation<u64>,
    pub swap_used_bytes: ScalarObservation<u64>,
    pub used_rate_mib_per_sec: ScalarObservation<f32>,
}

impl MemoryScalarObservations {
    /// Retain prior successful values only as stale when a field fails.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        Self {
            total_bytes: self.total_bytes.retain_previous(previous.total_bytes),
            used_bytes: self.used_bytes.retain_previous(previous.used_bytes),
            available_bytes: self
                .available_bytes
                .retain_previous(previous.available_bytes),
            swap_total_bytes: self
                .swap_total_bytes
                .retain_previous(previous.swap_total_bytes),
            swap_used_bytes: self
                .swap_used_bytes
                .retain_previous(previous.swap_used_bytes),
            used_rate_mib_per_sec: self
                .used_rate_mib_per_sec
                .retain_previous(previous.used_rate_mib_per_sec),
        }
    }

    #[must_use]
    pub fn unavailable(failure: FailureKind) -> Self {
        Self {
            total_bytes: ScalarObservation::unavailable(failure),
            used_bytes: ScalarObservation::unavailable(failure),
            available_bytes: ScalarObservation::unavailable(failure),
            swap_total_bytes: ScalarObservation::unavailable(failure),
            swap_used_bytes: ScalarObservation::unavailable(failure),
            used_rate_mib_per_sec: ScalarObservation::unavailable(failure),
        }
    }
}

/// Structured detailed memory breakdown.
#[derive(Debug, Clone, Default)]
pub struct MemoryMetrics {
    /// Authoritative typed truth for live memory scalars.
    scalar_observations: MemoryScalarObservations,
    /// Authoritative typed truth for optional memory enrichments.
    optional_observations: MemoryOptionalObservations,
}

/// Compatibility-only schema-v1 memory shape. Canonical memory state owns no
/// duplicate scalar, composition, module, commit, compression, or rate fields.
#[derive(Serialize, Deserialize, Default)]
struct MemoryMetricsWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    used_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    available_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cached_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    buffers_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    swap_total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    swap_used_bytes: Option<u64>,
    #[serde(default)]
    scalar_observations: MemoryScalarObservations,
    #[serde(default)]
    optional_observations: MemoryOptionalObservations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hardware_reserved_bytes: Option<u64>,
    #[serde(default, flatten)]
    composition: MemoryComposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    speed_mhz: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slots_used: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slots_total: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    module_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    module_manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    module_form_factor: Option<String>,
    #[serde(default, flatten)]
    virtual_memory_commit: VirtualMemoryCommit,
    #[serde(default, flatten)]
    compression: MemoryCompression,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mem_used_rate_mbps: Option<f32>,
}

impl Serialize for MemoryMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let scalar = &self.scalar_observations;
        let optional = &self.optional_observations;
        MemoryMetricsWire {
            total_bytes: legacy_scalar_projection(&scalar.total_bytes),
            used_bytes: legacy_scalar_projection(&scalar.used_bytes),
            available_bytes: legacy_scalar_projection(&scalar.available_bytes),
            cached_bytes: legacy_optional_projection(&optional.composition.cached_bytes),
            buffers_bytes: legacy_optional_projection(&optional.composition.buffers_bytes),
            swap_total_bytes: legacy_scalar_projection(&scalar.swap_total_bytes),
            swap_used_bytes: legacy_scalar_projection(&scalar.swap_used_bytes),
            scalar_observations: *scalar,
            optional_observations: optional.clone(),
            hardware_reserved_bytes: legacy_optional_projection(&optional.hardware_reserved_bytes),
            composition: MemoryComposition {
                active_bytes: legacy_optional_projection(&optional.composition.active_bytes),
                inactive_bytes: legacy_optional_projection(&optional.composition.inactive_bytes),
                free_bytes: legacy_optional_projection(&optional.composition.free_bytes),
                reclaimable_bytes: legacy_optional_projection(
                    &optional.composition.reclaimable_bytes,
                ),
            },
            speed_mhz: legacy_optional_projection(&optional.modules.speed_mhz),
            slots_used: legacy_optional_projection(&optional.modules.slots_used),
            slots_total: legacy_optional_projection(&optional.modules.slots_total),
            module_type: legacy_optional_projection(&optional.modules.module_type),
            module_manufacturer: legacy_optional_projection(&optional.modules.manufacturer),
            module_form_factor: legacy_optional_projection(&optional.modules.form_factor),
            virtual_memory_commit: VirtualMemoryCommit {
                committed_bytes: legacy_optional_projection(
                    &optional.virtual_memory_commit.committed_bytes,
                ),
                limit_bytes: legacy_optional_projection(
                    &optional.virtual_memory_commit.limit_bytes,
                ),
            },
            compression: MemoryCompression {
                compressed_memory_used_bytes: legacy_optional_projection(
                    &optional.compression.compressed_memory_used_bytes,
                ),
                compressed_swap_used_bytes: legacy_optional_projection(
                    &optional.compression.compressed_swap_used_bytes,
                ),
                compressed_swap_capacity_bytes: legacy_optional_projection(
                    &optional.compression.compressed_swap_capacity_bytes,
                ),
                compressed_swap_cache_enabled: legacy_optional_projection(
                    &optional.compression.compressed_swap_cache_enabled,
                ),
            },
            mem_used_rate_mbps: legacy_scalar_projection(&scalar.used_rate_mib_per_sec),
        }
        .serialize(serializer)
    }
}

/// Hydrate typed observation fields from legacy schema-v1 mirror values.
///
/// One row per typed field that has a frozen schema-v1 mirror, naming the
/// legacy source expression — a flat key, a nested wire group, or a
/// pre-filtered value such as `nonempty(..)`. Typed-only additions without a
/// legacy mirror deliberately carry no row and stay `Unknown` on read. Every
/// row expands to the same guarded merge: typed truth wins, and only an
/// `Unknown` observation adopts the legacy value.
macro_rules! hydrate_legacy_fields {
    ($target:ident : $hydrate:path {
        $($($segment:ident).+ <- $source:expr),+ $(,)?
    }) => {
        $(
            $target.$($segment).+ = $hydrate($target.$($segment).+, $source);
        )+
    };
}

impl<'de> Deserialize<'de> for MemoryMetrics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = MemoryMetricsWire::deserialize(deserializer)?;
        let trustworthy_legacy_denominator = wire.total_bytes.is_some_and(|total| total > 0);
        let mut scalar = wire.scalar_observations;
        if trustworthy_legacy_denominator {
            hydrate_legacy_fields!(scalar: hydrate_legacy_scalar {
                total_bytes <- wire.total_bytes,
                used_bytes <- wire.used_bytes,
                available_bytes <- wire.available_bytes,
                swap_total_bytes <- wire.swap_total_bytes,
                swap_used_bytes <- wire.swap_used_bytes,
                used_rate_mib_per_sec <- wire.mem_used_rate_mbps,
            });
        }
        let trustworthy_optional_identity = scalar
            .total_bytes
            .current_value()
            .is_some_and(|total| *total > 0);
        let mut optional = wire.optional_observations;
        if trustworthy_optional_identity {
            hydrate_legacy_fields!(optional: hydrate_legacy_optional {
                composition.cached_bytes <- wire.cached_bytes,
                composition.buffers_bytes <- wire.buffers_bytes,
                composition.active_bytes <- wire.composition.active_bytes,
                composition.inactive_bytes <- wire.composition.inactive_bytes,
                composition.free_bytes <- wire.composition.free_bytes,
                composition.reclaimable_bytes <- wire.composition.reclaimable_bytes,
                hardware_reserved_bytes <- wire.hardware_reserved_bytes,
                modules.speed_mhz <- wire.speed_mhz,
                modules.slots_used <- wire.slots_used,
                modules.slots_total <- wire.slots_total,
                modules.module_type <- nonempty(wire.module_type),
                modules.manufacturer <- nonempty(wire.module_manufacturer),
                modules.form_factor <- nonempty(wire.module_form_factor),
                virtual_memory_commit.committed_bytes
                    <- wire.virtual_memory_commit.committed_bytes,
                virtual_memory_commit.limit_bytes <- wire.virtual_memory_commit.limit_bytes,
                compression.compressed_memory_used_bytes
                    <- wire.compression.compressed_memory_used_bytes,
                compression.compressed_swap_used_bytes
                    <- wire.compression.compressed_swap_used_bytes,
                compression.compressed_swap_capacity_bytes
                    <- wire.compression.compressed_swap_capacity_bytes,
                compression.compressed_swap_cache_enabled
                    <- wire.compression.compressed_swap_cache_enabled,
            });
        }
        Ok(Self {
            scalar_observations: scalar,
            optional_observations: optional,
        })
    }
}

impl MemoryMetrics {
    #[must_use]
    pub const fn scalar_observations(&self) -> &MemoryScalarObservations {
        &self.scalar_observations
    }

    #[must_use]
    pub const fn optional_observations(&self) -> &MemoryOptionalObservations {
        &self.optional_observations
    }

    #[must_use]
    pub fn from_observations(
        scalar_observations: MemoryScalarObservations,
        optional_observations: MemoryOptionalObservations,
    ) -> Self {
        Self {
            scalar_observations,
            optional_observations,
        }
    }

    #[must_use]
    pub const fn current_total_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .total_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_used_bytes(&self) -> Option<u64> {
        self.scalar_observations.used_bytes.current_value().copied()
    }

    #[must_use]
    pub const fn current_available_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .available_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_swap_total_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .swap_total_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_swap_used_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .swap_used_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_used_rate_mib_per_sec(&self) -> Option<f32> {
        self.scalar_observations
            .used_rate_mib_per_sec
            .current_value()
            .copied()
    }

    /// Replace both canonical memory groups atomically.
    pub fn apply_observations(
        &mut self,
        scalar_observations: MemoryScalarObservations,
        optional_observations: MemoryOptionalObservations,
    ) {
        self.scalar_observations = scalar_observations;
        self.optional_observations = optional_observations;
    }

    pub fn retain_previous_observations(&mut self, previous: &Self) {
        let scalar_observations = self
            .scalar_observations
            .retain_previous(previous.scalar_observations);
        let optional_observations = self
            .optional_observations
            .clone()
            .retain_previous(previous.optional_observations.clone());
        self.scalar_observations = scalar_observations;
        self.optional_observations = optional_observations;
    }

    /// Percentage of physical memory in use when the total is known.
    #[must_use]
    pub fn used_percentage_observed(&self) -> Option<f32> {
        let total = self.current_total_bytes()?;
        let used = self.current_used_bytes()?;
        (total > 0).then(|| (used as f64 / total as f64 * 100.0) as f32)
    }

    /// Percentage of swap in use when swap capacity is present.
    #[must_use]
    pub fn swap_percentage_observed(&self) -> Option<f32> {
        let total = self.current_swap_total_bytes()?;
        let used = self.current_swap_used_bytes()?;
        (total > 0).then(|| (used as f64 / total as f64 * 100.0) as f32)
    }
}

const fn legacy_scalar_projection<T: Copy>(observation: &ScalarObservation<T>) -> Option<T> {
    if matches!(observation.availability(), ScalarAvailability::Available) {
        observation.current_value().copied()
    } else {
        None
    }
}

fn legacy_optional_projection<T: Clone>(observation: &OptionalObservation<T>) -> Option<T> {
    if !matches!(observation.availability(), ScalarAvailability::Available) {
        return None;
    }
    observation.current_value().cloned()
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

fn hydrate_legacy_optional<T>(
    observation: OptionalObservation<T>,
    legacy: Option<T>,
) -> OptionalObservation<T> {
    if matches!(observation.availability(), ScalarAvailability::Unknown)
        && let Some(value) = legacy
    {
        return OptionalObservation::present(value, 0);
    }
    observation
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

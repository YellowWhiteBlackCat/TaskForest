//! Typed optional memory enrichments and schema-v1 projections.

use serde::{Deserialize, Serialize};

use super::MemoryMetrics;
use crate::core::FailureKind;
use crate::core::metrics::OptionalObservation;

/// Declare a typed memory observation group from a single field table.
///
/// One declaration owns the struct — field order, doc comments and per-field
/// serde attributes such as `#[serde(default)]` pass straight through, so the
/// frozen wire shape cannot drift — and both whole-group degradation
/// transitions. A field added to the table therefore always gains
/// `retain_previous`/`unavailable` semantics: the generated struct literals
/// keep both transitions exhaustive, so a forgotten field is a compile error
/// instead of a silently stale or undegraded observation.
macro_rules! observation_group {
    (
        $(#[$outer:meta])*
        $name:ident {
            $(
                $(#[$meta:meta])*
                $field:ident: $observation:ty,
            )+
        }
    ) => {
        $(#[$outer])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
        pub struct $name {
            $(
                $(#[$meta])*
                pub $field: $observation,
            )+
        }

        impl $name {
            /// Retain each prior trustworthy observation only as stale when
            /// the new refresh failed; current observations always win.
            #[must_use]
            pub fn retain_previous(self, previous: Self) -> Self {
                Self {
                    $($field: self.$field.retain_previous(previous.$field),)+
                }
            }

            /// Mark every observation in the group unavailable for one reason.
            #[must_use]
            pub fn unavailable(failure: FailureKind) -> Self {
                Self {
                    $($field: <$observation>::unavailable(failure),)+
                }
            }
        }
    };
}

observation_group! {
    /// Typed optional page-state and cache observations.
    MemoryCompositionObservations {
        cached_bytes: OptionalObservation<u64>,
        buffers_bytes: OptionalObservation<u64>,
        active_bytes: OptionalObservation<u64>,
        inactive_bytes: OptionalObservation<u64>,
        free_bytes: OptionalObservation<u64>,
        reclaimable_bytes: OptionalObservation<u64>,
        /// ZFS adaptive replacement cache size. A reclaimable component in the
        /// same semantics family as slab-reclaimable: the kernel can shrink it
        /// under pressure, but Linux `MemAvailable` does not count it, so its
        /// absence on non-ZFS hosts is a typed absence, never a failure.
        /// `serde(default)` keeps payloads written before this field unknown
        /// rather than unreadable.
        #[serde(default)]
        zfs_arc_bytes: OptionalObservation<u64>,
    }
}

observation_group! {
    /// Typed optional physical-module inventory observations.
    MemoryModuleObservations {
        speed_mhz: OptionalObservation<u32>,
        slots_used: OptionalObservation<usize>,
        slots_total: OptionalObservation<usize>,
        /// Module technology label(s) from the DMI type-17 "Type" field, e.g.
        /// `"LPDDR5"` or `"DDR5 / DDR4"` for a mixed-population host. Sources with
        /// per-module truth (the world-readable udev database) join distinct
        /// types; an empty-but-present set stays honest via `present("")`-free
        /// semantics — see the Linux adapter.
        module_type: OptionalObservation<String>,
        /// Module manufacturer label(s), e.g. `"Samsung"`, joined across distinct
        /// values.
        manufacturer: OptionalObservation<String>,
        /// Module form-factor label(s), e.g. `"SO-DIMM"`, joined across distinct
        /// values (out-of-spec sentinels filtered by the source).
        form_factor: OptionalObservation<String>,
        /// Module part number(s) (the SPD/DIMM label product code), joined
        /// across distinct values. An unprogrammed part number is an honest
        /// absence, never a fabricated placeholder.
        #[serde(default)]
        part_number: OptionalObservation<String>,
        /// Module serial number(s), joined across distinct values. An
        /// unprogrammed (all-zero/sentinel) serial is an honest absence.
        #[serde(default)]
        serial_number: OptionalObservation<String>,
    }
}

observation_group! {
    /// Typed optional virtual-address commitment observations.
    VirtualMemoryCommitObservations {
        committed_bytes: OptionalObservation<u64>,
        limit_bytes: OptionalObservation<u64>,
    }
}

observation_group! {
    /// Typed optional compressed-memory observations.
    MemoryCompressionObservations {
        compressed_memory_used_bytes: OptionalObservation<u64>,
        compressed_swap_used_bytes: OptionalObservation<u64>,
        compressed_swap_capacity_bytes: OptionalObservation<u64>,
        compressed_swap_cache_enabled: OptionalObservation<bool>,
        /// Uncompressed size of the data currently held in the compressed swap
        /// store (Linux zram `mm_stat` `orig_data_size`). `serde(default)`
        /// keeps payloads written before these fields unknown rather than
        /// unreadable.
        #[serde(default)]
        compressed_swap_original_bytes: OptionalObservation<u64>,
        /// Size of that data after compression (zram `mm_stat` `compr_data_size`).
        #[serde(default)]
        compressed_swap_compressed_bytes: OptionalObservation<u64>,
        /// RAM the store consumes to hold the compressed data, metadata included
        /// (zram `mm_stat` `mem_used_total`).
        #[serde(default)]
        compressed_swap_memory_used_bytes: OptionalObservation<u64>,
    }
}

impl MemoryCompressionObservations {
    /// Pure fold: original ÷ compressed for the compressed swap store.
    ///
    /// Guarded: both inputs must be current values and the compressed size
    /// must be positive; anything else yields `None` rather than a
    /// fabricated 0:1 or infinite ratio.
    #[must_use]
    pub fn compression_ratio(&self) -> Option<f32> {
        let original = self
            .compressed_swap_original_bytes
            .current_value()
            .copied()?;
        let compressed = self
            .compressed_swap_compressed_bytes
            .current_value()
            .copied()?;
        (compressed > 0).then(|| original as f32 / compressed as f32)
    }
}

observation_group! {
    /// Authoritative typed truth for optional memory enrichments.
    ///
    /// Presence (`Present`, `Absent`, `NotApplicable`) stays independent from
    /// freshness/failure. Schema-v1 `Option` fields remain projections only.
    MemoryOptionalObservations {
        composition: MemoryCompositionObservations,
        hardware_reserved_bytes: OptionalObservation<u64>,
        modules: MemoryModuleObservations,
        virtual_memory_commit: VirtualMemoryCommitObservations,
        compression: MemoryCompressionObservations,
    }
}

impl MemoryMetrics {
    #[must_use]
    pub fn current_cached_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.composition.cached_bytes)
    }

    #[must_use]
    pub fn current_buffers_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.composition.buffers_bytes)
    }

    #[must_use]
    pub fn current_active_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.composition.active_bytes)
    }

    #[must_use]
    pub fn current_inactive_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.composition.inactive_bytes)
    }

    #[must_use]
    pub fn current_free_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.composition.free_bytes)
    }

    #[must_use]
    pub fn current_reclaimable_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.composition.reclaimable_bytes)
    }

    /// ZFS adaptive replacement cache size, when the host reports one.
    #[must_use]
    pub fn current_zfs_arc_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.composition.zfs_arc_bytes)
    }

    /// Total reclaimable bytes: slab-reclaimable plus the ZFS ARC.
    ///
    /// Pure projection rule for the reclaimable family — the two components
    /// share the "kernel can reclaim under pressure" semantics, so either
    /// one alone is already an honest total. Both missing yields `None`.
    #[must_use]
    pub fn current_reclaimable_with_arc_bytes(&self) -> Option<u64> {
        match (
            self.current_reclaimable_bytes(),
            self.current_zfs_arc_bytes(),
        ) {
            (Some(slab), Some(arc)) => Some(slab.saturating_add(arc)),
            (slab, arc) => slab.or(arc),
        }
    }

    /// Kernel-reported available memory with the ZFS ARC layered on top.
    ///
    /// Linux `MemAvailable` excludes the ARC, so a ZFS host looks starved
    /// while the kernel could reclaim gigabytes of cache. The kernel fact
    /// ([`MemoryMetrics::current_available_bytes`]) is never redefined; this
    /// is the presentation projection for "available" readouts and bars.
    #[must_use]
    pub fn projected_available_bytes(&self) -> Option<u64> {
        let available = self.current_available_bytes()?;
        match self.current_zfs_arc_bytes() {
            Some(arc) => Some(available.saturating_add(arc)),
            None => Some(available),
        }
    }

    #[must_use]
    pub fn current_hardware_reserved_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.hardware_reserved_bytes)
    }

    #[must_use]
    pub fn current_speed_mhz(&self) -> Option<u32> {
        self.current_optional(&self.optional_observations.modules.speed_mhz)
    }

    #[must_use]
    pub fn current_slots_used(&self) -> Option<usize> {
        self.current_optional(&self.optional_observations.modules.slots_used)
    }

    #[must_use]
    pub fn current_slots_total(&self) -> Option<usize> {
        self.current_optional(&self.optional_observations.modules.slots_total)
    }

    #[must_use]
    pub fn current_module_type(&self) -> Option<&str> {
        self.optional_observations
            .modules
            .module_type
            .current_value()
            .map(String::as_str)
    }

    #[must_use]
    pub fn current_module_manufacturer(&self) -> Option<&str> {
        self.optional_observations
            .modules
            .manufacturer
            .current_value()
            .map(String::as_str)
    }

    #[must_use]
    pub fn current_module_form_factor(&self) -> Option<&str> {
        self.optional_observations
            .modules
            .form_factor
            .current_value()
            .map(String::as_str)
    }
    /// Module part number(s) (the SPD/DIMM product code), joined across
    /// distinct values.
    #[must_use]
    pub fn current_module_part_number(&self) -> Option<&str> {
        self.optional_observations
            .modules
            .part_number
            .current_value()
            .map(String::as_str)
    }

    /// Module serial number(s), joined across distinct values; `None` when
    /// the source reported none or only unprogrammed sentinels.
    #[must_use]
    pub fn current_module_serial_number(&self) -> Option<&str> {
        self.optional_observations
            .modules
            .serial_number
            .current_value()
            .map(String::as_str)
    }

    #[must_use]
    pub fn current_committed_bytes(&self) -> Option<u64> {
        self.current_optional(
            &self
                .optional_observations
                .virtual_memory_commit
                .committed_bytes,
        )
    }

    #[must_use]
    pub fn current_commit_limit_bytes(&self) -> Option<u64> {
        self.current_optional(&self.optional_observations.virtual_memory_commit.limit_bytes)
    }

    #[must_use]
    pub fn current_compressed_memory_used_bytes(&self) -> Option<u64> {
        self.current_optional(
            &self
                .optional_observations
                .compression
                .compressed_memory_used_bytes,
        )
    }

    #[must_use]
    pub fn current_compressed_swap_used_bytes(&self) -> Option<u64> {
        self.current_optional(
            &self
                .optional_observations
                .compression
                .compressed_swap_used_bytes,
        )
    }

    #[must_use]
    pub fn current_compressed_swap_capacity_bytes(&self) -> Option<u64> {
        self.current_optional(
            &self
                .optional_observations
                .compression
                .compressed_swap_capacity_bytes,
        )
    }

    #[must_use]
    pub fn current_compressed_swap_cache_enabled(&self) -> Option<bool> {
        self.current_optional(
            &self
                .optional_observations
                .compression
                .compressed_swap_cache_enabled,
        )
    }

    /// Uncompressed size of the data held in the compressed swap store.
    #[must_use]
    pub fn current_compressed_swap_original_bytes(&self) -> Option<u64> {
        self.current_optional(
            &self
                .optional_observations
                .compression
                .compressed_swap_original_bytes,
        )
    }

    /// Size of that data after compression.
    #[must_use]
    pub fn current_compressed_swap_compressed_bytes(&self) -> Option<u64> {
        self.current_optional(
            &self
                .optional_observations
                .compression
                .compressed_swap_compressed_bytes,
        )
    }

    /// RAM the compressed swap store consumes (compressed data + metadata).
    #[must_use]
    pub fn current_compressed_swap_memory_used_bytes(&self) -> Option<u64> {
        self.current_optional(
            &self
                .optional_observations
                .compression
                .compressed_swap_memory_used_bytes,
        )
    }

    /// Original ÷ compressed for the compressed swap store, from the core
    /// pure rule (guarded against zero/unavailable inputs).
    #[must_use]
    pub fn current_compressed_swap_ratio(&self) -> Option<f32> {
        self.optional_observations.compression.compression_ratio()
    }

    fn current_optional<T: Copy>(&self, observation: &OptionalObservation<T>) -> Option<T> {
        observation.current_value().copied()
    }
}

//! Typed cross-crate memory fixtures.
//!
//! These doc-hidden builders expose canonical observation groups and named
//! domain assembly only. They deliberately do not accept schema-v1 field
//! names or hydrate legacy sentinel values.

use std::marker::PhantomData;

use super::{
    MemoryMetrics, MemoryOptionalObservations, MemoryScalarObservations, OptionalObservation,
    ScalarObservation,
};
use crate::{GroupBaseOpen, NamedOverrides};

const FIXTURE_OBSERVED_AT: u64 = 1;

/// Canonical memory-row builder for cross-crate behavior fixtures.
#[doc(hidden)]
#[derive(Debug)]
pub struct MemoryMetricsFixtureBuilder<ScalarStage = GroupBaseOpen> {
    item: MemoryMetrics,
    scalars: MemoryScalarObservations,
    optional: MemoryOptionalObservations,
    scalar_stage: PhantomData<ScalarStage>,
}

impl Default for MemoryMetricsFixtureBuilder {
    fn default() -> Self {
        Self {
            item: MemoryMetrics::default(),
            scalars: MemoryScalarObservations::default(),
            optional: MemoryOptionalObservations::default(),
            scalar_stage: PhantomData,
        }
    }
}

impl MemoryMetricsFixtureBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_item(item: MemoryMetrics) -> Self {
        let scalars = *item.scalar_observations();
        let optional = item.optional_observations().clone();
        Self {
            item,
            scalars,
            optional,
            scalar_stage: PhantomData,
        }
    }
}

impl<ScalarStage> MemoryMetricsFixtureBuilder<ScalarStage> {
    fn retag<NextScalar>(self) -> MemoryMetricsFixtureBuilder<NextScalar> {
        MemoryMetricsFixtureBuilder {
            item: self.item,
            scalars: self.scalars,
            optional: self.optional,
            scalar_stage: PhantomData,
        }
    }

    #[must_use]
    pub fn build(mut self) -> MemoryMetrics {
        self.item.apply_observations(self.scalars, self.optional);
        self.item
    }
}

impl MemoryMetricsFixtureBuilder<GroupBaseOpen> {
    /// Install the scalar group base. Both whole-group bases stay legal
    /// until the first named override closes the base stage.
    #[must_use]
    pub fn scalar_observations(mut self, value: MemoryScalarObservations) -> Self {
        self.scalars = value;
        self
    }

    /// Install the optional-observation group base. Both whole-group bases
    /// stay legal until the first named override closes the base stage.
    #[must_use]
    pub fn optional_observations(mut self, value: MemoryOptionalObservations) -> Self {
        self.optional = value;
        self
    }

    #[must_use]
    pub fn current_total_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_total_bytes(value)
    }

    #[must_use]
    pub fn current_used_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_used_bytes(value)
    }

    #[must_use]
    pub fn current_available_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_available_bytes(value)
    }

    #[must_use]
    pub fn current_swap_total_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_swap_total_bytes(value)
    }

    #[must_use]
    pub fn current_swap_used_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_swap_used_bytes(value)
    }

    #[must_use]
    pub fn current_used_rate_mib_per_sec(
        self,
        value: f32,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_used_rate_mib_per_sec(value)
    }

    #[must_use]
    pub fn cached_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.cached_bytes(value)
    }

    #[must_use]
    pub fn buffers_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.buffers_bytes(value)
    }

    #[must_use]
    pub fn active_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.active_bytes(value)
    }

    #[must_use]
    pub fn inactive_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.inactive_bytes(value)
    }

    #[must_use]
    pub fn free_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.free_bytes(value)
    }

    #[must_use]
    pub fn reclaimable_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.reclaimable_bytes(value)
    }

    #[must_use]
    pub fn zfs_arc_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.zfs_arc_bytes(value)
    }

    #[must_use]
    pub fn hardware_reserved_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.hardware_reserved_bytes(value)
    }

    #[must_use]
    pub fn speed_mhz(self, value: u32) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.speed_mhz(value)
    }

    #[must_use]
    pub fn slots_used(self, value: usize) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.slots_used(value)
    }

    #[must_use]
    pub fn slots_total(self, value: usize) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.slots_total(value)
    }

    #[must_use]
    pub fn committed_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.committed_bytes(value)
    }

    #[must_use]
    pub fn commit_limit_bytes(self, value: u64) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.commit_limit_bytes(value)
    }

    #[must_use]
    pub fn committed_bytes_observation(
        self,
        value: OptionalObservation<u64>,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.committed_bytes_observation(value)
    }

    #[must_use]
    pub fn commit_limit_bytes_observation(
        self,
        value: OptionalObservation<u64>,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.commit_limit_bytes_observation(value)
    }

    #[must_use]
    pub fn compressed_swap_used_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.compressed_swap_used_bytes(value)
    }

    #[must_use]
    pub fn compressed_swap_capacity_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.compressed_swap_capacity_bytes(value)
    }

    #[must_use]
    pub fn compressed_swap_original_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.compressed_swap_original_bytes(value)
    }

    #[must_use]
    pub fn compressed_swap_compressed_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.compressed_swap_compressed_bytes(value)
    }

    #[must_use]
    pub fn compressed_swap_memory_used_bytes(
        self,
        value: u64,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.compressed_swap_memory_used_bytes(value)
    }

    #[must_use]
    pub fn compressed_swap_cache_enabled(
        self,
        value: bool,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.compressed_swap_cache_enabled(value)
    }

    #[must_use]
    pub fn compressed_swap_used_bytes_observation(
        self,
        value: OptionalObservation<u64>,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.compressed_swap_used_bytes_observation(value)
    }

    #[must_use]
    pub fn compressed_swap_capacity_bytes_observation(
        self,
        value: OptionalObservation<u64>,
    ) -> MemoryMetricsFixtureBuilder<NamedOverrides> {
        let next: MemoryMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.compressed_swap_capacity_bytes_observation(value)
    }
}

impl MemoryMetricsFixtureBuilder<NamedOverrides> {
    #[must_use]
    pub fn current_total_bytes(mut self, value: u64) -> Self {
        self.scalars.total_bytes = available(value);
        self
    }

    #[must_use]
    pub fn current_used_bytes(mut self, value: u64) -> Self {
        self.scalars.used_bytes = available(value);
        self
    }

    #[must_use]
    pub fn current_available_bytes(mut self, value: u64) -> Self {
        self.scalars.available_bytes = available(value);
        self
    }

    #[must_use]
    pub fn current_swap_total_bytes(mut self, value: u64) -> Self {
        self.scalars.swap_total_bytes = available(value);
        self
    }

    #[must_use]
    pub fn current_swap_used_bytes(mut self, value: u64) -> Self {
        self.scalars.swap_used_bytes = available(value);
        self
    }

    #[must_use]
    pub fn current_used_rate_mib_per_sec(mut self, value: f32) -> Self {
        self.scalars.used_rate_mib_per_sec = available(value);
        self
    }

    #[must_use]
    pub fn cached_bytes(mut self, value: u64) -> Self {
        self.optional.composition.cached_bytes = present(value);
        self
    }

    #[must_use]
    pub fn buffers_bytes(mut self, value: u64) -> Self {
        self.optional.composition.buffers_bytes = present(value);
        self
    }

    #[must_use]
    pub fn active_bytes(mut self, value: u64) -> Self {
        self.optional.composition.active_bytes = present(value);
        self
    }

    #[must_use]
    pub fn inactive_bytes(mut self, value: u64) -> Self {
        self.optional.composition.inactive_bytes = present(value);
        self
    }

    #[must_use]
    pub fn free_bytes(mut self, value: u64) -> Self {
        self.optional.composition.free_bytes = present(value);
        self
    }

    #[must_use]
    pub fn reclaimable_bytes(mut self, value: u64) -> Self {
        self.optional.composition.reclaimable_bytes = present(value);
        self
    }

    #[must_use]
    pub fn zfs_arc_bytes(mut self, value: u64) -> Self {
        self.optional.composition.zfs_arc_bytes = present(value);
        self
    }

    #[must_use]
    pub fn hardware_reserved_bytes(mut self, value: u64) -> Self {
        self.optional.hardware_reserved_bytes = present(value);
        self
    }

    #[must_use]
    pub fn speed_mhz(mut self, value: u32) -> Self {
        self.optional.modules.speed_mhz = present(value);
        self
    }

    #[must_use]
    pub fn slots_used(mut self, value: usize) -> Self {
        self.optional.modules.slots_used = present(value);
        self
    }

    #[must_use]
    pub fn slots_total(mut self, value: usize) -> Self {
        self.optional.modules.slots_total = present(value);
        self
    }

    #[must_use]
    pub fn committed_bytes(mut self, value: u64) -> Self {
        self.optional.virtual_memory_commit.committed_bytes = present(value);
        self
    }

    #[must_use]
    pub fn commit_limit_bytes(mut self, value: u64) -> Self {
        self.optional.virtual_memory_commit.limit_bytes = present(value);
        self
    }

    #[must_use]
    pub fn committed_bytes_observation(mut self, value: OptionalObservation<u64>) -> Self {
        self.optional.virtual_memory_commit.committed_bytes = value;
        self
    }

    #[must_use]
    pub fn commit_limit_bytes_observation(mut self, value: OptionalObservation<u64>) -> Self {
        self.optional.virtual_memory_commit.limit_bytes = value;
        self
    }

    #[must_use]
    pub fn compressed_swap_used_bytes(mut self, value: u64) -> Self {
        self.optional.compression.compressed_swap_used_bytes = present(value);
        self
    }

    #[must_use]
    pub fn compressed_swap_capacity_bytes(mut self, value: u64) -> Self {
        self.optional.compression.compressed_swap_capacity_bytes = present(value);
        self
    }

    #[must_use]
    pub fn compressed_swap_original_bytes(mut self, value: u64) -> Self {
        self.optional.compression.compressed_swap_original_bytes = present(value);
        self
    }

    #[must_use]
    pub fn compressed_swap_compressed_bytes(mut self, value: u64) -> Self {
        self.optional.compression.compressed_swap_compressed_bytes = present(value);
        self
    }

    #[must_use]
    pub fn compressed_swap_memory_used_bytes(mut self, value: u64) -> Self {
        self.optional.compression.compressed_swap_memory_used_bytes = present(value);
        self
    }

    #[must_use]
    pub fn compressed_swap_cache_enabled(mut self, value: bool) -> Self {
        self.optional.compression.compressed_swap_cache_enabled = present(value);
        self
    }

    #[must_use]
    pub fn compressed_swap_used_bytes_observation(
        mut self,
        value: OptionalObservation<u64>,
    ) -> Self {
        self.optional.compression.compressed_swap_used_bytes = value;
        self
    }

    #[must_use]
    pub fn compressed_swap_capacity_bytes_observation(
        mut self,
        value: OptionalObservation<u64>,
    ) -> Self {
        self.optional.compression.compressed_swap_capacity_bytes = value;
        self
    }
}

fn available<T>(value: T) -> ScalarObservation<T> {
    ScalarObservation::available(value, FIXTURE_OBSERVED_AT)
}

fn present<T>(value: T) -> OptionalObservation<T> {
    OptionalObservation::present(value, FIXTURE_OBSERVED_AT)
}

//! Neutral category-bucket projection shared by every frontend.
//!
//! The four frontends (gpui / iced / tui / bevy) each render a "Group by category"
//! process table. Everything they must agree on — bucket order, empty-bucket
//! omission, member order, the shared aggregate conventions, and the
//! locale-neutral expansion key — lives here, toolkit-neutral: no colors, no
//! layout, no toolkit types. Labels, i18n, row shapes, and the single-member
//! presentation policy stay in each frontend.
//!
//! Fixed semantics (the "same" every frontend consumes):
//! - Buckets are emitted in the fixed
//!   [`taskmanager_core::core::process::ProcessCategory::ALL`] order
//!   (Application → Background → Uncategorized).
//! - An empty bucket is omitted entirely (no fabricated header).
//! - Each bucket keeps its members in INPUT order — the caller passes the
//!   already-sorted visible list ("group members follow the active sort");
//!   this module never re-sorts.
//! - Aggregation is typed: `CategoryBucketProjection::aggregate_f32` and
//!   `CategoryBucketProjection::aggregate_u64` delegate to core's
//!   availability-preserving
//!   [`taskmanager_core::core::process::aggregate::AggregateMetric`] folds. A missing member is
//!   therefore not silently converted into a successful zero. Callers choose
//!   the typed observation field, while core remains the sole authority for
//!   coverage, freshness, failure, and saturating-add semantics.
//! - The expansion key is `category:<stable_key>` — locale-neutral (a
//!   language switch never orphans an expansion) and collision-free with any
//!   normalized app-group name or type label.
//!
//! Frontends reach this projection through `taskmanager-application` and
//! import the underlying
//! [`taskmanager_core::core::process::ProcessCategory`] facts directly from their owner
//! module in `taskmanager-core`.

use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::aggregate::{AggregateMetric, aggregate_f32, aggregate_u64};
use taskmanager_core::core::process::{ProcessCategory, ProcessItem};

/// Prefix of every category expansion key (see [`category_expansion_key`]).
pub const CATEGORY_EXPANSION_KEY_PREFIX: &str = "category:";

/// Stable expansion-set key for one category bucket, shared verbatim by every
/// frontend: `category:<stable_key>` (see [`CATEGORY_EXPANSION_KEY_PREFIX`]).
/// Locale-neutral — a language switch never orphans an expansion — and
/// collision-free with any normalized app-group name or type label, which
/// cannot contain the prefix.
#[must_use]
pub fn category_expansion_key(category: ProcessCategory) -> String {
    format!("{CATEGORY_EXPANSION_KEY_PREFIX}{}", category.stable_key())
}

/// One non-empty [`ProcessCategory`] bucket over the caller's items: the
/// category plus its members in input order. Toolkit-neutral — it carries no
/// label, color, or row shape; each frontend projects it into its own row
/// type and keeps its own single-member presentation policy.
#[derive(Debug, Clone)]
pub struct CategoryBucketProjection<'a, T> {
    category: ProcessCategory,
    members: Vec<&'a T>,
}

impl<T> CategoryBucketProjection<'_, T> {
    /// The bucket's category (its position in [`ProcessCategory::ALL`]).
    #[must_use]
    pub const fn category(&self) -> ProcessCategory {
        self.category
    }

    /// The bucket's members in input order (the caller's active sort order).
    #[must_use]
    pub fn members(&self) -> &[&'_ T] {
        &self.members
    }

    /// How many members the bucket holds.
    #[must_use]
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Aggregate one typed `f32` observation over the members.
    ///
    /// `observed_at_ms` must come from the accepted owning snapshot. The
    /// returned [`AggregateMetric`] preserves the distinction between a
    /// measured zero, partial coverage, stale history, unavailable data, and
    /// an unknown observation.
    #[must_use]
    pub fn aggregate_f32(
        &self,
        observed_at_ms: u64,
        observation: impl Fn(&T) -> &ScalarObservation<f32>,
    ) -> Option<AggregateMetric<f32>> {
        aggregate_f32(
            self.members.iter().map(|member| observation(*member)),
            observed_at_ms,
        )
    }

    /// Aggregate one typed `u64` observation over the members with core's
    /// saturating addition. See [`Self::aggregate_f32`] for availability
    /// semantics.
    #[must_use]
    pub fn aggregate_u64(
        &self,
        observed_at_ms: u64,
        observation: impl Fn(&T) -> &ScalarObservation<u64>,
    ) -> Option<AggregateMetric<u64>> {
        aggregate_u64(
            self.members.iter().map(|member| observation(*member)),
            observed_at_ms,
        )
    }
}

impl CategoryBucketProjection<'_, ProcessItem> {
    /// Aggregate the process members' typed CPU observations.
    ///
    /// This is the canonical CPU metric for category headers. The caller
    /// supplies the timestamp of the accepted process snapshot; the fold
    /// itself remains owned by core.
    #[must_use]
    pub fn aggregate_process_cpu(&self, observed_at_ms: u64) -> Option<AggregateMetric<f32>> {
        self.aggregate_f32(observed_at_ms, |process| {
            &process.scalar_observations().cpu_percentage
        })
    }

    /// Aggregate the process members' resident-set-size observations.
    #[must_use]
    pub fn aggregate_process_memory_rss(
        &self,
        observed_at_ms: u64,
    ) -> Option<AggregateMetric<u64>> {
        self.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().memory_bytes
        })
    }

    /// Aggregate the process members' PSS observations without changing the
    /// measurement kind when PSS is unavailable.
    #[must_use]
    pub fn aggregate_process_memory_pss(
        &self,
        observed_at_ms: u64,
    ) -> Option<AggregateMetric<u64>> {
        self.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().memory_pss_bytes
        })
    }

    /// Aggregate the product's PSS-preferred memory display metric.
    ///
    /// A process uses current PSS when it is present and otherwise uses its
    /// typed RSS observation. This selection is an application display rule;
    /// each selected value still goes through core's typed aggregate fold, so
    /// unavailable/stale/partial coverage cannot be collapsed into zero.
    #[must_use]
    pub fn aggregate_process_memory_for_display(
        &self,
        observed_at_ms: u64,
    ) -> Option<AggregateMetric<u64>> {
        self.aggregate_u64(observed_at_ms, process_memory_observation_for_display)
    }
}

/// Select the typed memory observation used by the Applications display.
///
/// PSS is preferred only while it has a current value. A stale or unavailable
/// PSS observation does not masquerade as current PSS; the display falls back
/// to the process' independently typed RSS observation instead.
#[must_use]
pub fn process_memory_observation_for_display(process: &ProcessItem) -> &ScalarObservation<u64> {
    if process
        .scalar_observations()
        .memory_pss_bytes
        .current_value()
        .is_some()
    {
        &process.scalar_observations().memory_pss_bytes
    } else {
        &process.scalar_observations().memory_bytes
    }
}

/// Split `items` into one [`CategoryBucketProjection`] per NON-EMPTY category,
/// in the fixed [`ProcessCategory::ALL`] order; an empty bucket is omitted
/// entirely. Members keep their input order — pass the already-sorted visible
/// list; this function never re-sorts. `classify` is expected to be the core
/// `core::process::process_category` (the single classification source), but stays a
/// parameter so tests and future callers can classify any item type.
#[must_use]
pub fn category_buckets<'a, T>(
    items: &'a [T],
    classify: impl Fn(&T) -> ProcessCategory,
) -> Vec<CategoryBucketProjection<'a, T>> {
    // The exhaustive match keeps the slot mapping compile-time total: if core
    // ever adds a variant, this match (and therefore this function) fails to
    // compile until ALL and the slots are extended together.
    let mut slots: [Vec<&'a T>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for item in items {
        let slot = match classify(item) {
            ProcessCategory::Application => 0,
            ProcessCategory::Background => 1,
            ProcessCategory::Uncategorized => 2,
        };
        slots[slot].push(item);
    }
    let mut buckets = Vec::new();
    for (category, members) in ProcessCategory::ALL.into_iter().zip(slots) {
        if members.is_empty() {
            continue;
        }
        buckets.push(CategoryBucketProjection { category, members });
    }
    buckets
}

#[cfg(test)]
#[path = "../tests/headless/application_process_category_projection_tests.rs"]
mod tests;

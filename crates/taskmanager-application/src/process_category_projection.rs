//! Neutral category-bucket projection shared by every frontend.
//!
//! The three frontends (gpui / iced / tui) each render a "Group by category"
//! process table. Everything they must agree on — bucket order, empty-bucket
//! omission, member order, the shared aggregate conventions, and the
//! locale-neutral expansion key — lives here, toolkit-neutral: no colors, no
//! layout, no toolkit types. Labels, i18n, row shapes, and the single-member
//! presentation policy stay in each frontend.
//!
//! Fixed semantics (the "same" every frontend consumes):
//! - Buckets are emitted in the fixed [`ProcessCategory::ALL`] order
//!   (Application → Background → Uncategorized).
//! - An empty bucket is omitted entirely (no fabricated header).
//! - Each bucket keeps its members in INPUT order — the caller passes the
//!   already-sorted visible list ("group members follow the active sort");
//!   this module never re-sorts.
//! - Aggregation is closure-driven: `CategoryBucketProjection::sum_f32`
//!   sums the available values and skips members whose extractor returns
//!   `None` (the `filter_map().sum()` header-CPU% convention every frontend
//!   used), and `CategoryBucketProjection::sum_u64` is its saturating u64
//!   counterpart. Callers needing a different fold (all-or-nothing `Option`
//!   sums, PSS-preferred memory, ...) fold
//!   `CategoryBucketProjection::members` themselves with their own metric
//!   closure.
//! - The expansion key is `category:<stable_key>` — locale-neutral (a
//!   language switch never orphans an expansion) and collision-free with any
//!   normalized app-group name or type label.
//!
//! Frontends reach this module through `taskmanager_application`; the TUI
//! (firewalled from `taskmanager-core`) also reaches [`ProcessCategory`] /
//! `core::process::process_category` through the crate-root re-exports, as before.

use crate::ProcessCategory;

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

    /// Sum one optional `f32` metric over the members, skipping members whose
    /// extractor returns `None` — an unavailable observation contributes
    /// nothing (the header CPU% convention every frontend used).
    #[must_use]
    pub fn sum_f32(&self, value: impl Fn(&T) -> Option<f32>) -> f32 {
        self.members.iter().filter_map(|member| value(member)).sum()
    }

    /// Saturating sum of one optional `u64` metric over the members, skipping
    /// members whose extractor returns `None`.
    #[must_use]
    pub fn sum_u64(&self, value: impl Fn(&T) -> Option<u64>) -> u64 {
        self.members
            .iter()
            .filter_map(|member| value(member))
            .fold(0, u64::saturating_add)
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

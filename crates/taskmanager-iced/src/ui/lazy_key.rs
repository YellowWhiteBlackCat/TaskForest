//! The lazy-body invalidation discipline, typed (the iced counterpart of the
//! GPUI FrameBudget/ContentBudget cache-key doctrine, ADR-038).
//!
//! Every large iced surface rebuilds through `iced::widget::lazy`, and the
//! lazy body's correctness rests entirely on its key: a key that misses a
//! visual input shows stale rows forever, a key that flaps rebuilds every
//! frame. This module turns that discipline from a per-table convention into
//! one component:
//!
//! - **scope** — the surface's stable identity. Two different surfaces with
//!   identical visual inputs must never share a key; every key names its
//!   table, so a field mix cannot collide across surfaces.
//! - **theme fingerprint** — the ONE rule for when a theme change invalidates
//!   a materialized tree (skin, mode, contrast, both font families). Tables
//!   no longer hand-copy a subset and drift.
//! - **fields** — the page-owned visual inputs (query, selection, open
//!   surfaces, column state). Set-valued inputs are sorted before hashing so
//!   iteration order cannot cause spurious rebuilds.
//!
//! What deliberately does NOT enter a key:
//! - the raw scroll offset (it enters only through the clamped
//!   [`VirtualWindow`], so scrolling inside the overscan band is free);
//! - hover/pointer state (styling handles it without rebuilding the tree);
//! - interaction truth itself (selection, open menus live in app state — the
//!   key carries only their visual consequence).
//!
//! Canvas caches follow the same doctrine at the renderer layer: static
//! geometry is keyed by data revision, dynamic layers redraw per frame (see
//! `perf_chart`'s DATA/OVERLAY split).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use taskmanager_theme::Theme;

use super::virtual_list::VirtualWindow;

/// One lazy-body invalidation key under construction. Cheap; build, chain,
/// [`Self::finish`].
pub(crate) struct LazyKey {
    hasher: DefaultHasher,
}

impl LazyKey {
    /// Start the key for one named surface. The scope is the surface's stable
    /// identity — a table added later must pick its own name, not reuse one.
    pub(crate) fn new(scope: &'static str) -> Self {
        let mut hasher = DefaultHasher::new();
        scope.hash(&mut hasher);
        Self { hasher }
    }

    /// The data watermark this surface renders (projection generation,
    /// history revision). A new watermark means new rows, not new visuals.
    pub(crate) fn revision(mut self, revision: u64) -> Self {
        revision.hash(&mut self.hasher);
        self
    }

    /// The shared theme fingerprint: every axis that can change the painted
    /// surface. One rule for all tables — a new visual axis is added here
    /// once, never per table.
    pub(crate) fn theme(mut self, theme: &Theme) -> Self {
        theme.skin.label().hash(&mut self.hasher);
        theme.mode.label().hash(&mut self.hasher);
        theme.dark.hash(&mut self.hasher);
        theme.hc.hash(&mut self.hasher);
        theme.ui_font.hash(&mut self.hasher);
        theme.mono_font.hash(&mut self.hasher);
        self
    }

    /// The materialized row window, for surfaces that fold the window into
    /// their base key instead of wrapping with [`super::virtual_list::
    /// virtual_table_key`].
    pub(crate) fn geometry(mut self, window: VirtualWindow) -> Self {
        window.key().hash(&mut self.hasher);
        self
    }

    /// One page-owned visual input. Set-valued inputs must be sorted (or
    /// otherwise order-stabilized) by the caller before hashing.
    pub(crate) fn field(mut self, field: impl Hash) -> Self {
        field.hash(&mut self.hasher);
        self
    }

    pub(crate) fn finish(self) -> u64 {
        self.hasher.finish()
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/lazy_key_tests.rs"]
mod tests;

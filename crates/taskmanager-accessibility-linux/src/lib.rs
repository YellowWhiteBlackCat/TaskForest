#![forbid(unsafe_code)]

//! Linux AT-SPI accessibility bridge for TaskForest.
//!
//! This crate turns the toolkit-neutral [`SemanticSnapshot`] produced by
//! `taskmanager-ui-contract` into a live screen-reader feed on Linux by driving
//! an `accesskit_unix` adapter. (Plain code span, not an intra-doc link:
//! `accesskit_unix` is a Linux-only dependency, so the doc build on other
//! targets must still resolve.)
//!
//! ## Layout
//!
//! * [`mapping`] is pure, depends only on core `accesskit` types, and compiles
//!   on every target so the `SemanticSnapshot → accesskit::TreeUpdate`
//!   translation is unit-testable off-Linux.
//! * `bridge` (Linux only) wraps `accesskit_unix::Adapter` and implements the
//!   [`AccessibilityBridge`](taskmanager_ui_contract::AccessibilityBridge)
//!   trait: it publishes snapshots lazily (a no-op until an AT subscribes) and
//!   translates inbound AT actions back into semantic action requests.
//!
//! ## Why this works under gpui/Wayland
//!
//! `accesskit_unix::Adapter::new` takes **no window handle**: it registers on
//! the AT-SPI session bus at the process level and only connects to the a11y
//! bus once a screen reader (Orca, etc.) has enabled accessibility on the
//! desktop. gpui never has to expose its Wayland `wl_surface`, which gpui 0.2.2
//! has no accesskit hook for anyway.
//!
//! [`SemanticSnapshot`]: taskmanager_ui_contract::SemanticSnapshot

pub mod mapping;

#[cfg(target_os = "linux")]
mod bridge;

#[cfg(target_os = "linux")]
pub use bridge::LinuxAccessKitBridge;

#[cfg(not(target_os = "linux"))]
pub type LinuxAccessKitBridge = taskmanager_ui_contract::DetachedAccessibilityBridge;

// Re-export the pure mapping entry points at the crate root so frontends can
// build a `TreeUpdate` without naming the inner module.
pub use mapping::{snapshot_to_tree_update, stable_node_id};

/// Status of the accesskit bridge on the current target.
///
/// Only Linux links a real adapter; every other target falls back to the
/// contract's `DetachedAccessibilityBridge`, which honestly reports that no
/// native bridge is linked.
#[cfg(target_os = "linux")]
pub const ACCESSKIT_BRIDGE_AVAILABLE: bool = true;

#[cfg(not(target_os = "linux"))]
pub const ACCESSKIT_BRIDGE_AVAILABLE: bool = false;

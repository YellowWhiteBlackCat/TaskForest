//! Platform-neutral domain-model foundation for TaskForest: identity,
//! telemetry snapshots, metrics, sensors, process and process-telemetry,
//! alerts, storage, startup, and services.
//!
//! Defines no operating-system behaviour and depends on no native adapter.

#![forbid(unsafe_code)]

pub mod core;

// The domain module owns the explicit aggregate API in `core.rs`. This
// compatibility facade intentionally forwards that single aggregate surface;
// the surface guard checks renamed aliases, not this established path.
pub use core::*;

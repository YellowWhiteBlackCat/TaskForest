//! Platform-neutral domain-model foundation for TaskForest: identity,
//! telemetry snapshots, metrics, sensors, process and process-telemetry,
//! alerts, storage, startup, and services.
//!
//! Defines no operating-system behaviour and depends on no native adapter.

#![forbid(unsafe_code)]

pub mod core;

// The domain module owns the explicit aggregate API in `core.rs`; this crate
// root keeps the owner crate's named module tree available to its own public
// API. Cross-layer consumers must still import the owner module directly.
pub use core::*;

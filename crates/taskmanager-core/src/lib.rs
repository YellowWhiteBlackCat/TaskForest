//! Platform-neutral domain-model foundation for TaskForest: identity,
//! telemetry snapshots, metrics, sensors, process and process-telemetry,
//! alerts, storage, startup, and services.
//!
//! # Architecture & Guarantees
//!
//! - **Platform-Neutral & Pure**: Defines no operating-system behaviour, issues no
//!   syscalls, and performs no filesystem, network, or process I/O.
//! - **Deterministic & Clock-Independent**: Never reads the wall clock or monotonic
//!   clock directly (`SystemTime::now()` or `Instant::now()`). All temporal logic
//!   receives explicit timestamps from callers.
//! - **Safe Rust**: The entire crate is strictly `#![forbid(unsafe_code)]`.
//!
//! # Example
//!
//! ```rust
//! use taskmanager_core::identity::ProviderId;
//! use taskmanager_core::units::{format_memory, UnitPreferences};
//!
//! // Stable provider identity
//! let provider = ProviderId::borrowed("linux.procfs");
//! assert_eq!(provider.as_str(), "linux.procfs");
//!
//! // Canonical unit formatting
//! let prefs = UnitPreferences::default();
//! let readable = format_memory(1024 * 1024 * 1024, &prefs);
//! assert_eq!(readable, "1.0 GiB");
//! ```

#![forbid(unsafe_code)]

pub mod core;

// The domain module owns the explicit aggregate API in `core.rs`; this crate
// root keeps the owner crate's named module tree available to its own public
// API. Cross-layer consumers must still import the owner module directly.
pub use core::*;

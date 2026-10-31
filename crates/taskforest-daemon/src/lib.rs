//! # taskforest-daemon
//!
//! Background telemetry collector, persistence owner, and IPC daemon service for TaskForest.
//!
//! Implements ADR-055's background tier:
//! - 7×24 continuous background telemetry persistence holding the single-writer lock.
//! - Multi-client on-demand subscription aggregation via `taskforest-ipc`.
//! - Graceful sleep/downscale when zero frontend subscribers are connected.
//!
//! # Safety & Invariants
//!
//! - **Strict Safe Rust**: Strictly `#![forbid(unsafe_code)]`.
//! - **Single Writer Authority**: Frontends in daemon mode never write directly to history files.

#![forbid(unsafe_code)]

pub mod collector;
pub mod config;
pub mod persistence;
pub mod service;
pub mod state;

pub use collector::{DaemonCollector, MetricsCollector, create_platform_collector};
pub use config::DaemonConfig;
pub use persistence::DaemonHistoryManager;
pub use service::DaemonService;
pub use state::{DaemonCadenceMode, DaemonState};

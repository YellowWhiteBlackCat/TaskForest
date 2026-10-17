//! # taskmanager-ipc
//!
//! High-performance compact binary IPC protocol and cross-platform transport for TaskForest.
//!
//! Provides structured, framed, and bounded inter-process communication between the
//! background `taskforest-daemon` service and multiple frontend client instances
//! (`taskforest-g`, `taskforest-i`, `taskmanager-tui`, `taskforest-b`).
//!
//! # Safety & Invariants
//!
//! - **Strict Safe Rust**: The entire crate is `#![forbid(unsafe_code)]`.
//! - **Framed & Bounded**: Magic verification, CRC32 checksums, and strict 16 MiB payload ceiling.
//! - **On-Demand Sampling**: Subscription bitmask aggregation prevents unnecessary polling.

#![forbid(unsafe_code)]

pub mod protocol;
pub mod service;
pub mod transport;

pub use protocol::{
    ClientMessage, DaemonMessage, Frame, FrameHeader, IpcError, IpcResult, Opcode, SubscriptionMask,
};
pub use service::{ClientSession, IpcClient, IpcConnectionHandler, SubscriptionHub};
pub use transport::{FramedReader, FramedWriter, IpcEndpoint};

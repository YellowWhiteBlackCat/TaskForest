//! Neutral single-instance contract.
//!
//! The GUI product is single-instance: a second launch must not open a second
//! window; instead it asks the existing instance to show its main window and
//! then exits. This module is the toolkit-neutral vocabulary for that
//! handshake. The native mechanism is per-OS (Unix socket on Linux/macOS,
//! named mutex + named event on Windows), owned by the OS adapters; here we
//! only define the outcome and the inbound event.

#![forbid(unsafe_code)]

use std::fmt;

/// Outcome of trying to become the primary instance.
pub enum InstanceRole {
    /// This process owns the instance; it should continue and create its
    /// window/tray. The guard keeps the primary's native resources (Linux
    /// D-Bus name, macOS socket, Windows mutex/event) alive for as long as
    /// the caller holds it; dropping it releases the instance for the next
    /// launch. Activation requests are forwarded to the event channel given
    /// at acquisition.
    Primary(Box<dyn InstanceGuard>),
    /// Another process already owns the instance. The adapter has already
    /// asked that instance to show its window (best-effort); the caller
    /// should exit promptly.
    Secondary,
}

impl fmt::Debug for InstanceRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primary(_) => f.write_str("Primary"),
            Self::Secondary => f.write_str("Secondary"),
        }
    }
}

/// Keeps the primary instance's native resources alive.
///
/// `Send + Sync` so the frontend can hold it in its root view for the
/// process lifetime. Dropping it must release the instance.
pub trait InstanceGuard: Send + Sync {}

/// An inbound request delivered to the primary instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceEvent {
    /// A secondary launch (or the tray) asked the primary to show its main
    /// window.
    Activate,
}

/// Why single-instance acquisition failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InstanceFailure {
    /// The platform (or this build shape) cannot provide single-instance.
    Unsupported,
    /// A required dependency is missing (e.g. no runtime directory on Unix).
    MissingDependency,
    /// A transient failure (e.g. a racing stale socket that could not be
    /// recovered); retrying later may succeed.
    Rejected,
}

impl fmt::Display for InstanceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("single-instance is unsupported here"),
            Self::MissingDependency => {
                f.write_str("single-instance dependency is missing (no runtime directory)")
            }
            Self::Rejected => f.write_str("single-instance acquisition was rejected"),
        }
    }
}

impl std::error::Error for InstanceFailure {}

#[cfg(test)]
#[path = "../tests/headless/instance_contract.rs"]
mod tests;

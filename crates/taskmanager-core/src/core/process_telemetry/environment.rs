//! Per-process environment variables and working directory as a typed
//! insight facet.
//!
//! Mirrors the open-files facet: every entry is a fact the native source can
//! prove, absent information stays `None`, and bounded collection reports
//! truncation honestly instead of silently dropping entries.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

/// Hard cap on environment entries kept per process. A provider may read
/// fewer; anything beyond this cap is reported through
/// [`ProcessEnvironment::truncated_count`] rather than fabricated or dropped
/// silently.
pub const MAX_ENVIRONMENT_ENTRIES: usize = 256;

/// Hard cap on the raw environment byte budget per process (NUL-separated
/// entries). A provider must stop reading at this bound and report the number
/// of entries it could not retain.
pub const MAX_ENVIRONMENT_BYTES: usize = 16 * 1024;

/// One bounded environment variable: the key and value are kept verbatim as
/// observed (no normalization, no redaction — the process's own data).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessEnvironmentEntry {
    pub key: String,
    pub value: String,
}

/// The environment facet: working directory plus a bounded key/value table.
///
/// `state` carries the typed collection outcome so a permission-denied read
/// is distinguishable from a genuinely empty environment. `entries` is the
/// bounded subset in source order; `truncated_count` counts entries that were
/// dropped by the byte/entry budget.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProcessEnvironment {
    /// Aggregated collection state. `Healthy` when the native source was
    /// read, `PermissionDenied` when access was refused, `Stale` when the
    /// process vanished mid-read.
    pub state: DeviceState,
    /// Working directory at collection time, when the native source exposes
    /// one. `None` is honest absence — never a fabricated `/`.
    pub working_directory: Option<PathBuf>,
    /// Bounded environment entries in source order.
    pub entries: Vec<ProcessEnvironmentEntry>,
    /// Number of entries the bounded read had to drop.
    pub truncated_count: u32,
}

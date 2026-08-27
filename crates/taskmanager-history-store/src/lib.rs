//! Opt-in persistent telemetry history (roadmap #4, R1).
//!
//! This crate is the storage half of the history-persistence seam: it
//! implements the core [`taskmanager_core::HistoryRecordSink`] port (driven only by
//! `CorrelatedSystemTelemetryIngestor`), buffers accepted samples, and
//! flushes them to one JSONL file per series under an injected directory —
//! the composition edge picks the platform convention, this crate never
//! touches a platform API. Reads go through [`HistoryQuery`], which answers
//! windowed [`taskmanager_core::HistoricalSeries`] / [`taskmanager_core::PeakSummary`]
//! read models for the
//! performance page's future history mode.
//!
//! Privacy and durability posture (deliberate, roadmap #4):
//!
//! * **Off by default.** Nothing here runs unless the composition edge
//!   constructs [`PersistentHistoryStore`] explicitly; the config default is
//!   `false`.
//! * **serde-only v1.** Plain JSONL, no compression — a compressed format
//!   needs a new workspace dependency and owner review (ADR-028).
//! * **Explicit gaps, never zeros.** A missing measurement is stored as a
//!   `null` value and replays as a gap.
//! * **Clock honesty.** Wall-clock steps backwards are kept as recorded and
//!   only *counted* (the core `count_clock_jumps` vocabulary); TTL trimming
//!   never treats a future-dated (jumped-back) sample as expired.
//! * **Bounded loss.** Samples lost on a crash are bounded by the flush
//!   cadence; pending memory and persisted file/cardinality reads have hard
//!   global limits, and a graceful drop flushes with the last seen completion
//!   time.

#![forbid(unsafe_code)]

mod boot_history;
mod bounded_io;
mod query;
mod records;
mod retention;
mod store;

pub use boot_history::{
    BootEvidenceHistory, BootTimelineRecord, MAX_BOOT_HISTORY_BYTES, MAX_RECORDED_BOOTS,
    RecordBootOutcome, boot_history_path,
};
pub use query::{HistoryQuery, SeriesRead};
pub use retention::RetentionPolicy;
pub use store::{
    FlushReport, HistoryStoreStatus, HistoryWriterClaimStatus, MAX_DIRECTORY_ENTRIES_PER_SCAN,
    MAX_PENDING_BYTES, MAX_PENDING_SAMPLES, MAX_PENDING_SERIES, MAX_SERIES_FILE_BYTES,
    MAX_SERIES_FILES, MAX_SERIES_FILES_PER_SCAN, MAX_SERIES_KEY_BYTES, MAX_TRACKED_SERIES,
    PersistentHistoryStore, RecordSampleOutcome, RecordSampleRejection, STALE_TEMPORARY_AGE_MS,
    TRIM_INTERVAL_MS, probe_root_lock,
};

use std::fmt;

/// Typed failure of the history store. `detail` is host-specific diagnostic
/// text; `kind` alone drives decisions so callers never parse message text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryStoreError {
    kind: HistoryStoreErrorKind,
    detail: String,
}

/// The failure categories of the persistence surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryStoreErrorKind {
    CreateDirectory,
    Open,
    Read,
    /// A file that should contain JSONL records could not be decoded at all.
    Decode,
    Write,
    /// Retiring a fully expired series file failed.
    Remove,
    /// The atomic-replace step of a retention rewrite failed.
    Rename,
    /// Serializing records failed (serde derive bug class).
    Encode,
    /// Input or persisted state exceeds a declared memory/file cardinality
    /// boundary. The operation is rejected rather than allocating past it.
    ResourceLimit,
    /// Another live instance owns the history directory (single-writer lock).
    /// The composition edge degrades to in-memory-only telemetry for this run.
    Locked,
}

impl HistoryStoreErrorKind {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::CreateDirectory => "create_directory",
            Self::Open => "open",
            Self::Read => "read",
            Self::Decode => "decode",
            Self::Write => "write",
            Self::Remove => "remove",
            Self::Rename => "rename",
            Self::Encode => "encode",
            Self::ResourceLimit => "resource_limit",
            Self::Locked => "locked",
        }
    }
}

impl HistoryStoreError {
    #[must_use]
    pub fn new(kind: HistoryStoreErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> HistoryStoreErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for HistoryStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.kind.stable_code())
    }
}

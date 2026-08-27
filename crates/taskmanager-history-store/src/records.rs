//! The JSONL wire record and its codec.
//!
//! One line per sample with short field names keeps the serde-only v1 format
//! compact and append-only; decoding a line never panics — a malformed line is
//! skipped and counted by the caller (typed honesty, not a store-down error).

use serde::{Deserialize, Serialize};
use taskmanager_core::HistoricalSample;

/// Serialized form of one sample (one JSONL line). Field names are the stable
/// wire contract: `r` revision, `c` completed_at_ms, `m` measured_at_ms,
/// `v` value (`null` = explicit gap).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct PersistedSampleRecord {
    pub r: u64,
    pub c: u64,
    pub m: Option<u64>,
    pub v: Option<f64>,
}

impl From<HistoricalSample> for PersistedSampleRecord {
    fn from(sample: HistoricalSample) -> Self {
        Self {
            r: sample.revision,
            c: sample.completed_at_ms,
            m: sample.measured_at_ms,
            v: sample.value,
        }
    }
}

impl From<PersistedSampleRecord> for HistoricalSample {
    fn from(record: PersistedSampleRecord) -> Self {
        Self {
            revision: record.r,
            completed_at_ms: record.c,
            measured_at_ms: record.m,
            value: record.v,
        }
    }
}

/// Encode one sample as a JSONL line (without the trailing newline).
///
/// Serialization of this plain-data record is infallible in practice; a
/// failure would mean a serde derive bug, so the empty line it produces is
/// skipped on read (counted as corrupt) rather than treated as fatal.
pub(super) fn encode_line(sample: &HistoricalSample) -> String {
    serde_json::to_string(&PersistedSampleRecord::from(*sample)).unwrap_or_default()
}

/// Decode one JSONL line; `None` for blank or malformed lines.
pub(super) fn decode_line(line: &str) -> Option<HistoricalSample> {
    serde_json::from_str::<PersistedSampleRecord>(line)
        .ok()
        .map(HistoricalSample::from)
}

#[cfg(test)]
#[path = "../tests/headless/records.rs"]
mod tests;

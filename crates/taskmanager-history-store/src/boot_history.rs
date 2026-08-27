//! Per-boot waterfall history (roadmap #5 comparison markers, ADR-030).
//!
//! Stores the last few boots' [`BootTimeline`] projections in one JSON file
//! under the opt-in history directory, so the Startup waterfall can mark
//! each unit's change against the previous boot. Boot identity is CONTENT
//! equality: a boot's critical-chain timings are static, so a re-delivered
//! identical timeline is the same boot (no append), and a changed timeline
//! is a new boot. Two boots with byte-identical timings would compare as
//! one — an honest, documented limitation (and a delta of exactly zero).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use taskmanager_core::BootTimeline;

use crate::HistoryStoreError;
use crate::bounded_io;

/// How many boots stay comparable (bounded by design).
pub const MAX_RECORDED_BOOTS: usize = 8;

/// Hard ceiling for the serialized boot-evidence document. Normal timelines
/// are a few KiB; this protects startup from an externally enlarged file or
/// an unexpectedly large provider identity.
pub const MAX_BOOT_HISTORY_BYTES: u64 = 2 * 1024 * 1024;

const BOOT_HISTORY_FILE: &str = "boot-evidence.json";

/// One stored boot: when it was first recorded plus its waterfall.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootTimelineRecord {
    pub recorded_at_ms: u64,
    pub timeline: BootTimeline,
}

/// What recording a timeline concluded about the boot session. Both variants
/// carry the comparison baseline for the CURRENT boot: on a new boot that is
/// the just-superseded last record, on a same-boot redelivery the record
/// before the last (the previous boot) — stable across repeated folds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordBootOutcome {
    SameBoot { previous: Option<BootTimeline> },
    NewBoot { previous: Option<BootTimeline> },
}

/// Append-mostly boot-timeline history over one JSON file. Constructed from
/// the same opt-in history root as the series store; a history that was
/// never enabled simply never gets one of these.
pub struct BootEvidenceHistory {
    path: PathBuf,
}

impl BootEvidenceHistory {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            path: root.into().join(BOOT_HISTORY_FILE),
        }
    }

    /// Record one observed boot waterfall (deduplicated by content).
    ///
    /// Returns the comparison baseline for the CURRENT boot: on a new boot
    /// that is the previous last record; on a same-boot redelivery it is the
    /// record before the last (the previous boot), staying stable across
    /// repeated evidence folds.
    pub fn record_boot(
        &self,
        timeline: &BootTimeline,
        recorded_at_ms: u64,
    ) -> Result<RecordBootOutcome, HistoryStoreError> {
        let mut records = self.read_records()?;
        if records
            .last()
            .is_some_and(|last| last.timeline.segments == timeline.segments)
        {
            // With a single record that record IS the current boot — there
            // is no previous boot, and the marker logic must not compare a
            // boot against itself.
            let previous = (records.len() >= 2)
                .then(|| records.get(records.len() - 2))
                .flatten()
                .map(|record| record.timeline.clone());
            return Ok(RecordBootOutcome::SameBoot { previous });
        }
        // Same-boot EVOLUTION: a provider retry can first report part of the
        // critical chain and later the complete one. If everything previously
        // recorded reappears with identical timings, this is the same boot
        // gaining detail — update the record in place (keeping its original
        // first-seen time) instead of fabricating a phantom boot that would
        // compare the boot against itself.
        if records
            .last()
            .is_some_and(|last| is_same_boot_evolution(&last.timeline, timeline))
        {
            let previous = (records.len() >= 2)
                .then(|| records.get(records.len() - 2))
                .flatten()
                .map(|record| record.timeline.clone());
            if let Some(last) = records.last_mut() {
                last.timeline = timeline.clone();
                self.write_records(&records)?;
            }
            return Ok(RecordBootOutcome::SameBoot { previous });
        }
        let previous = records.last().map(|record| record.timeline.clone());
        records.push(BootTimelineRecord {
            recorded_at_ms,
            timeline: timeline.clone(),
        });
        let overflow = records.len().saturating_sub(MAX_RECORDED_BOOTS);
        if overflow > 0 {
            records.drain(..overflow);
        }
        self.write_records(&records)?;
        Ok(RecordBootOutcome::NewBoot { previous })
    }

    /// Every stored boot, oldest first.
    pub fn boots(&self) -> Result<Vec<BootTimelineRecord>, HistoryStoreError> {
        self.read_records()
    }

    fn read_records(&self) -> Result<Vec<BootTimelineRecord>, HistoryStoreError> {
        match std::fs::metadata(&self.path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::Read,
                    format!("{}: {error}", self.path.display()),
                ));
            }
        }
        let bytes = bounded_io::read_file(&self.path, MAX_BOOT_HISTORY_BYTES)?;
        // A torn/corrupt file degrades to "no comparison history" — the
        // markers disappear for this run, the app keeps running.
        serde_json::from_slice(&bytes).map_err(|error| {
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Decode,
                format!("{}: {error}", self.path.display()),
            )
        })
    }

    fn write_records(&self, records: &[BootTimelineRecord]) -> Result<(), HistoryStoreError> {
        if let Some(parent) = self.path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            return Err(HistoryStoreError::new(
                crate::HistoryStoreErrorKind::CreateDirectory,
                format!("{}: {error}", parent.display()),
            ));
        }
        let temporary = self
            .path
            .with_extension(crate::store::temporary_extension("json"));
        let bytes = serde_json::to_vec(records).map_err(|error| {
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Encode,
                format!("{}: {error}", self.path.display()),
            )
        })?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BOOT_HISTORY_BYTES {
            return Err(HistoryStoreError::new(
                crate::HistoryStoreErrorKind::ResourceLimit,
                format!(
                    "{} exceeds the {} byte boot-history limit",
                    self.path.display(),
                    MAX_BOOT_HISTORY_BYTES
                ),
            ));
        }
        std::fs::write(&temporary, bytes).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Write,
                format!("{}: {error}", temporary.display()),
            )
        })?;
        std::fs::rename(&temporary, &self.path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Rename,
                format!(
                    "{} -> {}: {error}",
                    temporary.display(),
                    self.path.display()
                ),
            )
        })
    }
}

/// Whether `next` carries everything `last` recorded, unchanged (possibly
/// plus additional units): the same boot observed more completely.
fn is_same_boot_evolution(last: &BootTimeline, next: &BootTimeline) -> bool {
    last.segments.iter().all(|known| {
        next.segments.iter().any(|candidate| {
            candidate.unit == known.unit
                && candidate.start_ms == known.start_ms
                && candidate.duration_ms == known.duration_ms
        })
    })
}

/// Path of the backing file (tests + diagnostics).
#[must_use]
pub fn boot_history_path(root: &Path) -> PathBuf {
    root.join(BOOT_HISTORY_FILE)
}

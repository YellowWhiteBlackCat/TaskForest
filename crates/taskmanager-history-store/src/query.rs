//! The read half of the history store: windowed queries over the JSONL
//! directory, producing the core read models a history-mode surface renders.

use std::path::PathBuf;

use taskmanager_core::{HistoricalSeries, HistorySeriesKey, HistoryWindow, PeakSummary};

use crate::HistoryStoreError;
use crate::bounded_io;
use crate::records::decode_line;
use crate::{MAX_DIRECTORY_ENTRIES_PER_SCAN, MAX_SERIES_FILE_BYTES, MAX_SERIES_FILES};

const SERIES_EXTENSION: &str = "jsonl";

/// Read-only windowed query facade over one history directory.
#[derive(Clone)]
pub struct HistoryQuery {
    root: PathBuf,
}

/// One queried series plus the honesty ledger of its file: lines that could
/// not be decoded were skipped (and counted), never guessed.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesRead {
    pub series: HistoricalSeries,
    pub corrupt_lines: usize,
}

impl HistoryQuery {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Every series identity the directory holds, sorted by file stem.
    /// Malformed stems are skipped, not fatal. An externally enlarged
    /// directory fails with `ResourceLimit` instead of growing this result
    /// without bound.
    pub fn known_series(&self) -> Result<Vec<HistorySeriesKey>, HistoryStoreError> {
        let entries = std::fs::read_dir(&self.root).map_err(|error| {
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Read,
                format!("{}: {error}", self.root.display()),
            )
        })?;
        let mut keys = Vec::new();
        let mut file_count = 0usize;
        let mut entries_seen = 0usize;
        for entry in entries {
            let entry = entry.map_err(|error| {
                HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::Read,
                    format!("{}: {error}", self.root.display()),
                )
            })?;
            entries_seen = entries_seen.saturating_add(1);
            if entries_seen > MAX_DIRECTORY_ENTRIES_PER_SCAN {
                return Err(HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::ResourceLimit,
                    format!(
                        "{} contains more than {MAX_DIRECTORY_ENTRIES_PER_SCAN} directory entries",
                        self.root.display()
                    ),
                ));
            }
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| {
                HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::Read,
                    format!("{}: {error}", path.display()),
                )
            })?;
            if !file_type.is_file()
                || !path
                    .extension()
                    .is_some_and(|extension| extension == SERIES_EXTENSION)
            {
                continue;
            }
            file_count = file_count.saturating_add(1);
            if file_count > MAX_SERIES_FILES {
                return Err(HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::ResourceLimit,
                    format!(
                        "{} contains more than {MAX_SERIES_FILES} series files",
                        self.root.display()
                    ),
                ));
            }
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
                && let Some(key) = HistorySeriesKey::from_file_stem(stem)
            {
                keys.push(key);
            }
        }
        keys.sort_by_key(|key| key.file_stem());
        Ok(keys)
    }

    /// The samples of one series inside `window` ending at `now_ms`, in file
    /// (chronological) order. `Ok(None)` when the series has no file.
    /// Future-dated samples (a clock that stepped backwards) are kept — they
    /// are inside any "since" bound by definition — and surface through the
    /// series' `clock_jumps` count instead of being rewritten. The file is
    /// streamed line by line under the per-series byte ceiling, so peak
    /// memory follows the window, not the whole retained history.
    pub fn series(
        &self,
        key: &HistorySeriesKey,
        window: HistoryWindow,
        now_ms: u64,
    ) -> Result<Option<SeriesRead>, HistoryStoreError> {
        let path = self
            .root
            .join(format!("{}.{}", key.file_stem(), SERIES_EXTENSION));
        match std::fs::metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::Read,
                    format!("{}: {error}", path.display()),
                ));
            }
        }
        let floor = now_ms.saturating_sub(window.duration_ms());
        let mut samples = Vec::new();
        let mut corrupt_lines = 0usize;
        bounded_io::for_each_line_bounded(&path, MAX_SERIES_FILE_BYTES, |line| {
            if line.trim().is_empty() {
                return;
            }
            match decode_line(line) {
                Some(sample) if sample.completed_at_ms >= floor => samples.push(sample),
                Some(_) => {}
                None => corrupt_lines = corrupt_lines.saturating_add(1),
            }
        })?;
        Ok(Some(SeriesRead {
            series: HistoricalSeries::new(key.clone(), samples),
            corrupt_lines,
        }))
    }

    /// Fact-only peak summary for one series and window. `Ok(None)` when the
    /// series has no file; an empty/never-measured series yields a summary
    /// with `peak_value: None` rather than a fabricated zero.
    pub fn peak_summary(
        &self,
        key: &HistorySeriesKey,
        window: HistoryWindow,
        now_ms: u64,
    ) -> Result<Option<PeakSummary>, HistoryStoreError> {
        let Some(read) = self.series(key, window, now_ms)? else {
            return Ok(None);
        };
        let peak = read.series.peak();
        let observed_samples = read
            .series
            .samples
            .iter()
            .filter(|sample| !sample.is_gap())
            .count();
        Ok(Some(PeakSummary {
            key: key.clone(),
            window,
            peak_value: peak.and_then(|peak| peak.value),
            peak_measured_at_ms: peak.and_then(|peak| peak.measured_at_ms),
            observed_samples,
            gap_samples: read.series.gap_count(),
            clock_jumps: read.series.clock_jumps,
        }))
    }
}

//! Bounded JSONL append/rewrite and streaming retention reconciliation.

use std::collections::{HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use taskmanager_core::{HistoricalSample, HistorySeriesKey};

use super::pending::PendingSample;
use super::{PersistentHistoryStore, SERIES_EXTENSION, temporary_extension};
use crate::HistoryStoreError;
use crate::records::{decode_line, encode_line};
use crate::retention::{halve_newest, retain_by_ttl};

/// Maximum number of persisted JSONL series files.
pub const MAX_SERIES_FILES: usize = 1_024;
/// Maximum number of JSONL series files one retention pass will inspect.
/// One pass can reconcile a full prior-session set plus one full in-session
/// set; larger externally-created directories fail typed and require an
/// explicit operator cleanup rather than monopolizing a flush worker.
pub const MAX_SERIES_FILES_PER_SCAN: usize = MAX_SERIES_FILES * 2;
/// Total directory-entry work allowed per scan, including the lock, boot
/// evidence, abandoned temporary files and unrelated external debris.
/// Temporaries of provably dead writers are swept before every scan by the
/// stale-tmp hygiene pass; only foreign debris still requires an operator.
pub const MAX_DIRECTORY_ENTRIES_PER_SCAN: usize = MAX_SERIES_FILES_PER_SCAN + 64;
/// Hard bound for reading or writing one series file.
pub const MAX_SERIES_FILE_BYTES: u64 = 16 * 1024 * 1024;

impl PersistentHistoryStore {
    pub(super) fn append_samples(
        &self,
        key: &HistorySeriesKey,
        samples: &VecDeque<PendingSample>,
    ) -> Result<usize, HistoryStoreError> {
        let path = self.series_path(key);
        let samples = samples
            .iter()
            .map(|pending| pending.sample)
            .collect::<Vec<_>>();
        let buffer = encode_samples(&samples);
        let existing_bytes = match fs::metadata(&path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => {
                return Err(HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::Read,
                    format!("{}: {error}", path.display()),
                ));
            }
        };
        let appended_bytes = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if existing_bytes.saturating_add(appended_bytes) > MAX_SERIES_FILE_BYTES {
            let (mut combined, corrupt) = read_samples(&path)?;
            combined.extend(samples);
            let encoded_lengths = combined
                .iter()
                .map(encoded_sample_bytes)
                .collect::<Vec<_>>();
            let mut total = encoded_lengths.iter().copied().sum::<usize>();
            let maximum = usize::try_from(MAX_SERIES_FILE_BYTES).unwrap_or(usize::MAX);
            let mut keep_from = 0usize;
            while total > maximum && keep_from < encoded_lengths.len() {
                total = total.saturating_sub(encoded_lengths[keep_from]);
                keep_from = keep_from.saturating_add(1);
            }
            if keep_from == combined.len() && !combined.is_empty() {
                return Err(HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::ResourceLimit,
                    format!(
                        "one record for {} exceeds the {MAX_SERIES_FILE_BYTES} byte series limit",
                        path.display()
                    ),
                ));
            }
            self.rewrite_file(&path, &combined[keep_from..])?;
            return Ok(corrupt);
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| {
                HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::Open,
                    format!("{}: {error}", path.display()),
                )
            })?;
        file.write_all(buffer.as_bytes()).map_err(|error| {
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Write,
                format!("{}: {error}", path.display()),
            )
        })?;
        Ok(0)
    }

    fn series_path(&self, key: &HistorySeriesKey) -> PathBuf {
        self.root
            .join(format!("{}.{}", key.file_stem(), SERIES_EXTENSION))
    }

    /// TTL + quota pass over series files. TTL runs on its cadence; strict
    /// cardinality, per-file and global quota bounds run on every flush.
    pub(super) fn apply_retention(
        &self,
        now_ms: u64,
        run_ttl: bool,
        corrupt_skipped: &mut u64,
    ) -> Result<(usize, usize, Vec<HistorySeriesKey>), HistoryStoreError> {
        let mut descriptors = Vec::new();
        let mut retired_series = Vec::new();
        let mut ttl_trimmed = 0usize;
        let mut quota_trimmed_paths = HashSet::new();
        let inventory = series_inventory(&self.root)?;
        let inventory_bytes = inventory
            .iter()
            .fold(0u64, |total, file| total.saturating_add(file.bytes));
        if !run_ttl
            && inventory.len() <= MAX_SERIES_FILES
            && inventory_bytes <= self.policy.max_bytes
            && inventory
                .iter()
                .all(|file| file.bytes <= MAX_SERIES_FILE_BYTES)
        {
            return Ok((0, 0, retired_series));
        }
        for file in inventory {
            let path = file.path;
            let length = file.bytes;
            if length > MAX_SERIES_FILE_BYTES {
                retire_file(&path, &mut retired_series)?;
                quota_trimmed_paths.insert(path);
                continue;
            }
            let (samples, corrupt) = read_samples(&path)?;
            *corrupt_skipped =
                corrupt_skipped.saturating_add(u64::try_from(corrupt).unwrap_or(u64::MAX));
            let retained = if run_ttl {
                retain_by_ttl(&samples, now_ms, self.policy.ttl_ms)
            } else {
                samples.clone()
            };
            if retained.is_empty() {
                retire_file(&path, &mut retired_series)?;
                if run_ttl && !samples.is_empty() {
                    ttl_trimmed = ttl_trimmed.saturating_add(1);
                } else {
                    quota_trimmed_paths.insert(path);
                }
                continue;
            }
            if retained.len() != samples.len() {
                ttl_trimmed = ttl_trimmed.saturating_add(1);
                self.rewrite_file(&path, &retained)?;
            } else if corrupt > 0 {
                self.rewrite_file(&path, &retained)?;
                quota_trimmed_paths.insert(path.clone());
            }

            let bytes = file_length(&path)?;
            let descriptor = SeriesDescriptor {
                path,
                first_completed_ms: retained.first().map_or(0, |sample| sample.completed_at_ms),
                last_completed_ms: retained.last().map_or(0, |sample| sample.completed_at_ms),
                bytes,
            };
            retain_newest_descriptors(
                &mut descriptors,
                descriptor,
                &mut retired_series,
                &mut quota_trimmed_paths,
            )?;
        }

        enforce_byte_quota(
            self,
            &mut descriptors,
            &mut retired_series,
            &mut quota_trimmed_paths,
            corrupt_skipped,
        )?;
        Ok((ttl_trimmed, quota_trimmed_paths.len(), retired_series))
    }

    fn rewrite_file(
        &self,
        path: &Path,
        samples: &[HistoricalSample],
    ) -> Result<(), HistoryStoreError> {
        let temporary = path.with_extension(temporary_extension(SERIES_EXTENSION));
        let mut file = create_file(&temporary)?;
        let buffer = encode_samples(samples);
        if u64::try_from(buffer.len()).unwrap_or(u64::MAX) > MAX_SERIES_FILE_BYTES {
            let _ = fs::remove_file(&temporary);
            return Err(HistoryStoreError::new(
                crate::HistoryStoreErrorKind::ResourceLimit,
                format!(
                    "{} exceeds the {MAX_SERIES_FILE_BYTES} byte series limit",
                    path.display()
                ),
            ));
        }
        file.write_all(buffer.as_bytes()).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Write,
                format!("{}: {error}", temporary.display()),
            )
        })?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Rename,
                format!("{} -> {}: {error}", temporary.display(), path.display()),
            )
        })
    }
}

struct SeriesDescriptor {
    path: PathBuf,
    first_completed_ms: u64,
    last_completed_ms: u64,
    bytes: u64,
}

fn retain_newest_descriptors(
    descriptors: &mut Vec<SeriesDescriptor>,
    descriptor: SeriesDescriptor,
    retired_series: &mut Vec<HistorySeriesKey>,
    quota_trimmed_paths: &mut HashSet<PathBuf>,
) -> Result<(), HistoryStoreError> {
    if descriptors.len() < MAX_SERIES_FILES {
        descriptors.push(descriptor);
        return Ok(());
    }
    let oldest_index = descriptors
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| retention_age(left).cmp(&retention_age(right)))
        .map(|(index, _)| index)
        .unwrap_or(0);
    if retention_age(&descriptor) <= retention_age(&descriptors[oldest_index]) {
        retire_file(&descriptor.path, retired_series)?;
        quota_trimmed_paths.insert(descriptor.path);
    } else {
        let retired = std::mem::replace(&mut descriptors[oldest_index], descriptor);
        retire_file(&retired.path, retired_series)?;
        quota_trimmed_paths.insert(retired.path);
    }
    Ok(())
}

fn enforce_byte_quota(
    store: &PersistentHistoryStore,
    descriptors: &mut [SeriesDescriptor],
    retired_series: &mut Vec<HistorySeriesKey>,
    quota_trimmed_paths: &mut HashSet<PathBuf>,
    corrupt_skipped: &mut u64,
) -> Result<(), HistoryStoreError> {
    let mut total_bytes = descriptors.iter().fold(0u64, |total, descriptor| {
        total.saturating_add(descriptor.bytes)
    });
    descriptors.sort_by(|left, right| {
        left.first_completed_ms
            .cmp(&right.first_completed_ms)
            .then_with(|| left.path.cmp(&right.path))
    });
    for descriptor in descriptors {
        if total_bytes <= store.policy.max_bytes {
            break;
        }
        let (mut samples, corrupt) = read_samples(&descriptor.path)?;
        *corrupt_skipped =
            corrupt_skipped.saturating_add(u64::try_from(corrupt).unwrap_or(u64::MAX));
        while total_bytes > store.policy.max_bytes && samples.len() > 1 {
            samples = halve_newest(&samples);
            store.rewrite_file(&descriptor.path, &samples)?;
            let after = file_length(&descriptor.path)?;
            total_bytes = total_bytes
                .saturating_sub(descriptor.bytes)
                .saturating_add(after);
            descriptor.bytes = after;
            quota_trimmed_paths.insert(descriptor.path.clone());
        }
        if total_bytes > store.policy.max_bytes {
            retire_file(&descriptor.path, retired_series)?;
            total_bytes = total_bytes.saturating_sub(descriptor.bytes);
            descriptor.bytes = 0;
            quota_trimmed_paths.insert(descriptor.path.clone());
        }
    }
    Ok(())
}

fn retention_age(descriptor: &SeriesDescriptor) -> (u64, &Path) {
    (descriptor.last_completed_ms, &descriptor.path)
}

struct SeriesFile {
    path: PathBuf,
    bytes: u64,
}

fn series_inventory(root: &Path) -> Result<Vec<SeriesFile>, HistoryStoreError> {
    let entries = fs::read_dir(root).map_err(|error| {
        HistoryStoreError::new(
            crate::HistoryStoreErrorKind::Read,
            format!("{}: {error}", root.display()),
        )
    })?;
    let mut files = Vec::new();
    let mut entries_seen = 0usize;
    for entry in entries {
        let entry = entry.map_err(|error| {
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Read,
                format!("{}: {error}", root.display()),
            )
        })?;
        entries_seen = entries_seen.saturating_add(1);
        if entries_seen > MAX_DIRECTORY_ENTRIES_PER_SCAN {
            return Err(HistoryStoreError::new(
                crate::HistoryStoreErrorKind::ResourceLimit,
                format!(
                    "{} contains more than {MAX_DIRECTORY_ENTRIES_PER_SCAN} directory entries",
                    root.display()
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
        if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == SERIES_EXTENSION)
        {
            if files.len() >= MAX_SERIES_FILES_PER_SCAN {
                return Err(HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::ResourceLimit,
                    format!(
                        "{} contains more than {MAX_SERIES_FILES_PER_SCAN} scannable series files",
                        root.display()
                    ),
                ));
            }
            files.push(SeriesFile {
                bytes: file_length(&path)?,
                path,
            });
        }
    }
    Ok(files)
}

fn create_file(path: &Path) -> Result<fs::File, HistoryStoreError> {
    fs::File::create(path).map_err(|error| {
        HistoryStoreError::new(
            crate::HistoryStoreErrorKind::Open,
            format!("{}: {error}", path.display()),
        )
    })
}

fn file_length(path: &Path) -> Result<u64, HistoryStoreError> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| {
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::Read,
                format!("{}: {error}", path.display()),
            )
        })
}

fn retire_file(
    path: &Path,
    retired_series: &mut Vec<HistorySeriesKey>,
) -> Result<(), HistoryStoreError> {
    let key = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(HistorySeriesKey::from_file_stem);
    fs::remove_file(path).map_err(|error| {
        HistoryStoreError::new(
            crate::HistoryStoreErrorKind::Remove,
            format!("{}: {error}", path.display()),
        )
    })?;
    retired_series.extend(key);
    Ok(())
}

fn encoded_sample_bytes(sample: &HistoricalSample) -> usize {
    encode_line(sample).len().saturating_add(1)
}

fn encode_samples(samples: &[HistoricalSample]) -> String {
    let capacity = samples
        .iter()
        .map(encoded_sample_bytes)
        .fold(0usize, usize::saturating_add);
    let mut buffer = String::with_capacity(capacity);
    for sample in samples {
        buffer.push_str(&encode_line(sample));
        buffer.push('\n');
    }
    buffer
}

fn read_samples(path: &Path) -> Result<(Vec<HistoricalSample>, usize), HistoryStoreError> {
    let bytes = crate::bounded_io::read_file(path, MAX_SERIES_FILE_BYTES)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut samples = Vec::new();
    let mut corrupt = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match decode_line(line) {
            Some(sample) => samples.push(sample),
            None => corrupt = corrupt.saturating_add(1),
        }
    }
    Ok((samples, corrupt))
}

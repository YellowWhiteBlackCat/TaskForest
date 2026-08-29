//! Bounded Linux desktop-entry discovery keyed by `/proc/<pid>/exe` and argv.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use taskmanager_core::{
    FailureKind, ProcessApplicationIdentity, ProcessMetadataAvailability, ProcessMetadataFailure,
    ProcessMetadataObservation, SourceOutcome,
};

use super::PreviousProcessView;

mod catalog;
mod failures;
mod icons;
mod matching;

use catalog::*;
use failures::{classify_io, record_failure, shared_failure, stronger_metadata_failure};
use icons::resolve_icon_asset_from_dirs;
use matching::select_candidate;

const APPLICATION_CACHE_TTL_MS: u64 = 30_000;
const APPLICATION_CACHE_RETRY_MS: u64 = 5_000;
/// Only icons used by observed processes are resolved and retained. The
/// bounded byte budget prevents a desktop catalog with many large assets from
/// consuming the monitor's memory budget before the user opens a page.
const MAX_ICON_CACHE_BYTES: usize = 2 * 1024 * 1024;
const MAX_DESKTOP_FILES: usize = 4_096;
const MAX_DESKTOP_FILE_BYTES: u64 = 256 * 1024;

/// A parsed entry indexed by launcher identity; argv disambiguates shared browsers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CatalogEntry {
    identity: ProcessApplicationIdentity,
    executable: ExecutableSelector,
    exec_args: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct CachedIconResolution {
    asset: Option<taskmanager_core::ApplicationIconAsset>,
    failure: Option<ProcessMetadataFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutableSelector {
    /// Absolute `Exec=` paths take precedence over basename matching.
    path: Option<String>,
    basename: String,
    snap_package: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogMatch {
    NoEntry,
    InsufficientEvidence,
}

#[derive(Debug, Default)]
pub(super) struct ApplicationCatalog {
    loaded_at_ms: Option<u64>,
    entries: Vec<CatalogEntry>,
    failure: Option<ProcessMetadataFailure>,
    icon_dirs: Vec<PathBuf>,
    icon_cache: HashMap<String, CachedIconResolution>,
    icon_cache_bytes: usize,
}

impl ApplicationCatalog {
    /// Resolve an identity from executable and process argv without fabrication.
    pub(super) fn observe(
        &mut self,
        executable: &ProcessMetadataObservation<PathBuf>,
        argv: &[std::borrow::Cow<'_, str>],
        observed_at_ms: u64,
    ) -> (
        ProcessMetadataObservation<ProcessApplicationIdentity>,
        SourceOutcome,
    ) {
        let executable_failure = match executable.availability() {
            ProcessMetadataAvailability::Absent => {
                return (
                    ProcessMetadataObservation::absent(observed_at_ms),
                    SourceOutcome::Empty,
                );
            }
            ProcessMetadataAvailability::Unavailable(failure)
            | ProcessMetadataAvailability::Stale(failure) => {
                return (
                    ProcessMetadataObservation::unavailable(failure),
                    SourceOutcome::Unavailable(shared_failure(failure)),
                );
            }
            ProcessMetadataAvailability::Unknown => {
                return (
                    ProcessMetadataObservation::unavailable(ProcessMetadataFailure::ProviderFault),
                    SourceOutcome::Unavailable(FailureKind::ProviderFault),
                );
            }
            ProcessMetadataAvailability::Available => None,
            ProcessMetadataAvailability::Partial(failure) => Some(failure),
        };
        let Some(executable) = executable.current_value() else {
            return (
                ProcessMetadataObservation::unavailable(ProcessMetadataFailure::ProviderFault),
                SourceOutcome::Unavailable(FailureKind::ProviderFault),
            );
        };

        self.refresh_if_due(observed_at_ms);
        let matched = self.find_match(executable, argv);
        let identity = match matched {
            Ok(identity) => identity,
            Err(CatalogMatch::InsufficientEvidence) => {
                return (
                    ProcessMetadataObservation::unavailable(ProcessMetadataFailure::Unsupported),
                    SourceOutcome::Unavailable(FailureKind::Unsupported),
                );
            }
            Err(CatalogMatch::NoEntry) => {
                return match self.failure {
                    Some(failure) => (
                        ProcessMetadataObservation::unavailable(failure),
                        SourceOutcome::Unavailable(shared_failure(failure)),
                    ),
                    None if let Some(failure) = executable_failure => (
                        ProcessMetadataObservation::absent(observed_at_ms),
                        SourceOutcome::Partial(shared_failure(failure)),
                    ),
                    None => (
                        ProcessMetadataObservation::absent(observed_at_ms),
                        SourceOutcome::Empty,
                    ),
                };
            }
        };
        let identity = self.resolve_icon(identity);

        let partial_failure = [
            executable_failure,
            self.failure,
            identity.icon_failure,
            (!identity.has_icon_token()).then_some(ProcessMetadataFailure::NotFound),
        ]
        .into_iter()
        .flatten()
        .reduce(stronger_metadata_failure);
        match partial_failure {
            Some(failure) => (
                ProcessMetadataObservation::partial(identity, observed_at_ms, failure),
                SourceOutcome::Partial(shared_failure(failure)),
            ),
            None => (
                ProcessMetadataObservation::available(identity, observed_at_ms),
                SourceOutcome::Available,
            ),
        }
    }

    /// Resolve an identity, then revalidate the process start token.
    pub(super) fn observe_for_process(
        &mut self,
        pid: u32,
        expected_start_token: Result<u64, FailureKind>,
        executable: &ProcessMetadataObservation<PathBuf>,
        argv: &[std::borrow::Cow<'_, str>],
        observed_at_ms: u64,
    ) -> (
        ProcessMetadataObservation<ProcessApplicationIdentity>,
        SourceOutcome,
    ) {
        let expected_start_token = match expected_start_token {
            Ok(token) if token != 0 => token,
            Ok(_) | Err(_) => return pid_race_observation(),
        };
        let result = self.observe(executable, argv, observed_at_ms);
        if confirm_start_token(pid, expected_start_token).is_err() {
            return pid_race_observation();
        }
        result
    }

    fn find_match(
        &self,
        executable: &Path,
        argv: &[std::borrow::Cow<'_, str>],
    ) -> Result<ProcessApplicationIdentity, CatalogMatch> {
        let process_path = normalize_executable_path(executable);
        let process_basename = executable_key_from_path(executable).ok_or(CatalogMatch::NoEntry)?;

        let snap_package = process_path
            .as_deref()
            .and_then(snap_package_from_path)
            .or_else(|| snap_package_from_argv(argv));
        let candidates: Vec<_> = self
            .entries
            .iter()
            .filter_map(|entry| {
                executable_match_score(
                    &entry.executable,
                    process_path.as_deref(),
                    &process_basename,
                    snap_package.as_deref(),
                )
                .map(|score| (entry, score))
            })
            .collect();

        if candidates.is_empty() {
            if process_path
                .as_deref()
                .is_some_and(|path| appimage_mount_label(path).is_some())
            {
                // A mounted AppImage is known application-shaped evidence,
                // but an unmatched mount label cannot be assigned honestly.
                return Err(CatalogMatch::InsufficientEvidence);
            }
            return Err(CatalogMatch::NoEntry);
        }
        select_candidate(candidates, argv)
            .map(|entry| entry.identity.clone())
            .ok_or(CatalogMatch::InsufficientEvidence)
    }

    fn resolve_icon(&mut self, identity: ProcessApplicationIdentity) -> ProcessApplicationIdentity {
        let Some(token) = identity.icon_token.as_deref() else {
            return identity.with_icon_resolution(None, Some(ProcessMetadataFailure::NotFound));
        };
        // Test/embedded catalogs may provide parsed entries without any XDG
        // data roots. There is no icon source to classify in that state, so
        // preserve the matched identity without inventing a dependency
        // failure. A real catalog always records its discovered roots during
        // `refresh_if_due` before reaching this branch.
        if self.icon_dirs.is_empty() {
            return identity.with_icon_resolution(None, None);
        }
        if let Some(resolution) = self.icon_cache.get(token).cloned() {
            return identity.with_icon_resolution(resolution.asset, resolution.failure);
        }

        let (asset, failure) = resolve_icon_asset_from_dirs(&self.icon_dirs, Some(token));
        let resolution = CachedIconResolution {
            asset: asset.clone(),
            failure,
        };
        let bytes = asset.as_ref().map_or(0, |asset| asset.bytes.len());
        let retain = bytes > 0
            && bytes <= MAX_ICON_CACHE_BYTES
            && self.icon_cache_bytes.saturating_add(bytes) <= MAX_ICON_CACHE_BYTES;
        if retain {
            self.icon_cache_bytes = self.icon_cache_bytes.saturating_add(bytes);
            self.icon_cache.insert(token.to_owned(), resolution.clone());
        }
        if retain {
            identity.with_icon_resolution(resolution.asset, resolution.failure)
        } else {
            // Do not attach an uncached large asset to the current process
            // snapshot: doing so would defeat the catalog budget when many
            // distinct desktop icons are observed in one refresh. The process
            // identity remains valid and the UI uses its typed generic-icon
            // fallback until the bounded cache has room again.
            identity.with_icon_resolution(
                None,
                resolution
                    .failure
                    .or(Some(ProcessMetadataFailure::Unsupported)),
            )
        }
    }

    fn refresh_if_due(&mut self, observed_at_ms: u64) {
        if !self.loaded_at_ms.is_none_or(|loaded_at| {
            observed_at_ms.saturating_sub(loaded_at)
                >= if self.failure.is_some() {
                    APPLICATION_CACHE_RETRY_MS
                } else {
                    APPLICATION_CACHE_TTL_MS
                }
        }) {
            return;
        }
        let data_dirs = application_dirs();
        let (entries, failure) = load_catalog_from_dirs(&data_dirs);
        self.entries = entries;
        self.failure = failure;
        self.icon_dirs = data_dirs;
        self.icon_cache.clear();
        self.icon_cache_bytes = 0;
        self.loaded_at_ms = Some(observed_at_ms);
    }
}

/// Retain an application observation only for the same nonzero start token.
pub(super) fn retain_for_same_identity<P: PreviousProcessView + ?Sized>(
    current: ProcessMetadataObservation<ProcessApplicationIdentity>,
    current_start_token: Option<u64>,
    previous: Option<&P>,
) -> ProcessMetadataObservation<ProcessApplicationIdentity> {
    if current.availability()
        == ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PidRace)
    {
        // A race invalidates the previous PID association.
        return current;
    }
    let Some(current_start_token) = current_start_token.filter(|token| *token != 0) else {
        return current;
    };
    if previous
        .and_then(PreviousProcessView::current_start_token)
        .is_some_and(|previous_start_token| previous_start_token == current_start_token)
    {
        current.retain_previous(
            previous
                .map(|item| item.application_identity_observation().clone())
                .unwrap_or_default(),
        )
    } else {
        current
    }
}

fn pid_race_observation() -> (
    ProcessMetadataObservation<ProcessApplicationIdentity>,
    SourceOutcome,
) {
    (
        ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PidRace),
        SourceOutcome::Unavailable(FailureKind::IdentityChanged),
    )
}

fn confirm_start_token(pid: u32, expected_start_token: u64) -> Result<(), ProcessMetadataFailure> {
    let actual_start_token = super::procfs::read_proc_stat(pid)
        .map_err(ProcessMetadataFailure::from_inventory_failure)?
        .start_ticks;
    if actual_start_token == expected_start_token {
        Ok(())
    } else {
        Err(ProcessMetadataFailure::PidRace)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/engine/process/application.rs"]
mod tests;

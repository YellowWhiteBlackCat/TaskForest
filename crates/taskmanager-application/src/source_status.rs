//! Toolkit-neutral projection of source failures into honest recovery state.
//!
//! A composite snapshot may still contain useful rows while one provider is
//! partial, so consumers need a distinction between "empty" and "degraded".
//! This module keeps that distinction and reuses the platform failure policy
//! to decide whether an immediate refresh is meaningful. It deliberately does
//! not choose a page or a widget: the caller supplies the independently scoped
//! [`crate::RefreshRequest`] at the presentation boundary.

use std::iter::once;

use taskmanager_core::{
    DeviceState, DeviceStatus, FailureKind, ProviderId, SourceOutcome, SourceStatus,
};
use taskmanager_platform_contract::{OperationFailure, ProviderFailure, RetryDisposition};

/// The headline failure represented by one or more source statuses.
///
/// `Unavailable` takes precedence over `Partial` because it means a provider
/// did not answer at all. The first failure of the selected severity is kept,
/// preserving provider order for deterministic copy and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceNotice {
    Partial(FailureKind),
    Unavailable(FailureKind),
}

impl SourceNotice {
    /// The platform-neutral failure payload that produced this notice.
    #[must_use]
    pub const fn failure(self) -> FailureKind {
        match self {
            Self::Partial(kind) | Self::Unavailable(kind) => kind,
        }
    }

    /// The honest retry policy for this source failure.
    #[must_use]
    pub const fn retry(self) -> RetryDisposition {
        ProviderFailure::from_kind(self.failure()).retry()
    }

    /// Whether a frontend may offer an immediate refresh affordance.
    ///
    /// `AfterCapabilityChange` is intentionally not included: presenting a
    /// retry button before permissions/dependencies change only creates a
    /// loop and falsely suggests that the app can repair the provider.
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(
            self.retry(),
            RetryDisposition::RetryNow | RetryDisposition::RetryLater
        )
    }
}

/// Toolkit-neutral display state of one data source.
///
/// The kind is the only part a frontend needs for tone (color, icon, label
/// family); localized copy and palette stay toolkit-side. It normalizes both
/// input families:
///
/// * snapshot outcomes ([`SourceStatus`]): `Available`/`Empty` → `Ok` (a
///   confirmed empty answer is healthy, not a gap), `Partial` → `Degraded`,
///   `Unavailable` → `Failed`, or `Stale` when previously collected rows are
///   still visible (`item_count > 0`).
/// * runtime provider health ([`DeviceState`]): `Healthy` → `Ok`, `Stale` →
///   `Stale`, `PermissionDenied` → `Failed`, `MissingTool` → `Degraded` (a
///   warning, not a failure: the surrounding surface still renders usable
///   data), `Unsupported` → `Unknown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceStateKind {
    /// The source answered; zero items is a confirmed answer, not a gap.
    Ok,
    /// The answer is incomplete or conditionally frozen: some rows arrived,
    /// or the provider cannot refresh while other content still renders.
    Degraded,
    /// The source did not answer, but previously collected rows are still
    /// visible — show them as stale, never as fresh.
    Stale,
    /// The source did not answer and nothing usable replaces it.
    Failed,
    /// No verdict is possible here (unsupported / unexpected payload).
    Unknown,
}

impl SourceStateKind {
    /// Merge precedence for a page-level headline over several sources.
    ///
    /// `Failed` and `Stale` share the top rank — both mean the provider did
    /// not answer, so the first reporter wins and provider order stays
    /// deterministic. `Degraded` outranks `Unknown` because a partial outage
    /// is actionable headline material while an unsupported source is
    /// background information.
    #[must_use]
    pub const fn merge_rank(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Unknown => 1,
            Self::Degraded => 2,
            Self::Stale | Self::Failed => 3,
        }
    }

    /// Classify a runtime provider/device status into the neutral kind.
    ///
    /// The typed failure payload (`DeviceStatus::failure`) travels alongside
    /// in [`SourceLineProjection::failure`] so a frontend can resolve
    /// per-cause copy without re-classifying.
    #[must_use]
    pub const fn from_device_status(status: DeviceStatus) -> Self {
        match status {
            DeviceStatus::Healthy => Self::Ok,
            DeviceStatus::Stale => Self::Stale,
            DeviceStatus::PermissionDenied => Self::Failed,
            DeviceStatus::MissingTool => Self::Degraded,
            DeviceStatus::Unsupported => Self::Unknown,
        }
    }
}

/// One source's display line, toolkit-neutral: who (`origin`), how bad
/// (`state`), and the typed cause (`failure`) for the frontend's copy table.
/// The row count is carried for "stale rows still visible" surfaces; runtime
/// device lines have no count and keep `0`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLineProjection {
    /// Provider identity exactly as diagnostics print it (`ProviderId` text).
    pub origin: String,
    pub state: SourceStateKind,
    /// Typed failure payload for `Degraded`/`Stale`/`Failed` (and the reason
    /// an `Unknown` line is unknown, normally `Unsupported`); `None` for `Ok`.
    pub failure: Option<FailureKind>,
    /// Rows this source still shows; `0` for runtime device lines.
    pub item_count: usize,
}

/// Fold one snapshot source status into its display line.
#[must_use]
pub fn source_line(status: &SourceStatus) -> SourceLineProjection {
    let (state, failure) = match status.outcome {
        SourceOutcome::Available | SourceOutcome::Empty => (SourceStateKind::Ok, None),
        SourceOutcome::Partial(kind) => (SourceStateKind::Degraded, Some(kind)),
        SourceOutcome::Unavailable(kind) => (
            if status.item_count > 0 {
                SourceStateKind::Stale
            } else {
                SourceStateKind::Failed
            },
            Some(kind),
        ),
    };
    SourceLineProjection {
        origin: status.provider.as_str().to_string(),
        state,
        failure,
        item_count: status.item_count,
    }
}

/// Fold a composite observation's sources in stable provider order — the
/// order in is the order out, so page copy never re-sorts between frames.
#[must_use]
pub fn source_lines(sources: &[SourceStatus]) -> Vec<SourceLineProjection> {
    sources.iter().map(source_line).collect()
}

/// Fold one runtime provider state into its display line. `item_count` is a
/// snapshot-outcome concept and stays `0` here.
#[must_use]
pub fn device_source_line(provider: &ProviderId, state: &DeviceState) -> SourceLineProjection {
    SourceLineProjection {
        origin: provider.as_str().to_string(),
        state: SourceStateKind::from_device_status(state.status),
        failure: state.status.failure(),
        item_count: 0,
    }
}

/// The headline state of a composite observation: which kind drives tone and
/// copy family, plus the existing [`SourceNotice`] for the typed failure and
/// retry policy.
///
/// The winner is the first source at the highest [`SourceStateKind::merge_rank`]
/// — the single merge rule; [`source_notice`] is this fold read through its
/// `notice` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergedSourceState {
    pub kind: SourceStateKind,
    pub notice: SourceNotice,
}

/// Select the most informative source state for a page-level headline.
///
/// `None` means every source is healthy or confirmed empty, or no source has
/// reported yet.
#[must_use]
pub fn merge_source_lines(sources: &[SourceStatus]) -> Option<MergedSourceState> {
    let mut merged: Option<MergedSourceState> = None;
    for status in sources {
        let line = source_line(status);
        let Some(failure) = line.failure else {
            continue;
        };
        let notice = match line.state {
            SourceStateKind::Degraded => SourceNotice::Partial(failure),
            SourceStateKind::Stale | SourceStateKind::Failed => SourceNotice::Unavailable(failure),
            // A healthy or unclassifiable line never headlines a failure banner.
            SourceStateKind::Ok | SourceStateKind::Unknown => continue,
        };
        if merged
            .as_ref()
            .is_none_or(|current| line.state.merge_rank() > current.kind.merge_rank())
        {
            merged = Some(MergedSourceState {
                kind: line.state,
                notice,
            });
        }
    }
    merged
}

/// Char-boundary truncation rule shared by every compact source/detail line
/// (provider ids, boot args, diagnostics fragments).
///
/// Text at or under `max_chars` passes through unchanged. An over-long text is
/// cut to `max_chars - 1` chars plus `…`, so the result never exceeds
/// `max_chars` chars and never splits a codepoint. `max_chars == 0` yields the
/// empty string — not even the ellipsis fits.
#[must_use]
pub fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    text.chars().take(max_chars - 1).chain(once('…')).collect()
}

/// Select the most informative source failure from a composite observation.
///
/// `None` means every source is healthy/confirmed empty, or no source has
/// reported yet. The result is suitable both for an inline degraded banner
/// when rows exist and for an unavailable empty state when they do not. This
/// is [`merge_source_lines`] read through its `notice` field — one fold, two
/// readouts.
#[must_use]
pub fn source_notice(sources: &[SourceStatus]) -> Option<SourceNotice> {
    merge_source_lines(sources).map(|merged| merged.notice)
}

/// Turn an accepted inventory operation failure into source truth when the
/// provider could not publish a snapshot at all. A later successful snapshot
/// replaces this synthetic status; `item_count` lets a frontend distinguish a
/// stale-but-visible list from an initial empty response without fabricating a
/// successful source outcome.
#[must_use]
pub fn source_status_from_operation_failure(
    failure: &OperationFailure,
    item_count: usize,
) -> SourceStatus {
    SourceStatus {
        provider: failure
            .provider
            .clone()
            .unwrap_or_else(|| taskmanager_core::ProviderId::borrowed("platform.runtime")),
        outcome: SourceOutcome::Unavailable(failure.kind),
        item_count,
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_source_status_tests.rs"]
mod tests;

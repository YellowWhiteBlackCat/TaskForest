//! Discovery lifecycle authority derived from aggregate source truth.
//!
//! Empty and available observations authorize a refresh; partial or unavailable
//! collection never declares a previously known physical device absent.

use taskmanager_application::{SourceOutcome, SourceStatus};
use taskmanager_core::{DeviceRefreshOutcome, DeviceStatus};

/// Convert aggregate source truth into lifecycle authority.
///
/// Empty and available observations are authoritative. Partial or unavailable
/// collection must never declare a previously known physical device absent.
#[must_use]
pub fn discovery_refresh_outcome(sources: &[SourceStatus]) -> DeviceRefreshOutcome {
    if sources.is_empty() {
        return DeviceRefreshOutcome::Unavailable(DeviceStatus::Stale);
    }
    let mut failure_statuses = sources.iter().filter_map(|source| match source.outcome {
        SourceOutcome::Available | SourceOutcome::Empty => None,
        SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => {
            Some(DeviceStatus::from_failure(failure))
        }
    });
    let Some(first) = failure_statuses.next() else {
        return DeviceRefreshOutcome::Complete;
    };
    let status = failure_statuses.fold(first, |current, candidate| {
        if candidate.severity() > current.severity() {
            candidate
        } else {
            current
        }
    });
    DeviceRefreshOutcome::Unavailable(status)
}

#[cfg(test)]
#[path = "../tests/headless/runtime_lifecycle_tests.rs"]
mod tests;

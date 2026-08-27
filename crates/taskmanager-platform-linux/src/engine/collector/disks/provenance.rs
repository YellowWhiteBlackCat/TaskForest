//! Typed source failure aggregation and sysfs metadata readers.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use taskmanager_core::ScalarObservation;
use taskmanager_platform_contract::{FailureKind, SourceOutcome};

#[derive(Debug, Default)]
pub(super) struct SourceFailures {
    pub(super) strongest: Option<FailureKind>,
}

impl SourceFailures {
    pub(super) fn record(&mut self, failure: FailureKind) {
        if self
            .strongest
            .is_none_or(|current| failure_priority(failure) > failure_priority(current))
        {
            self.strongest = Some(failure);
        }
    }

    pub(super) fn record_io(&mut self, error: &std::io::Error) {
        self.record(io_failure_kind(error));
    }

    pub(super) fn outcome(&self, item_count: usize) -> SourceOutcome {
        match (item_count, self.strongest) {
            (0, None) => SourceOutcome::Empty,
            (_, None) => SourceOutcome::Available,
            (0, Some(failure)) => SourceOutcome::Unavailable(failure),
            (_, Some(failure)) => SourceOutcome::Partial(failure),
        }
    }
}

pub(super) fn read_optional_text(path: &Path, failures: &mut SourceFailures) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(value) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            failures.record_io(&error);
            None
        }
    }
}

pub(super) fn first_optional_text(
    paths: &[PathBuf],
    failures: &mut SourceFailures,
) -> Option<String> {
    paths
        .iter()
        .find_map(|path| read_optional_text(path, failures))
}

pub(super) fn observe_required_sector_bytes(
    path: &Path,
    now_ms: u64,
    failures: &mut SourceFailures,
) -> ScalarObservation<u64> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            let failure = if error.kind() == ErrorKind::NotFound {
                FailureKind::ProviderFault
            } else {
                io_failure_kind(&error)
            };
            failures.record(failure);
            return ScalarObservation::unavailable(failure);
        }
    };
    let sectors = match raw.trim().parse::<u64>() {
        Ok(sectors) => sectors,
        Err(_) => {
            failures.record(FailureKind::ProviderFault);
            return ScalarObservation::unavailable(FailureKind::ProviderFault);
        }
    };
    let Some(bytes) = sectors.checked_mul(512) else {
        failures.record(FailureKind::ProviderFault);
        return ScalarObservation::unavailable(FailureKind::ProviderFault);
    };
    ScalarObservation::available(bytes, now_ms)
}

pub(super) fn read_optional_bit(path: &Path, failures: &mut SourceFailures) -> Option<bool> {
    let raw = read_optional_text(path, failures)?;
    match raw.as_str() {
        "0" => Some(false),
        "1" => Some(true),
        _ => {
            failures.record(FailureKind::ProviderFault);
            None
        }
    }
}

pub(super) fn read_optional_canonical_basename(
    path: &Path,
    failures: &mut SourceFailures,
) -> Option<String> {
    match fs::canonicalize(path) {
        Ok(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            failures.record_io(&error);
            None
        }
    }
}

pub(super) fn io_failure_kind(error: &std::io::Error) -> FailureKind {
    match error.kind() {
        ErrorKind::NotFound => FailureKind::Unsupported,
        ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        ErrorKind::TimedOut => FailureKind::TimedOut,
        _ => FailureKind::ProviderFault,
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::IdentityChanged => 3,
        FailureKind::Rejected => 2,
        FailureKind::Unsupported => 1,
    }
}

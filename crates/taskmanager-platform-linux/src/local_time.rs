//! Linux local-time rule discovery at the native platform boundary.

use std::io::{ErrorKind, Read};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use taskmanager_core::{
    FailureKind, LocalTimeRules, LocalTimeRulesObservation, MAX_LOCAL_TIME_RULE_BYTES, unix_millis,
};

const SYSTEM_LOCALTIME: &str = "/etc/localtime";
const ZONEINFO_ROOT: &str = "/usr/share/zoneinfo";

/// Read and validate the Linux process's configured local-time rules.
///
/// This is called by the native composition root, never a renderer. `TZ=""`
/// is an explicit fixed-UTC rule. Missing, denied, malformed, and unsupported
/// rules stay distinguishable failures; none falls back to UTC.
#[must_use]
pub fn local_time_rules() -> LocalTimeRulesObservation {
    let observed_at_ms = unix_millis(SystemTime::now());
    let path = match std::env::var_os("TZ") {
        None => PathBuf::from(SYSTEM_LOCALTIME),
        Some(value) if value.is_empty() => {
            return LocalTimeRulesObservation::current(LocalTimeRules::utc(), observed_at_ms);
        }
        Some(value) => {
            let Some(value) = value.to_str() else {
                return LocalTimeRulesObservation::unavailable(
                    FailureKind::Unsupported,
                    observed_at_ms,
                );
            };
            let value = value.strip_prefix(':').unwrap_or(value);
            if value.is_empty() {
                return LocalTimeRulesObservation::current(LocalTimeRules::utc(), observed_at_ms);
            }
            let requested = Path::new(value);
            if requested.is_absolute() {
                requested.to_path_buf()
            } else if requested
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            {
                Path::new(ZONEINFO_ROOT).join(requested)
            } else {
                return LocalTimeRulesObservation::unavailable(
                    FailureKind::Rejected,
                    observed_at_ms,
                );
            }
        }
    };

    let bytes = match read_bounded(&path) {
        Ok(bytes) => bytes,
        Err(LocalTimeReadError::NotRegular | LocalTimeReadError::TooLarge) => {
            return LocalTimeRulesObservation::unavailable(FailureKind::Rejected, observed_at_ms);
        }
        Err(LocalTimeReadError::Io(error)) => {
            let failure = match error.kind() {
                ErrorKind::NotFound => FailureKind::MissingDependency,
                ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
                _ => FailureKind::TemporarilyUnavailable,
            };
            return LocalTimeRulesObservation::unavailable(failure, observed_at_ms);
        }
    };
    match LocalTimeRules::from_tzif(&bytes) {
        Ok(rules) => LocalTimeRulesObservation::current(rules, observed_at_ms),
        Err(_) => {
            LocalTimeRulesObservation::unavailable(FailureKind::ProviderFault, observed_at_ms)
        }
    }
}

#[derive(Debug)]
enum LocalTimeReadError {
    Io(std::io::Error),
    NotRegular,
    TooLarge,
}

impl From<std::io::Error> for LocalTimeReadError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, LocalTimeReadError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(LocalTimeReadError::NotRegular);
    }
    if metadata.len() > u64::try_from(MAX_LOCAL_TIME_RULE_BYTES).unwrap_or(u64::MAX) {
        return Err(LocalTimeReadError::TooLarge);
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(nix::libc::O_NONBLOCK);
    let file = options.open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(LocalTimeReadError::NotRegular);
    }
    let limit = u64::try_from(MAX_LOCAL_TIME_RULE_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_LOCAL_TIME_RULE_BYTES {
        return Err(LocalTimeReadError::TooLarge);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "../tests/headless/linux_local_time_tests.rs"]
mod tests;

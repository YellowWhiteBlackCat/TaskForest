//! Linux acquisition and classification for typed process metadata.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use taskmanager_core::{
    FailureKind, ProcessMetadataFailure, ProcessMetadataObservation, ProcessMetadataObservations,
    ProcessOwner, ProcessOwnerIdentity, SourceOutcome,
};
use tracing::warn;

use super::PreviousProcessView;

pub(super) type PasswdLabels = Result<HashMap<u32, String>, ProcessMetadataFailure>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessMetadataEvidence {
    pub(super) owner_identity: SourceOutcome,
    pub(super) owner_label: SourceOutcome,
    pub(super) executable_path: SourceOutcome,
}

pub(super) fn load_passwd_labels() -> PasswdLabels {
    fs::read_to_string("/etc/passwd").map_or_else(
        |error| {
            warn!(%error, "failed to read /etc/passwd for process owner labels");
            Err(classify_metadata_io(&error))
        },
        |text| Ok(parse_passwd_labels(&text)),
    )
}

pub(super) fn observe_process_metadata<P: PreviousProcessView + ?Sized>(
    pid: u32,
    passwd: &PasswdLabels,
    observed_at_ms: u64,
    current_start_token: Result<u64, FailureKind>,
    previous: Option<&P>,
) -> (ProcessMetadataObservations, ProcessMetadataEvidence) {
    let current_start_token = match current_start_token {
        Ok(token) => token,
        Err(failure) => return unavailable_for_identity_failure(failure),
    };
    let uid = read_proc_status_uid(pid);
    let executable_path = read_executable_path(pid, observed_at_ms);
    let evidence = ProcessMetadataEvidence {
        owner_identity: result_outcome(&uid),
        owner_label: label_outcome(&uid, passwd),
        executable_path: observation_outcome(&executable_path),
    };
    let current = observations_from_results(uid, passwd, executable_path, observed_at_ms);
    (
        retain_for_same_identity(current, Some(current_start_token), previous),
        evidence,
    )
}

fn unavailable_for_identity_failure(
    failure: FailureKind,
) -> (ProcessMetadataObservations, ProcessMetadataEvidence) {
    let metadata_failure = ProcessMetadataFailure::from_inventory_failure(failure);
    let unavailable = SourceOutcome::Unavailable(failure);
    (
        ProcessMetadataObservations {
            owner: ProcessMetadataObservation::unavailable(metadata_failure),
            executable_path: ProcessMetadataObservation::unavailable(metadata_failure),
        },
        ProcessMetadataEvidence {
            owner_identity: unavailable,
            owner_label: unavailable,
            executable_path: unavailable,
        },
    )
}

fn retain_for_same_identity<P: PreviousProcessView + ?Sized>(
    current: ProcessMetadataObservations,
    current_start_token: Option<u64>,
    previous: Option<&P>,
) -> ProcessMetadataObservations {
    let Some(previous) = previous.filter(|previous| {
        current_start_token.is_some_and(|token| previous.current_start_token() == Some(token))
    }) else {
        return current;
    };
    ProcessMetadataObservations {
        owner: current
            .owner
            .retain_previous(previous.metadata_observations().owner.clone()),
        executable_path: current
            .executable_path
            .retain_previous(previous.metadata_observations().executable_path.clone()),
    }
}

fn observations_from_results(
    uid: Result<u32, ProcessMetadataFailure>,
    passwd: &PasswdLabels,
    executable_path: ProcessMetadataObservation<PathBuf>,
    observed_at_ms: u64,
) -> ProcessMetadataObservations {
    ProcessMetadataObservations {
        owner: owner_observation(uid, passwd, observed_at_ms),
        executable_path,
    }
}

fn owner_observation(
    uid: Result<u32, ProcessMetadataFailure>,
    passwd: &PasswdLabels,
    observed_at_ms: u64,
) -> ProcessMetadataObservation<ProcessOwner> {
    let uid = match uid {
        Ok(uid) => uid,
        Err(failure) => return ProcessMetadataObservation::unavailable(failure),
    };
    let label = passwd
        .as_ref()
        .ok()
        .and_then(|labels| labels.get(&uid))
        .cloned();
    let owner = ProcessOwner {
        identity: ProcessOwnerIdentity::Numeric(u64::from(uid)),
        label,
    };
    match passwd {
        Ok(_) => ProcessMetadataObservation::available(owner, observed_at_ms),
        Err(failure) => ProcessMetadataObservation::partial(owner, observed_at_ms, *failure),
    }
}

fn parse_status_uid(text: &str) -> Result<u32, ProcessMetadataFailure> {
    text.lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|values| values.split_whitespace().next())
        .ok_or(ProcessMetadataFailure::ProviderFault)?
        .parse()
        .map_err(|_| ProcessMetadataFailure::ProviderFault)
}

fn read_proc_status_uid(pid: u32) -> Result<u32, ProcessMetadataFailure> {
    let path = format!("/proc/{pid}/status");
    let text = fs::read_to_string(path).map_err(|error| classify_process_io(&error))?;
    parse_status_uid(&text)
}

fn parse_passwd_labels(text: &str) -> HashMap<u32, String> {
    let mut labels = HashMap::new();
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let mut fields = line.split(':');
        let Some(name) = fields.next().filter(|name| !name.is_empty()) else {
            continue;
        };
        let _password = fields.next();
        let Some(uid) = fields.next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        labels.entry(uid).or_insert_with(|| name.to_owned());
    }
    labels
}

fn read_executable_path(pid: u32, observed_at_ms: u64) -> ProcessMetadataObservation<PathBuf> {
    let executable = fs::read_link(format!("/proc/{pid}/exe"));
    if executable
        .as_ref()
        .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
    {
        let process_dir = fs::metadata(format!("/proc/{pid}")).map(|_| ());
        executable_observation(executable, Some(process_dir), observed_at_ms)
    } else {
        executable_observation(executable, None, observed_at_ms)
    }
}

fn executable_observation(
    executable: io::Result<PathBuf>,
    process_dir_after_not_found: Option<io::Result<()>>,
    observed_at_ms: u64,
) -> ProcessMetadataObservation<PathBuf> {
    match executable {
        Ok(path) => ProcessMetadataObservation::available(path, observed_at_ms),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match process_dir_after_not_found {
                Some(Ok(())) => ProcessMetadataObservation::absent(observed_at_ms),
                Some(Err(error)) if error.kind() == io::ErrorKind::NotFound => {
                    ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PidRace)
                }
                Some(Err(error)) => {
                    ProcessMetadataObservation::unavailable(classify_metadata_io(&error))
                }
                None => {
                    ProcessMetadataObservation::unavailable(ProcessMetadataFailure::ProviderFault)
                }
            }
        }
        Err(error) => ProcessMetadataObservation::unavailable(classify_process_io(&error)),
    }
}

fn classify_metadata_io(error: &io::Error) -> ProcessMetadataFailure {
    match error.kind() {
        io::ErrorKind::NotFound => ProcessMetadataFailure::NotFound,
        io::ErrorKind::PermissionDenied => ProcessMetadataFailure::PermissionDenied,
        io::ErrorKind::Unsupported => ProcessMetadataFailure::Unsupported,
        _ => ProcessMetadataFailure::ProviderFault,
    }
}

fn classify_process_io(error: &io::Error) -> ProcessMetadataFailure {
    if error.kind() == io::ErrorKind::NotFound {
        ProcessMetadataFailure::PidRace
    } else {
        classify_metadata_io(error)
    }
}

fn result_outcome<T>(result: &Result<T, ProcessMetadataFailure>) -> SourceOutcome {
    match result {
        Ok(_) => SourceOutcome::Available,
        Err(failure) => SourceOutcome::Unavailable(shared_failure(*failure)),
    }
}

fn label_outcome(
    uid: &Result<u32, ProcessMetadataFailure>,
    passwd: &PasswdLabels,
) -> SourceOutcome {
    let uid = match uid {
        Ok(uid) => uid,
        Err(failure) => return SourceOutcome::Unavailable(shared_failure(*failure)),
    };
    match passwd {
        Ok(labels) if labels.contains_key(uid) => SourceOutcome::Available,
        Ok(_) => SourceOutcome::Empty,
        Err(failure) => SourceOutcome::Unavailable(shared_failure(*failure)),
    }
}

fn observation_outcome<T>(observation: &ProcessMetadataObservation<T>) -> SourceOutcome {
    use taskmanager_core::ProcessMetadataAvailability;

    match observation.availability() {
        ProcessMetadataAvailability::Available => SourceOutcome::Available,
        ProcessMetadataAvailability::Partial(failure) => {
            SourceOutcome::Partial(shared_failure(failure))
        }
        ProcessMetadataAvailability::Absent => SourceOutcome::Empty,
        ProcessMetadataAvailability::Stale(failure)
        | ProcessMetadataAvailability::Unavailable(failure) => {
            SourceOutcome::Unavailable(shared_failure(failure))
        }
        ProcessMetadataAvailability::Unknown => {
            SourceOutcome::Unavailable(taskmanager_core::FailureKind::ProviderFault)
        }
    }
}

const fn shared_failure(failure: ProcessMetadataFailure) -> taskmanager_core::FailureKind {
    use taskmanager_core::FailureKind;

    match failure {
        ProcessMetadataFailure::Unsupported => FailureKind::Unsupported,
        ProcessMetadataFailure::PermissionDenied => FailureKind::PermissionDenied,
        ProcessMetadataFailure::NotFound => FailureKind::MissingDependency,
        ProcessMetadataFailure::PidRace => FailureKind::IdentityChanged,
        ProcessMetadataFailure::ProviderFault => FailureKind::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_metadata_tests.rs"]
mod tests;

//! Fail-closed resolution of generation-bound Linux storage command targets.
//!
//! The telemetry collector is the sole publisher because its sysfs inventory
//! is the authoritative source for whole-device presence. SMART workers receive
//! a read-only resolver and never infer identity from a cached `/dev` path.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use taskmanager_core::core::device_state::stable_disk_id;
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::{
    DeviceGeneration, DeviceId, DiskMetrics, FailureKind, SourceOutcome, StorageConnection,
    StorageDeviceKey, StorageDeviceKind, StorageDeviceTarget, StorageIdentityStability,
};
use taskmanager_platform_contract::ProviderFailure;

const STORAGE_IDENTITY_METADATA_PROVIDER: &str = "linux.storage.sysfs.metadata";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedStorageTarget {
    pub(crate) device_id: DeviceId,
    pub(crate) device_generation: DeviceGeneration,
    pub(crate) locator: StorageDeviceKey,
    pub(crate) connection: StorageConnection,
    pub(crate) identity_stability: StorageIdentityStability,
}

#[derive(Clone)]
pub(crate) struct StorageTargetRegistry {
    shared: Arc<Mutex<RegistryState>>,
}

#[derive(Clone)]
pub(crate) struct StorageTargetResolver {
    shared: Arc<Mutex<RegistryState>>,
}

#[derive(Clone)]
pub(crate) struct LiveStorageTargetVerifier {
    sysfs_block_root: Arc<PathBuf>,
}

enum RegistryState {
    Unavailable(ProviderFailure),
    Authoritative(HashMap<DeviceId, RegistryEntry>),
}

enum RegistryEntry {
    Unique(ResolvedStorageTarget),
    Ambiguous,
    Unverifiable,
}

impl StorageTargetRegistry {
    pub(crate) fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(RegistryState::Unavailable(
                ProviderFailure::TemporarilyUnavailable,
            ))),
        }
    }

    pub(crate) fn resolver(&self) -> StorageTargetResolver {
        StorageTargetResolver {
            shared: self.shared.clone(),
        }
    }

    /// Replace the complete resolver snapshot after one sysfs discovery.
    ///
    /// Partial and unavailable observations deliberately discard the last good
    /// map. A cached native locator is never safe mutation authority.
    pub(crate) fn publish(
        &self,
        disks: &[DiskMetrics],
        discovery: SourceOutcome,
        identity_metadata: SourceOutcome,
    ) {
        let next = match authority_failure(discovery, identity_metadata) {
            None => RegistryState::Authoritative(authoritative_entries(disks)),
            Some(failure) => RegistryState::Unavailable(provider_failure(failure)),
        };
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state = next;
    }
}

impl StorageTargetResolver {
    pub(crate) fn resolve(
        &self,
        target: &StorageDeviceTarget,
    ) -> Result<ResolvedStorageTarget, ProviderFailure> {
        let state = self
            .shared
            .lock()
            .map_err(|_| ProviderFailure::ProviderFault)?;
        let entries = match &*state {
            RegistryState::Unavailable(failure) => return Err(*failure),
            RegistryState::Authoritative(entries) => entries,
        };
        let entry = entries
            .get(&target.device_id)
            .ok_or(ProviderFailure::IdentityChanged)?;
        let resolved = match entry {
            RegistryEntry::Unique(resolved) => resolved,
            RegistryEntry::Ambiguous | RegistryEntry::Unverifiable => {
                return Err(ProviderFailure::Rejected);
            }
        };
        if resolved.device_generation != target.device_generation
            || resolved.locator != target.locator
        {
            return Err(ProviderFailure::IdentityChanged);
        }
        Ok(resolved.clone())
    }
}

impl LiveStorageTargetVerifier {
    pub(crate) fn standard() -> Self {
        Self::with_root("/sys/class/block")
    }

    pub(crate) fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            sysfs_block_root: Arc::new(root.into()),
        }
    }

    /// Re-read the live whole-device identity immediately before native I/O.
    ///
    /// Generation remains collector-owned, while sysfs proves that its current
    /// locator still names the same persistent physical identity.
    pub(crate) fn verify(&self, target: &ResolvedStorageTarget) -> Result<(), ProviderFailure> {
        if target.identity_stability != StorageIdentityStability::Persistent {
            return Err(ProviderFailure::Rejected);
        }
        let name = kernel_name(target.locator.as_str()).ok_or(ProviderFailure::Rejected)?;
        let device = self.sysfs_block_root.join(name);
        fs::metadata(&device).map_err(|error| live_device_error(&error))?;
        match fs::metadata(device.join("partition")) {
            Ok(_) => return Err(ProviderFailure::IdentityChanged),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(live_metadata_error(&error)),
        }
        let mut failure = None;
        let wwid = first_live_text(
            &[
                device.join("wwid"),
                device.join("device/wwid"),
                device.join("device/cid"),
                device.join("md/uuid"),
                device.join("dm/uuid"),
            ],
            &mut failure,
        );
        let serial = read_live_text(&device.join("device/serial"), &mut failure);
        if wwid.is_none()
            && serial.is_none()
            && let Some(failure) = failure
        {
            return Err(failure);
        }
        let live_id = stable_disk_id(name, wwid.as_deref(), serial.as_deref());
        if live_id != target.device_id.as_str() {
            return Err(ProviderFailure::IdentityChanged);
        }
        Ok(())
    }
}

pub(crate) fn storage_identity_metadata_outcome(sources: &[SourceStatus]) -> SourceOutcome {
    sources
        .iter()
        .find(|source| source.provider.as_str() == STORAGE_IDENTITY_METADATA_PROVIDER)
        .map(|source| source.outcome)
        .unwrap_or(SourceOutcome::Unavailable(FailureKind::ProviderFault))
}

fn authoritative_entries(disks: &[DiskMetrics]) -> HashMap<DeviceId, RegistryEntry> {
    let candidates = disks
        .iter()
        .filter(|disk| !disk.device_id.is_empty() && !disk.name.is_empty())
        .map(|disk| ResolvedStorageTarget {
            device_id: DeviceId::new(disk.device_id.clone()),
            device_generation: disk.device_generation,
            locator: StorageDeviceKey::new(disk.name.clone()),
            connection: disk.connection(),
            identity_stability: disk.identity_stability,
        })
        .collect::<Vec<_>>();
    let mut id_counts = HashMap::<DeviceId, usize>::new();
    let mut locator_counts = HashMap::<StorageDeviceKey, usize>::new();
    for candidate in &candidates {
        let count = id_counts.entry(candidate.device_id.clone()).or_default();
        *count = count.saturating_add(1);
        let count = locator_counts.entry(candidate.locator.clone()).or_default();
        *count = count.saturating_add(1);
    }

    let mut entries = HashMap::new();
    for candidate in candidates {
        let ambiguous = id_counts
            .get(&candidate.device_id)
            .is_some_and(|count| *count != 1)
            || locator_counts
                .get(&candidate.locator)
                .is_some_and(|count| *count != 1);
        let entry = if ambiguous {
            RegistryEntry::Ambiguous
        } else if candidate.identity_stability != StorageIdentityStability::Persistent
            || candidate.connection.device_kind != StorageDeviceKind::Physical
        {
            RegistryEntry::Unverifiable
        } else {
            RegistryEntry::Unique(candidate.clone())
        };
        entries.insert(candidate.device_id, entry);
    }
    entries
}

fn authority_failure(
    discovery: SourceOutcome,
    identity_metadata: SourceOutcome,
) -> Option<FailureKind> {
    [discovery, identity_metadata]
        .into_iter()
        .filter_map(|outcome| match outcome {
            SourceOutcome::Available | SourceOutcome::Empty => None,
            SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => Some(failure),
        })
        .max_by_key(|failure| failure_priority(*failure))
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

const fn provider_failure(failure: FailureKind) -> ProviderFailure {
    match failure {
        FailureKind::Unsupported => ProviderFailure::Unsupported,
        FailureKind::RequiresEscalation => ProviderFailure::RequiresEscalation,
        FailureKind::PermissionDenied => ProviderFailure::PermissionDenied,
        FailureKind::MissingDependency => ProviderFailure::MissingDependency,
        FailureKind::TimedOut => ProviderFailure::TimedOut,
        FailureKind::IdentityChanged => ProviderFailure::IdentityChanged,
        FailureKind::TemporarilyUnavailable => ProviderFailure::TemporarilyUnavailable,
        FailureKind::Rejected => ProviderFailure::Rejected,
        FailureKind::ProviderFault => ProviderFailure::ProviderFault,
    }
}

fn kernel_name(locator: &str) -> Option<&str> {
    let name = locator.strip_prefix("/dev/").unwrap_or(locator);
    (!name.is_empty()
        && name.len() <= 255
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    .then_some(name)
}

fn first_live_text(paths: &[PathBuf], failure: &mut Option<ProviderFailure>) -> Option<String> {
    paths.iter().find_map(|path| read_live_text(path, failure))
}

fn read_live_text(path: &Path, failure: &mut Option<ProviderFailure>) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(value) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            let observed = live_metadata_error(&error);
            *failure = Some(stronger_provider_failure(*failure, observed));
            None
        }
    }
}

const fn stronger_provider_failure(
    current: Option<ProviderFailure>,
    observed: ProviderFailure,
) -> ProviderFailure {
    match current {
        Some(current)
            if provider_failure_priority(current) >= provider_failure_priority(observed) =>
        {
            current
        }
        Some(_) | None => observed,
    }
}

fn live_device_error(error: &std::io::Error) -> ProviderFailure {
    match error.kind() {
        ErrorKind::NotFound => ProviderFailure::IdentityChanged,
        _ => live_metadata_error(error),
    }
}

fn live_metadata_error(error: &std::io::Error) -> ProviderFailure {
    match error.kind() {
        ErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
        ErrorKind::TimedOut => ProviderFailure::TimedOut,
        _ => ProviderFailure::ProviderFault,
    }
}

const fn provider_failure_priority(failure: ProviderFailure) -> u8 {
    match failure {
        ProviderFailure::RequiresEscalation => 9,
        ProviderFailure::PermissionDenied => 8,
        ProviderFailure::MissingDependency => 7,
        ProviderFailure::TimedOut => 6,
        ProviderFailure::ProviderFault => 5,
        ProviderFailure::TemporarilyUnavailable => 4,
        ProviderFailure::IdentityChanged => 3,
        ProviderFailure::Rejected => 2,
        ProviderFailure::Unsupported => 1,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_storage_target_tests.rs"]
mod tests;

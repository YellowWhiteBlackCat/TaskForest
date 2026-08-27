//! Mounted-filesystem enrichment that never participates in device discovery.

use std::fs;
use std::io::ErrorKind;
use std::ops::Deref;
use std::path::PathBuf;

use sysinfo::Disks;
use taskmanager_platform_contract::{ProviderId, SourceStatus};

use super::provenance::SourceFailures;

const SYSINFO_MOUNTS_PROVIDER: ProviderId = ProviderId::borrowed("linux.storage.sysinfo.mounts");

#[derive(Debug, Clone)]
pub(super) struct MountedDiskFact {
    pub(super) device_name: String,
    pub(super) mount_point: String,
    pub(super) fs_type: String,
    pub(super) total_bytes: u64,
    pub(super) available_bytes: u64,
}

#[derive(Debug, Clone)]
pub(super) struct MountedDiskFactsObservation {
    facts: Vec<MountedDiskFact>,
    pub(super) source: SourceStatus,
}

impl Deref for MountedDiskFactsObservation {
    type Target = [MountedDiskFact];

    fn deref(&self) -> &Self::Target {
        &self.facts
    }
}

pub(super) fn mounted_disk_facts(disks: &Disks) -> MountedDiskFactsObservation {
    let mut failures = SourceFailures::default();
    let mut facts = Vec::with_capacity(disks.list().len());
    for disk in disks.list() {
        let device_path = PathBuf::from(disk.name());
        let canonical_name = match fs::canonicalize(&device_path) {
            Ok(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().to_string()),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => {
                failures.record_io(&error);
                None
            }
        };
        let device_name = canonical_name
            .or_else(|| {
                device_path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| disk.name().to_string_lossy().to_string());
        facts.push(MountedDiskFact {
            device_name,
            mount_point: disk.mount_point().to_string_lossy().to_string(),
            fs_type: disk.file_system().to_string_lossy().to_string(),
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
        });
    }
    let item_count = facts.len();
    MountedDiskFactsObservation {
        facts,
        source: SourceStatus {
            provider: SYSINFO_MOUNTS_PROVIDER,
            outcome: failures.outcome(item_count),
            item_count,
        },
    }
}

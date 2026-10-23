//! The partition ↔ filesystem association every frontend reads.
//!
//! The contract resolves the association between a partition and the
//! filesystem health fact of its mount. A renderer reads the read-only and
//! error state from the resolved fact instead of matching mount-point strings
//! itself.

use taskmanager_core::core::metrics::{DiskMetrics, DiskPartition};
use taskmanager_core::core::storage_health::{FilesystemHealth, FilesystemHealthSnapshot};

/// One partition with the filesystem health fact the contract resolved.
///
/// The fact is keyed by the partition's stable `device_id`, so a renderer
/// finds the row of a partition it already holds instead of re-deriving the
/// association.
#[derive(Clone, Copy, Debug)]
pub struct PartitionHealthFact<'a> {
    /// The owning disk's stable identity.
    pub disk_device_id: &'a str,
    /// The partition the fact belongs to.
    pub partition: &'a DiskPartition,
    /// The filesystem health fact of the partition's mount.
    ///
    /// `None` means the partition is unmounted or the filesystem provider
    /// observed no such mount — never a healthy filesystem.
    pub filesystem: Option<&'a FilesystemHealth>,
}

/// The filesystem health fact of one partition, resolved from the
/// authoritative filesystem snapshot.
///
/// This is the single-call path for a renderer that holds one partition: the
/// association is established in the contract layer, so no consumer compares
/// mount-point strings itself.
#[must_use]
pub fn partition_filesystem_health<'a>(
    partition: &DiskPartition,
    filesystems: Option<&'a FilesystemHealthSnapshot>,
) -> Option<&'a FilesystemHealth> {
    filesystems.and_then(|filesystems| partition.filesystem_health(filesystems))
}

/// Every partition of every disk, each paired with the filesystem health fact
/// the contract resolved for it.
///
/// The association is established in the contract layer; a renderer reads the
/// read-only flag, the error count and the health status from the returned
/// fact instead of matching mount-point strings itself.
#[must_use]
pub fn partition_health_facts<'a>(
    disks: &'a [DiskMetrics],
    filesystems: Option<&'a FilesystemHealthSnapshot>,
) -> Vec<PartitionHealthFact<'a>> {
    let mut facts = Vec::new();
    for disk in disks {
        for partition in &disk.partitions {
            facts.push(PartitionHealthFact {
                disk_device_id: disk.device_id.as_str(),
                partition,
                filesystem: filesystems
                    .and_then(|filesystems| partition.filesystem_health(filesystems)),
            });
        }
    }
    facts
}

#[cfg(test)]
#[path = "../../tests/headless/presentation_storage_health_tests.rs"]
mod tests;

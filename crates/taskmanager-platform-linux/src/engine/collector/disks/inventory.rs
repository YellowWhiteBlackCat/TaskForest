//! Assembly of authoritative sysfs devices with non-authoritative mount facts.

use std::collections::HashMap;
use std::path::Path;

use sysinfo::Disks;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus, stable_disk_id};
use taskmanager_core::core::metrics::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, DiskScalarObservations,
    ScalarObservation, StorageInterconnect,
};
use taskmanager_platform_contract::{DeviceId, DeviceSourceSnapshot, FailureKind, SourceOutcome};

use super::identity::reconcile_storage_identity;
use super::mounts::{MountedDiskFact, mounted_disk_facts};
use super::sysfs::{
    SYSFS_BLOCK_METADATA_PROVIDER, SysfsBlockInventory, is_auxiliary_block_device,
    read_sysfs_block_inventory,
};
use crate::engine::hardware::{describe_disk_type, physical_disk_key};

/// Assemble sysfs-authoritative devices with mount facts as enrichment.
///
/// `/proc/diskstats` and SMART are appended as further enrichments by the
/// stateful storage collector because they own counter baselines and protocol
/// caches. Neither is ever allowed to become discovery authority.
pub(super) fn collect_storage_inventory(
    disks: &Disks,
    sysfs_root: &Path,
    now_ms: u64,
) -> DeviceSourceSnapshot<Vec<DiskMetrics>> {
    let inventory = read_sysfs_block_inventory(sysfs_root, now_ms);
    let mounts = mounted_disk_facts(disks);
    let mut metrics = merge_disk_inventory(&inventory, &mounts, mounts.source.outcome, now_ms)
        .into_values()
        .collect::<Vec<_>>();
    metrics.sort_by(|left, right| left.name.cmp(&right.name));
    let discovered_devices = metrics
        .iter()
        .map(|metric| DeviceId::new(metric.device_id.clone()))
        .collect();
    DeviceSourceSnapshot::from_source_status(
        metrics,
        discovered_devices,
        inventory.discovery.clone(),
        vec![inventory.metadata.clone(), mounts.source.clone()],
    )
}

/// Reconcile both storage rows and lifecycle discovery IDs without requiring
/// the composition root to know provider-id strings or enrichment ordering.
pub(super) fn reconcile_storage_snapshot_identity(
    snapshot: &mut DeviceSourceSnapshot<Vec<DiskMetrics>>,
    identity_cache: &mut HashMap<String, DeviceId>,
) {
    let metadata_outcome = storage_metadata_outcome(snapshot);
    let discovered_devices =
        reconcile_storage_identity(&mut snapshot.value, metadata_outcome, identity_cache);
    snapshot.replace_discovered_devices(discovered_devices);
}

/// Return the typed sysfs metadata outcome from a storage domain snapshot.
///
/// A missing status is a composition fault, not an implicitly successful or
/// empty metadata refresh.
pub(super) fn storage_metadata_outcome(
    snapshot: &DeviceSourceSnapshot<Vec<DiskMetrics>>,
) -> SourceOutcome {
    snapshot
        .enrichments
        .iter()
        .find(|source| source.provider == SYSFS_BLOCK_METADATA_PROVIDER)
        .map(|source| source.outcome)
        .unwrap_or(SourceOutcome::Unavailable(FailureKind::ProviderFault))
}

pub(super) fn merge_disk_inventory(
    sysfs: &SysfsBlockInventory,
    mounted: &[MountedDiskFact],
    mount_outcome: SourceOutcome,
    now_ms: u64,
) -> HashMap<String, DiskMetrics> {
    let mut disk_map = HashMap::new();
    let mut mounted_by_device: HashMap<String, Vec<&MountedDiskFact>> = HashMap::new();
    for mount in mounted {
        mounted_by_device
            .entry(mount.device_name.clone())
            .or_default()
            .push(mount);
    }
    for device in &sysfs.whole_devices {
        let device_state = if device.capacity_bytes.current_value().is_some() {
            DeviceState::healthy(now_ms)
        } else {
            DeviceState {
                status: DeviceStatus::Stale,
                last_success_ms: None,
            }
        };
        let scalar_observations = DiskScalarObservations {
            capacity_bytes: device.capacity_bytes,
            available_bytes: unavailable_mount_observation(mount_outcome),
            read_bytes_per_sec: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            write_bytes_per_sec: ScalarObservation::unavailable(
                FailureKind::TemporarilyUnavailable,
            ),
            iops: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            active_time_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            response_time_ms: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        };
        let mut metric = DiskMetrics::new(format!("/dev/{}", device.name));
        metric.device_id = stable_disk_id(
            &device.name,
            device.stable_hardware_id.as_deref(),
            device.serial.as_deref(),
        );
        metric.device_state = device_state;
        metric.disk_type = describe_disk_type(device.connection, device.rotational);
        metric.identity_stability = device.identity_stability;
        metric.model = device.model.clone();
        metric.serial = device.serial.clone();
        metric.revision = device.revision.clone();
        metric.apply_connection(device.connection);
        metric.apply_attachment_capabilities(
            device.removable,
            matches!(
                device.connection.interconnect,
                StorageInterconnect::Usb
                    | StorageInterconnect::PcieTunnel
                    | StorageInterconnect::FireWire
            )
            .then_some(true),
        );
        metric.apply_scalar_observations(scalar_observations);
        disk_map.insert(device.name.clone(), metric);
    }

    let mut representative_sizes: HashMap<String, u64> = HashMap::new();
    for (device_name, mounts) in &mounted_by_device {
        let Some(mount) = representative_mount(mounts) else {
            continue;
        };
        let whole_name = sysfs
            .partition_parents
            .get(device_name)
            .cloned()
            .unwrap_or_else(|| physical_disk_key(device_name));
        if is_auxiliary_block_device(&whole_name) {
            continue;
        }

        let Some(entry) = disk_map.get_mut(&whole_name) else {
            // Mounts are filesystem enrichment only. They cannot manufacture a
            // physical device that the sysfs discovery source did not enumerate.
            continue;
        };
        let mut observations = *entry.scalar_observations();
        let accumulated_available = observations
            .available_bytes
            .current_value()
            .copied()
            .unwrap_or(0)
            .saturating_add(mount.available_bytes);
        observations.available_bytes =
            mount_observation(accumulated_available, now_ms, mount_outcome);
        entry.apply_scalar_observations(observations);

        let representative_size = representative_sizes.entry(whole_name).or_default();
        if mount.total_bytes > *representative_size {
            *representative_size = mount.total_bytes;
            entry.mount_point.clone_from(&mount.mount_point);
            entry.fs_type.clone_from(&mount.fs_type);
        }
    }
    for partition in &sysfs.partitions {
        let Some(parent) = disk_map.get_mut(&partition.parent_name) else {
            // A partition whose whole-device discovery did not succeed is not
            // promoted into a detached child row.
            continue;
        };
        let mounted = mounted_by_device
            .get(&partition.name)
            .and_then(|mounts| representative_mount(mounts));
        let (mount_point, fs_type, observations, device_state) = match mounted {
            Some(mount) => {
                let capacity = mount_observation(mount.total_bytes, now_ms, mount_outcome);
                let free = mount_observation(mount.available_bytes, now_ms, mount_outcome);
                let used = mount_observation(
                    mount.total_bytes.saturating_sub(mount.available_bytes),
                    now_ms,
                    mount_outcome,
                );
                (
                    mount.mount_point.clone(),
                    mount.fs_type.clone(),
                    DiskPartitionScalarObservations {
                        capacity_bytes: capacity,
                        used_bytes: used,
                        free_bytes: free,
                    },
                    DeviceState::healthy(now_ms),
                )
            }
            None => {
                let failure = match mount_outcome {
                    SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => {
                        failure
                    }
                    SourceOutcome::Available | SourceOutcome::Empty => FailureKind::Unsupported,
                };
                (
                    String::new(),
                    String::new(),
                    DiskPartitionScalarObservations {
                        capacity_bytes: partition.capacity_bytes,
                        used_bytes: ScalarObservation::unavailable(failure),
                        free_bytes: ScalarObservation::unavailable(failure),
                    },
                    if partition.capacity_bytes.current_value().is_some() {
                        DeviceState::healthy(now_ms)
                    } else {
                        DeviceState {
                            status: DeviceStatus::Stale,
                            last_success_ms: None,
                        }
                    },
                )
            }
        };
        let parent_device_id = parent.device_id.clone();
        let mut child = DiskPartition::new(partition.name.clone());
        child.device_id = DiskPartition::stable_id(&parent_device_id, &partition.name);
        child.parent_device_id = parent_device_id;
        child.device_state = device_state;
        child.mount_point = mount_point;
        child.fs_type = fs_type;
        child.apply_scalar_observations(observations);
        parent.partitions.push(child);
    }
    for metric in disk_map.values_mut() {
        metric
            .partitions
            .sort_by(|left, right| left.name.cmp(&right.name));
    }
    disk_map
}

fn representative_mount<'a>(mounts: &[&'a MountedDiskFact]) -> Option<&'a MountedDiskFact> {
    let largest_total = mounts.iter().map(|mount| mount.total_bytes).max()?;
    mounts
        .iter()
        .copied()
        .filter(|mount| mount.total_bytes == largest_total)
        .min_by(|left, right| left.mount_point.cmp(&right.mount_point))
}

fn unavailable_mount_observation(outcome: SourceOutcome) -> ScalarObservation<u64> {
    match outcome {
        SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => {
            ScalarObservation::unavailable(failure)
        }
        SourceOutcome::Available | SourceOutcome::Empty => {
            ScalarObservation::unavailable(FailureKind::Unsupported)
        }
    }
}

fn mount_observation(
    available_bytes: u64,
    now_ms: u64,
    outcome: SourceOutcome,
) -> ScalarObservation<u64> {
    match outcome {
        SourceOutcome::Partial(failure) => {
            ScalarObservation::partial(available_bytes, now_ms, failure)
        }
        SourceOutcome::Unavailable(failure) => ScalarObservation::unavailable(failure),
        SourceOutcome::Available | SourceOutcome::Empty => {
            ScalarObservation::available(available_bytes, now_ms)
        }
    }
}

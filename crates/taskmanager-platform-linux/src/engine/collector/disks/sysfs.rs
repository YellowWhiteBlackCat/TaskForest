//! Authoritative Linux block discovery and independently fallible metadata.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::ops::Deref;
use std::path::Path;

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::metrics::{
    ScalarObservation, StorageConnection, StorageIdentityStability,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

use super::provenance::{
    SourceFailures, first_optional_text, io_failure_kind, observe_required_sector_bytes,
    read_optional_bit, read_optional_canonical_basename, read_optional_text,
};
use crate::engine::hardware::{classify_storage_connection, physical_disk_key};

pub(super) const SYSFS_BLOCK_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.storage.sysfs.block");
pub(super) const SYSFS_BLOCK_METADATA_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.storage.sysfs.metadata");

#[derive(Debug, Clone, Default)]
pub(super) struct SysfsBlockInventory {
    pub(super) whole_devices: Vec<SysfsBlockDevice>,
    pub(super) partitions: Vec<SysfsBlockPartition>,
    pub(super) partition_parents: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub(super) struct SysfsBlockDevice {
    pub(super) name: String,
    pub(super) capacity_bytes: ScalarObservation<u64>,
    pub(super) stable_hardware_id: Option<String>,
    pub(super) serial: Option<String>,
    pub(super) revision: Option<String>,
    pub(super) model: String,
    pub(super) removable: Option<bool>,
    pub(super) rotational: Option<bool>,
    pub(super) connection: StorageConnection,
    pub(super) identity_stability: StorageIdentityStability,
}

#[derive(Debug, Clone)]
pub(super) struct SysfsBlockPartition {
    pub(super) name: String,
    pub(super) parent_name: String,
    pub(super) capacity_bytes: ScalarObservation<u64>,
}

/// Typed sysfs discovery and metadata observation.
///
/// Dereferencing exposes the successfully discovered inventory to the domain
/// assembler while preserving both source outcomes on the observation itself.
#[derive(Debug, Clone)]
pub(super) struct SysfsBlockInventoryObservation {
    inventory: SysfsBlockInventory,
    pub(super) discovery: SourceStatus,
    pub(super) metadata: SourceStatus,
}

impl Deref for SysfsBlockInventoryObservation {
    type Target = SysfsBlockInventory;

    fn deref(&self) -> &Self::Target {
        &self.inventory
    }
}

/// Enumerate Linux whole block devices independently of mount state.
///
/// Partitions are never emitted as top-level disks: their sysfs parent maps
/// mounted filesystem facts back to the whole device. Loop, zram, and legacy
/// ram devices are intentionally omitted because they are file/RAM-backed
/// helpers rather than durable storage hardware (zram has separate memory
/// telemetry). md and dm devices remain visible as whole logical storage and
/// use their sysfs UUID when available. Every other whole-device name is
/// accepted; hardware support is not based on a vendor/model allowlist.
///
/// Directory enumeration, entry decoding, and partition topology exclusively
/// determine `discovery`. Capacity, identity, labels, media hints, and
/// transport evidence are a separate metadata enrichment: failure there never
/// weakens a successfully enumerated inventory or confirms/removes a device.
pub(super) fn read_sysfs_block_inventory(
    root: &Path,
    now_ms: u64,
) -> SysfsBlockInventoryObservation {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            let failure = io_failure_kind(&error);
            return SysfsBlockInventoryObservation {
                inventory: SysfsBlockInventory::default(),
                discovery: SourceStatus {
                    provider: SYSFS_BLOCK_PROVIDER,
                    outcome: SourceOutcome::Unavailable(failure),
                    item_count: 0,
                },
                metadata: SourceStatus {
                    provider: SYSFS_BLOCK_METADATA_PROVIDER,
                    outcome: SourceOutcome::Unavailable(failure),
                    item_count: 0,
                },
            };
        }
    };
    let mut discovery_failures = SourceFailures::default();
    let mut metadata_failures = SourceFailures::default();
    let (canonical_root, canonical_root_failure) = match fs::canonicalize(root) {
        Ok(path) => (Some(path), None),
        Err(error) => (None, Some(io_failure_kind(&error))),
    };
    let mut inventory = SysfsBlockInventory::default();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                discovery_failures.record_io(&error);
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().trim().to_string();
        if name.is_empty() {
            discovery_failures.record(FailureKind::ProviderFault);
            continue;
        }
        let path = entry.path();
        let is_partition = match fs::metadata(path.join("partition")) {
            Ok(_) => true,
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Err(error) => {
                discovery_failures.record_io(&error);
                continue;
            }
        };
        if is_partition {
            if let Some(failure) = canonical_root_failure {
                discovery_failures.record(failure);
            }
            if let Some(parent) = sysfs_partition_parent(
                &path,
                &name,
                canonical_root.as_deref(),
                &mut discovery_failures,
            ) {
                let capacity_bytes = observe_required_sector_bytes(
                    &path.join("size"),
                    now_ms,
                    &mut metadata_failures,
                );
                inventory
                    .partition_parents
                    .insert(name.clone(), parent.clone());
                inventory.partitions.push(SysfsBlockPartition {
                    name,
                    parent_name: parent,
                    capacity_bytes,
                });
            } else {
                discovery_failures.record(FailureKind::ProviderFault);
            }
            continue;
        }
        if is_auxiliary_block_device(&name) {
            continue;
        }

        let capacity_bytes =
            observe_required_sector_bytes(&path.join("size"), now_ms, &mut metadata_failures);
        let stable_hardware_id = first_optional_text(
            &[
                path.join("wwid"),
                path.join("device/wwid"),
                path.join("device/cid"),
                path.join("md/uuid"),
                path.join("dm/uuid"),
            ],
            &mut metadata_failures,
        );
        let serial = read_optional_text(&path.join("device/serial"), &mut metadata_failures);
        let revision = first_optional_text(
            &[
                path.join("device/firmware_rev"),
                path.join("device/rev"),
                path.join("device/revision"),
            ],
            &mut metadata_failures,
        );
        let model = first_optional_text(
            &[path.join("device/model"), path.join("dm/name")],
            &mut metadata_failures,
        )
        .unwrap_or_default();
        let removable = read_optional_bit(&path.join("removable"), &mut metadata_failures);
        let rotational = read_optional_bit(&path.join("queue/rotational"), &mut metadata_failures);
        let transport_evidence =
            read_optional_text(&path.join("device/transport"), &mut metadata_failures);
        let protocol_evidence = first_optional_text(
            &[path.join("device/protocol"), path.join("device/type")],
            &mut metadata_failures,
        );
        let subsystem_evidence = read_optional_canonical_basename(
            &path.join("device/subsystem"),
            &mut metadata_failures,
        );
        let topology_evidence = match fs::canonicalize(&path) {
            Ok(topology) => Some(topology.to_string_lossy().into_owned()),
            Err(error) => {
                metadata_failures.record_io(&error);
                None
            }
        };
        let connection = classify_storage_connection(
            &name,
            transport_evidence.as_deref(),
            protocol_evidence.as_deref(),
            subsystem_evidence.as_deref(),
            topology_evidence.as_deref(),
        );
        let identity_stability = if stable_hardware_id.is_some() || serial.is_some() {
            StorageIdentityStability::Persistent
        } else {
            StorageIdentityStability::Attachment
        };
        inventory.whole_devices.push(SysfsBlockDevice {
            name,
            capacity_bytes,
            stable_hardware_id,
            serial,
            revision,
            model,
            removable,
            rotational,
            connection,
            identity_stability,
        });
    }

    inventory
        .whole_devices
        .sort_by(|left, right| left.name.cmp(&right.name));
    inventory
        .partitions
        .sort_by(|left, right| left.name.cmp(&right.name));
    let item_count = inventory.whole_devices.len();
    let metadata_item_count = item_count.saturating_add(inventory.partitions.len());
    SysfsBlockInventoryObservation {
        inventory,
        discovery: SourceStatus {
            provider: SYSFS_BLOCK_PROVIDER,
            outcome: discovery_failures.outcome(item_count),
            item_count,
        },
        metadata: SourceStatus {
            provider: SYSFS_BLOCK_METADATA_PROVIDER,
            outcome: metadata_failures.outcome(metadata_item_count),
            item_count: metadata_item_count,
        },
    }
}

pub(super) fn sysfs_partition_parent(
    path: &Path,
    name: &str,
    canonical_root: Option<&Path>,
    failures: &mut SourceFailures,
) -> Option<String> {
    let canonical = match fs::canonicalize(path) {
        Ok(path) => Some(path),
        Err(error) => {
            failures.record_io(&error);
            None
        }
    };
    let topology_parent = canonical
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .filter(|parent| Some(parent.as_path()) != canonical_root)
        .and_then(|parent| {
            parent
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
        })
        .filter(|parent| parent != name && !parent.is_empty());
    topology_parent.or_else(|| {
        let heuristic = physical_disk_key(name);
        (heuristic != name).then_some(heuristic)
    })
}

pub(super) fn is_auxiliary_block_device(name: &str) -> bool {
    ["loop", "zram", "ram"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
    })
}

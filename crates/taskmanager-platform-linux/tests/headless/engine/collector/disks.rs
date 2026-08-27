use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use sysinfo::Disks;
use taskmanager_core::core::device_state::{DeviceRefreshOutcome, DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::{
    StorageIdentityStability, StorageInterconnect, StorageProtocol,
};
use taskmanager_platform_contract::{FailureKind, SourceOutcome};

use super::inventory::{
    collect_storage_inventory, merge_disk_inventory, reconcile_storage_snapshot_identity,
};
use super::mounts::MountedDiskFact;
use super::provenance::SourceFailures;
use super::sysfs::{
    SYSFS_BLOCK_METADATA_PROVIDER, SYSFS_BLOCK_PROVIDER, SysfsBlockInventory,
    read_sysfs_block_inventory, sysfs_partition_parent,
};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct BlockFixture {
    root: PathBuf,
}

impl BlockFixture {
    fn new(label: &str) -> Self {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-block-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create isolated block fixture");
        Self { root }
    }

    fn device(&self, name: &str, sectors: u64) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(&path).expect("create fixture device");
        fs::write(path.join("size"), sectors.to_string()).expect("write fixture size");
        path
    }

    fn device_without_size(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(&path).expect("create fixture device");
        path
    }

    fn partition(&self, name: &str) {
        let path = self.device(name, 100);
        fs::write(path.join("partition"), "1").expect("write partition marker");
    }
}

impl Drop for BlockFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn sysfs_inventory_surfaces_unmounted_whole_disks_with_stable_identity() {
    let fixture = BlockFixture::new("unmounted");
    let nvme = fixture.device("nvme9n1", 2_000);
    fs::write(nvme.join("wwid"), "eui.00112233\n").expect("write WWID");
    fs::write(nvme.join("removable"), "0\n").expect("write removable bit");
    fs::create_dir_all(nvme.join("device")).expect("create device metadata");
    fs::write(nvme.join("device/model"), "World Disk\n").expect("write model");
    fs::write(nvme.join("device/serial"), "SERIAL-001\n").expect("write serial");
    fs::write(nvme.join("device/firmware_rev"), "FW-42\n").expect("write revision");
    fixture.partition("nvme9n1p1");
    fixture.device("loop0", 100);
    fixture.device("zram0", 100);
    let md = fixture.device("md0", 4_000);
    fs::create_dir_all(md.join("md")).expect("create md metadata");
    fs::write(md.join("md/uuid"), "md-array-uuid\n").expect("write md UUID");
    let dm = fixture.device("dm-0", 3_000);
    fs::create_dir_all(dm.join("dm")).expect("create dm metadata");
    fs::write(dm.join("dm/uuid"), "crypt-volume-uuid\n").expect("write dm UUID");

    let inventory = read_sysfs_block_inventory(&fixture.root, 55);
    assert_eq!(inventory.discovery.outcome, SourceOutcome::Available);
    assert_eq!(inventory.metadata.outcome, SourceOutcome::Available);
    let names: Vec<&str> = inventory
        .whole_devices
        .iter()
        .map(|device| device.name.as_str())
        .collect();
    assert_eq!(names, vec!["dm-0", "md0", "nvme9n1"]);
    assert_eq!(
        inventory.partition_parents.get("nvme9n1p1"),
        Some(&"nvme9n1".to_string())
    );
    assert_eq!(inventory.partitions.len(), 1);
    assert_eq!(inventory.partitions[0].name, "nvme9n1p1");
    assert_eq!(
        inventory.partitions[0]
            .capacity_bytes
            .current_value()
            .copied(),
        Some(100 * 512)
    );

    let merged = merge_disk_inventory(&inventory, &[], SourceOutcome::Empty, 55);
    let unmounted = merged
        .get("nvme9n1")
        .expect("unmounted whole disk must remain visible");
    assert_eq!(unmounted.device_id, "disk:wwid:eui.00112233");
    assert_eq!(unmounted.current_capacity_bytes(), Some(2_000 * 512));
    assert_eq!(unmounted.current_capacity_bytes(), Some(2_000 * 512));
    assert_eq!(unmounted.model, "World Disk");
    assert_eq!(unmounted.serial.as_deref(), Some("SERIAL-001"));
    assert_eq!(unmounted.revision.as_deref(), Some("FW-42"));
    assert_eq!(
        unmounted.identity_stability,
        StorageIdentityStability::Persistent
    );
    assert_eq!(unmounted.connection().protocol, StorageProtocol::Nvme);
    assert_eq!(unmounted.media_removable(), Some(false));
    assert_eq!(unmounted.hotplug_capable(), None);
    assert!(unmounted.mount_point.is_empty());
    assert!(unmounted.fs_type.is_empty());
    assert_eq!(
        unmounted.current_available_bytes(),
        None,
        "no mounted filesystem means no invented filesystem-free value"
    );
    assert_eq!(unmounted.device_state, DeviceState::healthy(55));
    assert_eq!(
        merged.get("md0").map(|disk| disk.device_id.as_str()),
        Some("disk:wwid:md-array-uuid")
    );
    assert_eq!(
        merged.get("dm-0").map(|disk| disk.device_id.as_str()),
        Some("disk:wwid:crypt-volume-uuid")
    );
}

#[test]
fn mounted_partitions_merge_without_subvolume_duplication() {
    let fixture = BlockFixture::new("mounted");
    fixture.device("nvme8n1", 10_000);
    fixture.partition("nvme8n1p1");
    fixture.partition("nvme8n1p2");
    let inventory = read_sysfs_block_inventory(&fixture.root, 77);
    assert_eq!(inventory.discovery.outcome, SourceOutcome::Available);
    let mounts = vec![
        MountedDiskFact {
            device_name: "nvme8n1p1".into(),
            mount_point: "/".into(),
            fs_type: "btrfs".into(),
            total_bytes: 3_000 * 512,
            available_bytes: 800 * 512,
        },
        MountedDiskFact {
            device_name: "nvme8n1p1".into(),
            mount_point: "/home".into(),
            fs_type: "btrfs".into(),
            total_bytes: 3_000 * 512,
            available_bytes: 800 * 512,
        },
        MountedDiskFact {
            device_name: "nvme8n1p2".into(),
            mount_point: "/boot".into(),
            fs_type: "vfat".into(),
            total_bytes: 500 * 512,
            available_bytes: 200 * 512,
        },
    ];

    let merged = merge_disk_inventory(&inventory, &mounts, SourceOutcome::Available, 77);
    assert_eq!(merged.len(), 1, "partitions must not become disk rows");
    let disk = merged.get("nvme8n1").expect("whole disk must be present");
    assert_eq!(disk.current_capacity_bytes(), Some(10_000 * 512));
    assert_eq!(
        disk.current_available_bytes().expect("mounted free space"),
        (800 + 200) * 512,
        "the duplicate btrfs subvolume must contribute only once"
    );
    assert_eq!(disk.current_available_bytes(), Some((800 + 200) * 512));
    assert_eq!(disk.mount_point, "/");
    assert_eq!(disk.fs_type, "btrfs");
    assert_eq!(disk.partitions.len(), 2);
    let first = &disk.partitions[0];
    assert_eq!(first.name, "nvme8n1p1");
    assert_eq!(first.mount_point, "/");
    assert_eq!(first.current_capacity_bytes(), Some(3_000 * 512));
    assert_eq!(first.current_used_bytes(), Some(2_200 * 512));
    assert_eq!(first.current_free_bytes(), Some(800 * 512));
    assert_eq!(first.parent_device_id, disk.device_id);
    assert_eq!(first.device_id, "partition:disk:path:nvme8n1:nvme8n1p1");
    let second = &disk.partitions[1];
    assert_eq!(second.name, "nvme8n1p2");
    assert_eq!(second.mount_point, "/boot");
    assert_eq!(second.current_used_bytes(), Some(300 * 512));
    assert_eq!(second.current_free_bytes(), Some(200 * 512));
}

#[test]
fn unmounted_partition_publishes_capacity_without_fabricating_filesystem_space() {
    let fixture = BlockFixture::new("unmounted-partition");
    fixture.device("nvme7n1", 10_000);
    fixture.partition("nvme7n1p1");
    let inventory = read_sysfs_block_inventory(&fixture.root, 78);
    let merged = merge_disk_inventory(&inventory, &[], SourceOutcome::Empty, 78);
    let disk = merged.get("nvme7n1").expect("whole disk is discovered");
    let partition = disk
        .partitions
        .first()
        .expect("sysfs partition remains visible without a mount");

    assert_eq!(partition.current_capacity_bytes(), Some(100 * 512));
    assert_eq!(partition.current_used_bytes(), None);
    assert_eq!(partition.current_free_bytes(), None);
    assert!(partition.mount_point.is_empty());
    assert!(partition.fs_type.is_empty());
}

#[test]
fn mount_partial_failure_keeps_available_bytes_current_but_typed_partial() {
    let fixture = BlockFixture::new("mounted-partial");
    fixture.device("future0", 1_000);
    let inventory = read_sysfs_block_inventory(&fixture.root, 80);
    let mounts = vec![MountedDiskFact {
        device_name: "future0".into(),
        mount_point: "/data".into(),
        fs_type: "futurefs".into(),
        total_bytes: 500,
        available_bytes: 0,
    }];

    let merged = merge_disk_inventory(
        &inventory,
        &mounts,
        SourceOutcome::Partial(FailureKind::PermissionDenied),
        80,
    );
    let disk = &merged["future0"];

    assert_eq!(disk.current_available_bytes(), Some(0));
    assert_eq!(
        disk.scalar_observations().available_bytes.availability(),
        taskmanager_core::ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(
        disk.scalar_observations().available_bytes.last_success_ms(),
        Some(80)
    );
}

#[test]
fn partition_mount_failure_recovers_without_changing_stable_child_identity() {
    let fixture = BlockFixture::new("mount-recovery");
    fixture.device("nvme6n1", 1_000);
    fixture.partition("nvme6n1p1");
    let inventory = read_sysfs_block_inventory(&fixture.root, 90);

    let failed = merge_disk_inventory(
        &inventory,
        &[],
        SourceOutcome::Partial(FailureKind::PermissionDenied),
        90,
    );
    let failed_partition = failed["nvme6n1"]
        .partitions
        .first()
        .expect("sysfs child remains visible when mount enrichment fails")
        .clone();
    assert!(failed_partition.mount_point.is_empty());
    assert_eq!(failed_partition.current_capacity_bytes(), Some(100 * 512));
    assert_eq!(failed_partition.current_used_bytes(), None);
    assert_eq!(failed_partition.current_free_bytes(), None);
    assert_eq!(
        failed_partition
            .scalar_observations()
            .used_bytes
            .availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(
        failed_partition
            .scalar_observations()
            .free_bytes
            .availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );

    let recovered = merge_disk_inventory(
        &inventory,
        &[MountedDiskFact {
            device_name: "nvme6n1p1".into(),
            mount_point: "/data".into(),
            fs_type: "ext4".into(),
            total_bytes: 500,
            available_bytes: 200,
        }],
        SourceOutcome::Available,
        91,
    );
    let recovered_partition = recovered["nvme6n1"]
        .partitions
        .first()
        .expect("the same sysfs child must remain after mount recovery");
    assert_eq!(recovered_partition.device_id, failed_partition.device_id);
    assert_eq!(
        recovered_partition.parent_device_id,
        failed_partition.parent_device_id
    );
    assert_eq!(recovered_partition.mount_point, "/data");
    assert_eq!(recovered_partition.fs_type, "ext4");
    assert_eq!(recovered_partition.current_capacity_bytes(), Some(500));
    assert_eq!(recovered_partition.current_used_bytes(), Some(300));
    assert_eq!(recovered_partition.current_free_bytes(), Some(200));
    assert_eq!(
        recovered_partition.device_state.status,
        DeviceStatus::Healthy
    );
}

#[test]
fn sysfs_transport_evidence_drives_typed_disk_and_media_labels() {
    let fixture = BlockFixture::new("transport");
    let sas = fixture.device("sda", 4_000);
    fs::create_dir_all(sas.join("device")).expect("create SAS metadata");
    fs::create_dir_all(sas.join("queue")).expect("create SAS queue");
    fs::write(sas.join("device/transport"), "sas\n").expect("write SAS transport");
    fs::write(sas.join("queue/rotational"), "1\n").expect("write rotational bit");

    let usb = fixture.device("sdb", 2_000);
    fs::create_dir_all(usb.join("device")).expect("create USB metadata");
    fs::create_dir_all(usb.join("queue")).expect("create USB queue");
    fs::write(usb.join("device/transport"), "usb\n").expect("write USB transport");
    fs::write(usb.join("device/protocol"), "ata\n").expect("write tunneled ATA protocol");
    fs::write(usb.join("queue/rotational"), "0\n").expect("write non-rotational bit");

    let unknown = fixture.device("sdc", 1_000);
    fs::create_dir_all(unknown.join("device")).expect("create unknown metadata");

    let inventory = read_sysfs_block_inventory(&fixture.root, 90);
    assert_eq!(inventory.discovery.outcome, SourceOutcome::Available);
    let merged = merge_disk_inventory(&inventory, &[], SourceOutcome::Empty, 90);

    assert_eq!(
        merged["sda"].connection().interconnect,
        StorageInterconnect::Sas
    );
    assert_eq!(merged["sda"].disk_type, "SAS HDD");
    assert_eq!(merged["sdb"].connection().protocol, StorageProtocol::Ata);
    assert_eq!(
        merged["sdb"].connection().interconnect,
        StorageInterconnect::Usb
    );
    assert_eq!(merged["sdb"].hotplug_capable(), Some(true));
    assert_eq!(merged["sdb"].disk_type, "USB SSD");
    assert_eq!(
        merged["sdc"].connection().protocol,
        StorageProtocol::Unknown
    );
    assert_eq!(
        merged["sdc"].connection().interconnect,
        StorageInterconnect::Unknown
    );
    assert_eq!(merged["sdc"].disk_type, "Unknown Block Device");
}

#[test]
fn partition_parent_prefers_sysfs_topology_for_unknown_naming_schemes() {
    let fixture = BlockFixture::new("topology");
    let whole = fixture.root.join("controller/future-whole-device");
    let partition = whole.join("vendor-partition-name");
    fs::create_dir_all(&partition).expect("create nested sysfs topology");
    fs::write(partition.join("partition"), "1").expect("write partition marker");
    let canonical_root = fs::canonicalize(&fixture.root).expect("canonical fixture root");
    let mut failures = SourceFailures::default();

    assert_eq!(
        sysfs_partition_parent(
            &partition,
            "vendor-partition-name",
            Some(canonical_root.as_path()),
            &mut failures,
        )
        .as_deref(),
        Some("future-whole-device"),
        "partition grouping must not depend on an nvme/sd/mmc vendor-name allowlist"
    );
    assert_eq!(failures.strongest, None);
}

#[test]
fn missing_size_is_stale_but_keeps_the_discovered_device() {
    let fixture = BlockFixture::new("missing-size");
    let device = fixture.device_without_size("future-block0");
    fs::create_dir_all(device.join("device")).expect("create metadata");
    fs::write(device.join("device/model"), "Future Storage\n").expect("write model");

    let inventory = read_sysfs_block_inventory(&fixture.root, 66);
    assert_eq!(
        inventory.discovery.outcome,
        SourceOutcome::Available,
        "metadata failure must not weaken successful enumeration"
    );
    assert_eq!(
        inventory.metadata.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    let merged = merge_disk_inventory(&inventory, &[], SourceOutcome::Empty, 66);
    let disk = merged
        .get("future-block0")
        .expect("unknown capacity must not erase the device");
    assert_eq!(disk.model, "Future Storage");
    assert_eq!(
        disk.current_capacity_bytes(),
        None,
        "failed capacity stays unavailable"
    );
    assert_eq!(disk.current_capacity_bytes(), None);
    assert_eq!(
        disk.scalar_observations().capacity_bytes.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(disk.device_state.status, DeviceStatus::Stale);
    assert_eq!(disk.device_state.last_success_ms, None);
}

#[test]
fn missing_sysfs_and_mount_facts_cannot_manufacture_device_rows() {
    let missing = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-missing-block-root-{}",
        std::process::id()
    ));
    let unavailable = read_sysfs_block_inventory(&missing, 88);
    assert_eq!(
        unavailable.discovery.outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported),
        "an absent sysfs root must be a typed source miss"
    );

    let mounts = vec![MountedDiskFact {
        device_name: "sdb1".into(),
        mount_point: "/data".into(),
        fs_type: "ext4".into(),
        total_bytes: 900,
        available_bytes: 400,
    }];
    let merged = merge_disk_inventory(
        &SysfsBlockInventory::default(),
        &mounts,
        SourceOutcome::Available,
        88,
    );
    assert!(
        merged.is_empty(),
        "mount enrichment must not become a device-discovery fallback"
    );
}

#[test]
fn device_source_snapshot_keeps_sysfs_as_the_only_discovery_authority() {
    let fixture = BlockFixture::new("device-source");
    fixture.device("future0", 100);
    let snapshot = collect_storage_inventory(&Disks::new(), &fixture.root, 91);

    assert!(snapshot.discovery_is_authoritative());
    assert_eq!(snapshot.discovery().provider, SYSFS_BLOCK_PROVIDER);
    assert_eq!(snapshot.discovery().item_count, 1);
    assert_eq!(snapshot.value.len(), 1);
    assert_eq!(snapshot.discovered_devices().len(), 1);
    assert_eq!(
        snapshot.discovered_devices()[0].as_str(),
        "disk:path:future0"
    );
    assert_eq!(snapshot.enrichments.len(), 2);
    assert_eq!(
        snapshot.enrichments[0].provider,
        SYSFS_BLOCK_METADATA_PROVIDER
    );
    assert_eq!(
        snapshot.enrichments[1].provider.as_str(),
        "linux.storage.sysinfo.mounts"
    );
    assert_eq!(snapshot.enrichments[0].outcome, SourceOutcome::Available);
    assert_eq!(snapshot.enrichments[1].outcome, SourceOutcome::Empty);
    assert_eq!(
        DeviceRefreshOutcome::from_discovery_outcome(snapshot.discovery().outcome),
        DeviceRefreshOutcome::Complete
    );
}

#[test]
fn partial_sysfs_discovery_retains_rows_but_cannot_confirm_absence() {
    let fixture = BlockFixture::new("partial-discovery");
    fixture.device("healthy0", 100);
    let orphan_partition = fixture.device("future-partition", 100);
    fs::write(orphan_partition.join("partition"), "1")
        .expect("write unresolvable partition marker");

    let snapshot = collect_storage_inventory(&Disks::new(), &fixture.root, 92);

    assert_eq!(snapshot.value.len(), 1);
    assert_eq!(
        snapshot.discovery().outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert!(!snapshot.discovery_is_authoritative());
    assert_eq!(
        DeviceRefreshOutcome::from_discovery_outcome(snapshot.discovery().outcome),
        DeviceRefreshOutcome::Unavailable(DeviceStatus::Stale)
    );
}

#[test]
fn metadata_failure_does_not_block_authoritative_absence_confirmation() {
    let fixture = BlockFixture::new("metadata-partial");
    fixture.device_without_size("future0");

    let snapshot = collect_storage_inventory(&Disks::new(), &fixture.root, 93);

    assert_eq!(snapshot.discovery().outcome, SourceOutcome::Available);
    assert_eq!(
        snapshot.enrichments[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert!(snapshot.discovery_is_authoritative());
    assert_eq!(
        DeviceRefreshOutcome::from_discovery_outcome(snapshot.discovery().outcome),
        DeviceRefreshOutcome::Complete
    );
}

#[test]
fn metadata_outage_reuses_same_kernel_paths_last_stable_identity() {
    let fixture = BlockFixture::new("identity-outage");
    let device = fixture.device("future0", 100);
    fs::write(device.join("wwid"), "eui.stable\n").expect("write stable identity");
    let mut identity_cache = HashMap::new();

    let mut initial = collect_storage_inventory(&Disks::new(), &fixture.root, 94);
    reconcile_storage_snapshot_identity(&mut initial, &mut identity_cache);
    assert_eq!(initial.value[0].device_id, "disk:wwid:eui.stable");
    assert_eq!(
        initial.value[0].identity_stability,
        StorageIdentityStability::Persistent
    );

    fs::remove_file(device.join("wwid")).expect("remove identity metadata");
    fs::write(device.join("removable"), "malformed").expect("make metadata refresh partial");
    let mut degraded = collect_storage_inventory(&Disks::new(), &fixture.root, 95);
    assert_eq!(
        degraded.enrichments[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(
        degraded.value[0].device_id, "disk:path:future0",
        "raw degraded observation demonstrates the fallback that must be reconciled"
    );

    reconcile_storage_snapshot_identity(&mut degraded, &mut identity_cache);

    assert_eq!(degraded.value[0].device_id, "disk:wwid:eui.stable");
    assert_eq!(
        degraded.value[0].identity_stability,
        StorageIdentityStability::Persistent,
        "cached persistent identity must not be mislabeled attachment-scoped"
    );
    assert_eq!(
        degraded.discovered_devices()[0].as_str(),
        "disk:wwid:eui.stable"
    );
}

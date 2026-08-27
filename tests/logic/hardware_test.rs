//! Unit tests for the Linux provider's pure disk-name helpers.
//!
//! `physical_disk_key` collapses a partition device name to its physical disk.
//! Transport classification consumes protocol/subsystem/topology evidence
//! instead of guessing SATA from the ambiguous Linux `sd*` namespace.

use taskmanager::core::metrics::{
    StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol,
};
use taskmanager_platform_linux::{
    classify_disk_type, classify_storage_connection, describe_disk_type, parse_cpulist,
    parse_size_to_kb, physical_disk_key,
};

/// Each supported partition grammar collapses to a whole-device key.
#[test]
fn test_physical_disk_key_collapses_partitions() {
    let cases = [
        ("nvme0n1p6", "nvme0n1"),
        ("nvme10n2p1", "nvme10n2"),
        ("mmcblk0p1", "mmcblk0"),
        ("mmcblk12p34", "mmcblk12"),
        ("nvdimm0p1", "nvdimm0"),
        ("sda3", "sda"),
        ("sdp1", "sdp"),
        ("sdaa12", "sdaa"),
        ("hda1", "hda"),
        ("vda2", "vda"),
        ("xvda3", "xvda"),
        ("ubda4", "ubda"),
    ];

    for (partition, expected) in cases {
        assert_eq!(physical_disk_key(partition), expected, "{partition}");
    }
}

#[test]
fn test_physical_disk_key_preserves_whole_disks_ending_in_digits() {
    for whole_disk in [
        "nvme0n1", "mmcblk0", "nvdimm0", "md0", "md127", "loop0", "zram0", "sr0", "ram0", "fd0",
    ] {
        assert_eq!(physical_disk_key(whole_disk), whole_disk);
    }
}

#[test]
fn test_physical_disk_key_strips_dev_prefix() {
    assert_eq!(physical_disk_key("/dev/nvme0n1p6"), "nvme0n1");
    assert_eq!(physical_disk_key("/dev/sda3"), "sda");
    assert_eq!(physical_disk_key(" /dev/mmcblk0p2\n"), "mmcblk0");
}

#[test]
fn test_physical_disk_key_rejects_malformed_partition_shapes() {
    for malformed in [
        "nvme0n1p",
        "nvme0n1px",
        "nvme0p1",
        "nvmen1p1",
        "fakenvme0n1p2",
        "mmcblkp1",
        "mmcblkxp1",
        "sda",
        "sda1x",
        "md0p1",
        "",
    ] {
        assert_eq!(physical_disk_key(malformed), malformed);
    }
}

#[test]
fn test_classify_disk_type_all_branches() {
    assert_eq!(classify_disk_type("nvme0n1"), "NVMe SSD");
    assert_eq!(classify_disk_type("nvme10n2"), "NVMe SSD");
    assert_eq!(classify_disk_type("sda"), "Unknown Block Device");
    assert_eq!(classify_disk_type("sdp"), "Unknown Block Device");
    assert_eq!(classify_disk_type("hda"), "IDE Storage");
    assert_eq!(classify_disk_type("hdz"), "IDE Storage");
    assert_eq!(classify_disk_type("mmcblk0"), "eMMC/SD");
    assert_eq!(classify_disk_type("vda"), "Virtio Disk");
    assert_eq!(classify_disk_type("dm-0"), "Device Mapper");
    assert_eq!(classify_disk_type("md0"), "Software RAID");
    assert_eq!(classify_disk_type("loop0"), "Unknown Block Device");
    assert_eq!(classify_disk_type("zram0"), "Unknown Block Device");
    assert_eq!(classify_disk_type("sr0"), "Unknown Block Device");
}

#[test]
fn test_classify_disk_type_requires_valid_whole_disk_shapes() {
    for malformed in ["nvme", "nvme0", "nvme0n", "nvme0n1p2", "mmcblk", "sd", "hd"] {
        assert_eq!(
            classify_disk_type(malformed),
            "Unknown Block Device",
            "{malformed}"
        );
    }
}

#[test]
fn test_connection_matrix_uses_protocol_and_topology_not_vendor_or_sd_name() {
    let cases = [
        (
            "sda",
            Some("ata"),
            None,
            Some("scsi"),
            None,
            StorageConnection::new(
                StorageProtocol::Ata,
                StorageInterconnect::Sata,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "sdb",
            Some("sas"),
            None,
            Some("scsi"),
            None,
            StorageConnection::new(
                StorageProtocol::Scsi,
                StorageInterconnect::Sas,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "sdc",
            Some("scsi"),
            None,
            Some("scsi"),
            None,
            StorageConnection::new(
                StorageProtocol::Scsi,
                StorageInterconnect::Other,
                StorageDeviceKind::Unknown,
            ),
        ),
        (
            "sdd",
            Some("ata"),
            None,
            Some("scsi"),
            Some("/sys/devices/pci/usb/usb2/2-1/host0/target0/block/sdd"),
            StorageConnection::new(
                StorageProtocol::Ata,
                StorageInterconnect::Usb,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "future0",
            Some("usb"),
            None,
            None,
            None,
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Usb,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "nvme9n1",
            Some("pcie"),
            None,
            Some("nvme"),
            None,
            StorageConnection::new(
                StorageProtocol::Nvme,
                StorageInterconnect::Pcie,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "mmcblk4",
            None,
            None,
            Some("mmc"),
            None,
            StorageConnection::new(
                StorageProtocol::Mmc,
                StorageInterconnect::Mmc,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "sda",
            Some("ufs"),
            Some("scsi"),
            Some("scsi"),
            None,
            StorageConnection::new(
                StorageProtocol::Ufs,
                StorageInterconnect::Ufs,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "vda",
            None,
            None,
            Some("virtio"),
            None,
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Virtio,
                StorageDeviceKind::Virtual,
            ),
        ),
        (
            "hda",
            None,
            None,
            Some("ide"),
            None,
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Ide,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "dm-12",
            None,
            None,
            None,
            None,
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Platform,
                StorageDeviceKind::Virtual,
            ),
        ),
        (
            "md127",
            None,
            None,
            None,
            None,
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Platform,
                StorageDeviceKind::Aggregate,
            ),
        ),
    ];

    for (name, transport, protocol, subsystem, topology, expected) in cases {
        assert_eq!(
            classify_storage_connection(name, transport, protocol, subsystem, topology),
            expected,
            "{name}"
        );
    }
    assert_eq!(
        classify_storage_connection("sdz", None, None, Some("scsi"), None),
        StorageConnection::new(
            StorageProtocol::Scsi,
            StorageInterconnect::Other,
            StorageDeviceKind::Unknown,
        ),
        "the sd namespace must not invent a physical device kind or transport"
    );
    assert_eq!(
        describe_disk_type(
            StorageConnection::new(
                StorageProtocol::Ata,
                StorageInterconnect::Sata,
                StorageDeviceKind::Physical,
            ),
            Some(false),
        ),
        "SATA SSD"
    );
    assert_eq!(
        describe_disk_type(
            StorageConnection::new(
                StorageProtocol::Scsi,
                StorageInterconnect::Sas,
                StorageDeviceKind::Physical,
            ),
            Some(true),
        ),
        "SAS HDD"
    );
}

#[test]
fn test_storage_connection_separates_protocol_bus_and_logical_kind() {
    let sat_bridge = classify_storage_connection(
        "sda",
        Some("ata"),
        None,
        Some("scsi"),
        Some("/devices/pci/usb/usb2/2-1/host0/target0/block/sda"),
    );
    assert_eq!(sat_bridge.protocol, StorageProtocol::Ata);
    assert_eq!(sat_bridge.interconnect, StorageInterconnect::Usb);
    assert_eq!(sat_bridge.device_kind, StorageDeviceKind::Physical);

    let sas = classify_storage_connection("sdb", Some("sas"), Some("scsi"), Some("scsi"), None);
    assert_eq!(sas.protocol, StorageProtocol::Scsi);
    assert_eq!(sas.interconnect, StorageInterconnect::Sas);

    let ufs = classify_storage_connection("sdc", Some("ufs"), Some("scsi"), Some("scsi"), None);
    assert_eq!(ufs.protocol, StorageProtocol::Ufs);
    assert_eq!(ufs.interconnect, StorageInterconnect::Ufs);

    let topology_ufs = classify_storage_connection(
        "sdc",
        None,
        Some("scsi"),
        Some("scsi"),
        Some("/devices/platform/1d84000.ufshc/host0/target0/block/sdc"),
    );
    assert_eq!(topology_ufs.protocol, StorageProtocol::Ufs);
    assert_eq!(topology_ufs.interconnect, StorageInterconnect::Ufs);

    let sd = classify_storage_connection("mmcblk0", None, Some("sd"), Some("mmc"), None);
    assert_eq!(sd.protocol, StorageProtocol::Sd);
    assert_eq!(sd.interconnect, StorageInterconnect::Sd);

    let fibre_channel =
        classify_storage_connection("sdd", Some("fc"), Some("scsi"), Some("scsi"), None);
    assert_eq!(
        fibre_channel.interconnect,
        StorageInterconnect::FibreChannel
    );
    assert_eq!(fibre_channel.protocol, StorageProtocol::Scsi);

    let iscsi = classify_storage_connection("sde", Some("iscsi"), Some("scsi"), Some("scsi"), None);
    assert_eq!(iscsi.interconnect, StorageInterconnect::Iscsi);
    assert_eq!(iscsi.protocol, StorageProtocol::Scsi);

    let tunneled_nvme = classify_storage_connection(
        "nvme5n1",
        Some("pcie"),
        Some("nvme"),
        Some("nvme"),
        Some("/devices/pci/thunderbolt/domain0/nvme/nvme5"),
    );
    assert_eq!(tunneled_nvme.interconnect, StorageInterconnect::PcieTunnel);
    assert_eq!(tunneled_nvme.protocol, StorageProtocol::Nvme);

    let firewire = classify_storage_connection(
        "sdf",
        Some("sbp2"),
        Some("scsi"),
        Some("scsi"),
        Some("/devices/pci/firewire/fw1/host0/block/sdf"),
    );
    assert_eq!(firewire.interconnect, StorageInterconnect::FireWire);

    let future =
        classify_storage_connection("future0", Some("future-fabric"), Some("future"), None, None);
    assert_eq!(future.interconnect, StorageInterconnect::Other);
    assert_eq!(future.protocol, StorageProtocol::Other);
    assert_eq!(future.device_kind, StorageDeviceKind::Unknown);

    let network_virtual =
        classify_storage_connection("nbd0", Some("tcp"), Some("scsi"), Some("nbd"), None);
    assert_eq!(network_virtual.interconnect, StorageInterconnect::Network);
    assert_eq!(network_virtual.device_kind, StorageDeviceKind::Virtual);

    let aggregate = classify_storage_connection("md0", None, None, None, None);
    assert_eq!(aggregate.device_kind, StorageDeviceKind::Aggregate);
    assert_eq!(aggregate.interconnect, StorageInterconnect::Platform);

    let libata = classify_storage_connection(
        "sda",
        None,
        None,
        Some("scsi"),
        Some("/devices/pci0000:00/0000:00:17.0/ata1/host0/target0:0:0/block/sda"),
    );
    assert_eq!(libata.protocol, StorageProtocol::Ata);
    assert_eq!(libata.interconnect, StorageInterconnect::Sata);
    assert_eq!(libata.device_kind, StorageDeviceKind::Physical);

    let contradictory = classify_storage_connection(
        "nvme7n1",
        Some("pcie"),
        Some("ata"),
        Some("nvme"),
        Some("/devices/pci0000:00/0000:01:00.0/nvme/nvme7/nvme7n1"),
    );
    assert_eq!(
        contradictory.protocol,
        StorageProtocol::Unknown,
        "conflicting native-family evidence must not select a command protocol"
    );
    assert_eq!(contradictory.interconnect, StorageInterconnect::Pcie);
    assert_eq!(
        contradictory.device_kind,
        StorageDeviceKind::Unknown,
        "contradictory evidence must not authorize a physical command target"
    );

    for contradictory in [
        classify_storage_connection(
            "nvme8n1",
            Some("sata"),
            None,
            Some("nvme"),
            Some("/devices/pci/nvme/nvme8/nvme8n1"),
        ),
        classify_storage_connection(
            "mmcblk7",
            Some("sata"),
            None,
            Some("mmc"),
            Some("/devices/platform/mmc_host/mmc7/block/mmcblk7"),
        ),
        classify_storage_connection(
            "future-ufs",
            Some("ata"),
            None,
            Some("ufs"),
            Some("/devices/platform/ufshc/host0/block/future-ufs"),
        ),
    ] {
        assert_eq!(contradictory.protocol, StorageProtocol::Unknown);
        assert_eq!(
            contradictory.device_kind,
            StorageDeviceKind::Unknown,
            "transport evidence used as a command protocol must participate in conflict checks"
        );
    }
}

/// `parse_cpulist` decodes the kernel's `cpus` list format used by
/// `/sys/devices/cpu_core/cpus` and `/sys/devices/cpu_atom/cpus`. It feeds the
/// heterogeneous core breakdown (P/E/LP-E counts) and the per-logical-CPU type
/// map, so a mis-parse would directly corrupt the core grid. Pure string fn,
/// no I/O — these cases pin every branch: ranges, singletons, empty input,
/// inverted ranges, and whitespace tolerance.
#[test]
fn test_parse_cpulist_mixed_ranges_and_singletons() {
    // "0-3,5,7-9" → the canonical Arrow Lake-H atom list shape.
    assert_eq!(parse_cpulist("0-3,5,7-9"), vec![0, 1, 2, 3, 5, 7, 8, 9]);
}

#[test]
fn test_parse_cpulist_single_cpu() {
    assert_eq!(parse_cpulist("4"), vec![4]);
}

#[test]
fn test_parse_cpulist_empty_string_returns_empty_vec() {
    // Empty / blank input must NOT push a bogus "0" — it must yield an empty vec
    // so the caller's `len() as u16` reports 0 cores for that bucket.
    assert_eq!(parse_cpulist(""), Vec::<u32>::new());
    assert_eq!(parse_cpulist("   "), Vec::<u32>::new());
}

#[test]
fn test_parse_cpulist_inverted_range_is_rejected() {
    // "3-1" has start > end; the kernel never emits this, but a corrupt read
    // must not panic or wrap. Silently skipped → empty.
    assert_eq!(parse_cpulist("3-1"), Vec::<u32>::new());
    // A valid range alongside an inverted one: the good part survives, the
    // inverted token is dropped.
    assert_eq!(parse_cpulist("0-2,5-3"), vec![0, 1, 2]);
}

#[test]
fn test_parse_cpulist_tolerates_surrounding_whitespace() {
    // sysfs reads often carry a trailing newline; tokens may be padded.
    assert_eq!(parse_cpulist("0-3,5,7-9\n"), vec![0, 1, 2, 3, 5, 7, 8, 9]);
    assert_eq!(
        parse_cpulist("  0-3 , 5 , 7-9  "),
        vec![0, 1, 2, 3, 5, 7, 8, 9]
    );
    // Whitespace inside a range token (around the hyphen) is also tolerated.
    assert_eq!(parse_cpulist("0 - 3"), vec![0, 1, 2, 3]);
}

#[test]
fn test_parse_cpulist_non_numeric_tokens_skipped() {
    // Garbage tokens are silently dropped, never panic.
    assert_eq!(parse_cpulist("0-1,foo,3"), vec![0, 1, 3]);
    assert_eq!(parse_cpulist("garbage"), Vec::<u32>::new());
}

/// `parse_size_to_kb` decodes the kernel's `/sys/.../cache/indexN/size` format
/// (e.g. "256K", "16M", "2G") into Kilobytes. It feeds the L1/L2/L3 cache
/// totals shown in the hardware sidebar, so a mis-parse would under/over-report
/// cache. Pure string fn, no I/O — these cases pin each suffix branch plus the
/// bare-bytes fallback and the garbage guard.
#[test]
fn test_parse_size_to_kb_kib_suffix() {
    assert_eq!(parse_size_to_kb("256K"), 256);
    // Lower-case suffix is accepted (lowercased internally).
    assert_eq!(parse_size_to_kb("256k"), 256);
}

#[test]
fn test_parse_size_to_kb_mib_suffix() {
    // 16 MiB = 16384 KiB.
    assert_eq!(parse_size_to_kb("16M"), 16384);
    assert_eq!(parse_size_to_kb("16m"), 16384);
}

#[test]
fn test_parse_size_to_kb_gib_suffix() {
    // 2 GiB = 2 * 1024 * 1024 KiB = 2097152 KiB.
    assert_eq!(parse_size_to_kb("2G"), 2_097_152);
    assert_eq!(parse_size_to_kb("2g"), 2_097_152);
}

#[test]
fn test_parse_size_to_kb_bare_byte_count_divides_by_1024() {
    // A bare integer is treated as bytes → /1024 to get KiB.
    assert_eq!(parse_size_to_kb("262144"), 256);
}

#[test]
fn test_parse_size_to_kb_garbage_returns_zero() {
    // Unparseable input must NOT panic and must NOT underflow; it returns 0 so
    // a missing/corrupt size node just contributes nothing to the cache total.
    assert_eq!(parse_size_to_kb("garbage"), 0);
    assert_eq!(parse_size_to_kb(""), 0);
}

#[test]
fn test_parse_size_to_kb_trims_surrounding_whitespace() {
    // sysfs reads commonly carry a trailing newline.
    assert_eq!(parse_size_to_kb("256K\n"), 256);
    assert_eq!(parse_size_to_kb("  16M \n"), 16384);
}

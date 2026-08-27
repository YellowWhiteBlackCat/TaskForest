//! Linux block-device classification from sysfs protocol/subsystem/topology
//! evidence.
//!
//! Resolves a typed `StorageConnection` (protocol, interconnect, device kind)
//! and collapses partition names to their physical disk key (for example,
//! `nvme0n1p6` becomes `nvme0n1`).
use taskmanager_core::core::metrics::{
    StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol,
};

/// Classify protocol, outer interconnect, and presentation independently.
///
/// Linux sysfs vocabulary remains inside this adapter. A USB bridge can retain
/// an ATA or SCSI command protocol while still reporting USB as its outer bus.
/// Logical volume kinds never masquerade as hardware transports.
pub fn classify_storage_connection(
    phys: &str,
    transport: Option<&str>,
    protocol: Option<&str>,
    subsystem: Option<&str>,
    topology: Option<&str>,
) -> StorageConnection {
    let phys = phys.trim_start_matches("/dev/");
    if has_numeric_suffix(phys, "dm-") {
        return StorageConnection::new(
            StorageProtocol::Unknown,
            StorageInterconnect::Platform,
            StorageDeviceKind::Virtual,
        );
    }
    if has_numeric_suffix(phys, "md") {
        return StorageConnection::new(
            StorageProtocol::Unknown,
            StorageInterconnect::Platform,
            StorageDeviceKind::Aggregate,
        );
    }

    let transport = transport.unwrap_or_default().trim().to_ascii_lowercase();
    let protocol = protocol.unwrap_or_default().trim().to_ascii_lowercase();
    let subsystem = subsystem.unwrap_or_default().trim().to_ascii_lowercase();
    let topology = topology.unwrap_or_default().to_ascii_lowercase();
    let topology_has_usb = topology
        .split('/')
        .any(|component| component == "usb" || component.starts_with("usb"));
    let topology_has_pcie_tunnel = topology
        .split('/')
        .any(|component| component.contains("thunderbolt") || component.contains("usb4"));
    let topology_has_firewire = topology
        .split('/')
        .any(|component| component.contains("firewire"));
    let topology_has_fibre_channel = topology
        .split('/')
        .any(|component| component.starts_with("fc_host"));
    let topology_has_ufs = topology
        .split('/')
        .any(|component| component.contains("ufs"));
    let topology_has_ata = topology
        .split('/')
        .any(|component| has_numeric_suffix(component, "ata"));
    let native_protocol = if subsystem == "nvme" || is_nvme_namespace(phys) {
        Some(StorageProtocol::Nvme)
    } else if subsystem == "ufs" || topology_has_ufs {
        Some(StorageProtocol::Ufs)
    } else if subsystem == "mmc" || has_numeric_suffix(phys, "mmcblk") {
        Some(if protocol == "sd" {
            StorageProtocol::Sd
        } else {
            StorageProtocol::Mmc
        })
    } else {
        None
    };
    let explicit_protocol = explicit_protocol_evidence(&protocol);
    let transport_protocol = explicit_protocol_evidence(&transport);
    let contradictory_native_protocol = native_protocol.is_some_and(|native| {
        [explicit_protocol, transport_protocol]
            .into_iter()
            .flatten()
            .any(|explicit| !native_protocol_accepts(native, explicit))
    });

    let command_protocol = if contradictory_native_protocol {
        StorageProtocol::Unknown
    } else if subsystem == "nvme" || is_nvme_namespace(phys) {
        StorageProtocol::Nvme
    } else if subsystem == "ufs"
        || transport.contains("ufs")
        || protocol.contains("ufs")
        || topology_has_ufs
    {
        StorageProtocol::Ufs
    } else if subsystem == "mmc" || has_numeric_suffix(phys, "mmcblk") {
        if protocol == "sd" {
            StorageProtocol::Sd
        } else {
            StorageProtocol::Mmc
        }
    } else if transport.contains("sata")
        || protocol.contains("sata")
        || transport == "ata"
        || protocol == "ata"
        || topology_has_ata
    {
        StorageProtocol::Ata
    } else if transport.contains("sas")
        || protocol.contains("sas")
        || transport.contains("scsi")
        || protocol.contains("scsi")
        || subsystem == "scsi"
    {
        StorageProtocol::Scsi
    } else if !protocol.is_empty() {
        StorageProtocol::Other
    } else {
        StorageProtocol::Unknown
    };
    let interconnect = if topology_has_pcie_tunnel {
        StorageInterconnect::PcieTunnel
    } else if topology_has_usb || transport == "usb" || protocol == "usb" {
        StorageInterconnect::Usb
    } else if topology_has_fibre_channel
        || transport == "fc"
        || transport.contains("fibre_channel")
        || transport.contains("fibre channel")
    {
        StorageInterconnect::FibreChannel
    } else if transport.contains("iscsi") || protocol.contains("iscsi") {
        StorageInterconnect::Iscsi
    } else if transport.contains("tcp")
        || transport.contains("network")
        || subsystem == "nbd"
        || subsystem == "rbd"
    {
        StorageInterconnect::Network
    } else if topology_has_firewire || transport.contains("firewire") || transport.contains("sbp") {
        StorageInterconnect::FireWire
    } else if subsystem == "ufs" || transport.contains("ufs") || topology_has_ufs {
        StorageInterconnect::Ufs
    } else if subsystem == "mmc" || has_numeric_suffix(phys, "mmcblk") {
        if protocol == "sd" {
            StorageInterconnect::Sd
        } else {
            StorageInterconnect::Mmc
        }
    } else if subsystem == "virtio" || has_letter_suffix(phys, "vd") {
        StorageInterconnect::Virtio
    } else if transport.contains("sas")
        || (!contradictory_native_protocol && protocol.contains("sas"))
    {
        StorageInterconnect::Sas
    } else if transport.contains("sata")
        || transport == "ata"
        || topology_has_ata
        || (!contradictory_native_protocol && (protocol.contains("sata") || protocol == "ata"))
    {
        StorageInterconnect::Sata
    } else if subsystem == "ide" || has_letter_suffix(phys, "hd") {
        StorageInterconnect::Ide
    } else if transport == "pcie" {
        StorageInterconnect::Pcie
    } else if !transport.is_empty() || !subsystem.is_empty() {
        StorageInterconnect::Other
    } else {
        StorageInterconnect::Unknown
    };
    let device_kind = if contradictory_native_protocol {
        // Conflicting high-confidence native-family evidence must not
        // authorize either telemetry commands or a mutation target. Preserve
        // the independently observed interconnect while marking presentation
        // authority unknown.
        StorageDeviceKind::Unknown
    } else if interconnect == StorageInterconnect::Virtio
        || matches!(subsystem.as_str(), "nbd" | "rbd")
    {
        StorageDeviceKind::Virtual
    } else if interconnect == StorageInterconnect::Other
        || command_protocol == StorageProtocol::Other
    {
        StorageDeviceKind::Unknown
    } else if command_protocol != StorageProtocol::Unknown
        || interconnect != StorageInterconnect::Unknown
    {
        StorageDeviceKind::Physical
    } else {
        StorageDeviceKind::Unknown
    };
    StorageConnection::new(command_protocol, interconnect, device_kind)
}

fn explicit_protocol_evidence(value: &str) -> Option<StorageProtocol> {
    if value.contains("nvme") {
        Some(StorageProtocol::Nvme)
    } else if value.contains("sata") || value == "ata" {
        Some(StorageProtocol::Ata)
    } else if value.contains("sas") || value.contains("scsi") {
        Some(StorageProtocol::Scsi)
    } else if value.contains("ufs") {
        Some(StorageProtocol::Ufs)
    } else if value == "mmc" {
        Some(StorageProtocol::Mmc)
    } else if value == "sd" {
        Some(StorageProtocol::Sd)
    } else {
        None
    }
}

fn native_protocol_accepts(native: StorageProtocol, explicit: StorageProtocol) -> bool {
    match native {
        StorageProtocol::Ufs => {
            matches!(explicit, StorageProtocol::Ufs | StorageProtocol::Scsi)
        }
        StorageProtocol::Mmc | StorageProtocol::Sd => {
            matches!(explicit, StorageProtocol::Mmc | StorageProtocol::Sd)
        }
        _ => native == explicit,
    }
}

/// Human-readable media label derived from the typed connection plus the kernel's
/// optional rotational bit.
pub fn describe_disk_type(connection: StorageConnection, rotational: Option<bool>) -> String {
    let media = match rotational {
        Some(true) => "HDD",
        Some(false) => "SSD",
        None => "Storage",
    };
    match (
        connection.device_kind,
        connection.interconnect,
        connection.protocol,
    ) {
        (StorageDeviceKind::Virtual, StorageInterconnect::Platform, _) => {
            "Device Mapper".to_string()
        }
        (StorageDeviceKind::Aggregate, StorageInterconnect::Platform, _) => {
            "Software RAID".to_string()
        }
        (_, StorageInterconnect::Usb, _) => format!("USB {media}"),
        (_, StorageInterconnect::Sata, _) => format!("SATA {media}"),
        (_, StorageInterconnect::Sas, _) => format!("SAS {media}"),
        (_, StorageInterconnect::Mmc | StorageInterconnect::Sd, _) => "eMMC/SD".to_string(),
        (_, StorageInterconnect::Ufs, _) => "UFS Storage".to_string(),
        (_, StorageInterconnect::Ide, _) => format!("IDE {media}"),
        (_, StorageInterconnect::Virtio, _) => "Virtio Disk".to_string(),
        (_, _, StorageProtocol::Nvme) => "NVMe SSD".to_string(),
        (_, _, StorageProtocol::Ata) => format!("SATA {media}"),
        (_, _, StorageProtocol::Scsi) => format!("SCSI {media}"),
        (_, _, StorageProtocol::Mmc | StorageProtocol::Sd) => "eMMC/SD".to_string(),
        (_, _, StorageProtocol::Ufs) => "UFS Storage".to_string(),
        _ => "Unknown Block Device".to_string(),
    }
}

/// Test-support label for callers that only have a physical name. Ambiguous
/// `sd*` names remain unknown instead of being mislabeled SATA.
#[cfg(feature = "test-support")]
pub fn classify_disk_type(phys: &str) -> String {
    let connection = classify_storage_connection(phys, None, None, None, None);
    describe_disk_type(connection, None)
}

/// Collapse a partition device name to its physical disk name.
/// `nvme0n1p6` -> `nvme0n1`, `mmcblk0p1` -> `mmcblk0`, `sda3` ->
/// `sda`; whole devices such as `nvme0n1`, `mmcblk0`, `sda`, `md0`,
/// `loop0`, `zram0`, and `sr0` stay as-is.
pub fn physical_disk_key(name: &str) -> String {
    let trimmed = name.trim();
    let n = trimmed.strip_prefix("/dev/").unwrap_or(trimmed);

    if let Some((base, partition)) = n.rsplit_once('p')
        && is_partition_number(partition)
        && is_p_delimited_disk(base)
    {
        return base.to_string();
    }

    let letter_count = n.bytes().take_while(u8::is_ascii_lowercase).count();
    let (base, partition) = n.split_at(letter_count);
    if is_letter_disk(base) && is_partition_number(partition) {
        return base.to_string();
    }

    n.to_string()
}

fn is_partition_number(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_p_delimited_disk(base: &str) -> bool {
    is_nvme_namespace(base)
        || has_numeric_suffix(base, "mmcblk")
        || has_numeric_suffix(base, "nvdimm")
}

fn is_nvme_namespace(base: &str) -> bool {
    let Some(rest) = base.strip_prefix("nvme") else {
        return false;
    };
    let controller_digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if controller_digits == 0 {
        return false;
    }
    let Some(namespace) = rest[controller_digits..].strip_prefix('n') else {
        return false;
    };
    is_partition_number(namespace)
}

fn has_numeric_suffix(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(is_partition_number)
}

fn is_letter_disk(base: &str) -> bool {
    ["sd", "hd", "vd", "xvd", "ubd"]
        .iter()
        .any(|prefix| has_letter_suffix(base, prefix))
}

fn has_letter_suffix(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_lowercase()))
}

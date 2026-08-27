//! Platform-neutral storage identity, topology, and command-target contracts.
//!
//! A storage path has multiple independent axes. The command protocol can be
//! ATA, SCSI, NVMe, MMC, or UFS while the outer interconnect can be SATA, SAS,
//! PCIe, USB, or a native platform bus. Logical volumes and arrays are a
//! presentation kind, not a transport. Keeping those axes separate lets native
//! adapters describe bridges and future hardware without vendor allowlists.

use serde::{Deserialize, Serialize};

use crate::core::{DeviceGeneration, DeviceId, StorageDeviceKey};

/// Command/health protocol understood by the addressed storage device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageProtocol {
    Nvme,
    Ata,
    Scsi,
    Mmc,
    Sd,
    Ufs,
    Other,
    #[default]
    Unknown,
}

/// Outermost interconnect visible to the operating-system adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageInterconnect {
    Pcie,
    Sata,
    Sas,
    Usb,
    Mmc,
    Sd,
    Ufs,
    Ide,
    Virtio,
    FibreChannel,
    Iscsi,
    Network,
    /// PCIe tunneled through a hot-pluggable fabric such as Thunderbolt or
    /// USB4. This remains distinct from native USB mass storage.
    PcieTunnel,
    FireWire,
    Platform,
    Other,
    #[default]
    Unknown,
}

/// How the operating system presents the storage object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageDeviceKind {
    Physical,
    Virtual,
    Aggregate,
    #[default]
    Unknown,
}

/// Scope in which a device identity is expected to remain stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageIdentityStability {
    /// Hardware-backed identity expected to survive detach, reattach, and
    /// native locator renumbering.
    Persistent,
    /// Identity is only known to be stable while the current OS attachment is
    /// present. It must not be used to claim reorder-safe re-identification.
    Attachment,
    #[default]
    Unknown,
}

/// Orthogonal storage topology axes.
///
/// A USB SAT bridge is represented as `protocol = Ata` and
/// `interconnect = Usb`; a SAS disk is `Scsi` over `Sas`; device-mapper is a
/// `Virtual` presentation over the platform stack. Unknown evidence remains
/// unknown on the affected axis rather than forcing a guessed transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StorageConnection {
    #[serde(default)]
    pub protocol: StorageProtocol,
    #[serde(default)]
    pub interconnect: StorageInterconnect,
    #[serde(default)]
    pub device_kind: StorageDeviceKind,
}

impl StorageConnection {
    #[must_use]
    pub const fn new(
        protocol: StorageProtocol,
        interconnect: StorageInterconnect,
        device_kind: StorageDeviceKind,
    ) -> Self {
        Self {
            protocol,
            interconnect,
            device_kind,
        }
    }
}

/// Physical lifecycle identity plus an opaque native command locator.
///
/// Providers receive the full identity/generation pair when polling or
/// starting a job, so an adapter can revalidate a locator before acting after
/// hot-plug or native renumbering. The locator itself remains platform-owned.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageDeviceTarget {
    #[serde(default)]
    pub device_id: DeviceId,
    #[serde(default)]
    pub device_generation: DeviceGeneration,
    #[serde(rename = "device_key", alias = "locator")]
    pub locator: StorageDeviceKey,
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_storage_tests.rs"]
mod tests;

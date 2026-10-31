//! On-demand SMBIOS memory inventory snapshots (the MemorySmbios request
//! lane).
//!
//! Unlike the periodic [`MemoryModuleObservations`] projection (fed by the
//! unprivileged udev + world-readable DMI merge), these snapshots answer a
//! frontend-paced request/response lane backed by the privileged SMBIOS memory
//! helper (ADR-023, permission-model Boundary 2). The provider answers with
//! exactly one snapshot — real slot/module rows on success, a typed failure
//! otherwise — so no consumer can mistake a denied or missing helper for an
//! empty inventory.

use serde::{Deserialize, Serialize};

use crate::core::FailureKind;

/// One typed reason a memory-inventory request could not produce live rows.
///
/// `detail` is a host-specific diagnostic; `kind` alone drives every
/// state-machine decision so consumers never parse text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmbiosMemoryFailure {
    pub kind: FailureKind,
    pub detail: String,
}

/// System/board identity facts from the SMBIOS type-0/1/2 records, carried by
/// the same privileged lane as the module inventory (those `/sys/class/dmi/id`
/// nodes are root-only: serials, UUID, asset tag, SKU). Every field is `None`
/// when the source record did not state it or its table is absent on this
/// host — never a fabricated zero or empty string. The escalation seam maps
/// its own parsed identity struct onto this core type field-by-field at the
/// provider crossing (one fact, one authority: core owns the typed fact).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmiIdentityFacts {
    /// BIOS vendor string (type 0).
    pub bios_vendor: Option<String>,
    /// BIOS version string (type 0).
    pub bios_version: Option<String>,
    /// BIOS release date string (type 0).
    pub bios_date: Option<String>,
    /// Board manufacturer string (type 2).
    pub board_manufacturer: Option<String>,
    /// Board product name string (type 2).
    pub board_product: Option<String>,
    /// Board serial number string (type 2).
    pub board_serial: Option<String>,
    /// Board asset tag string (type 2).
    pub board_asset_tag: Option<String>,
    /// System manufacturer string (type 1).
    pub system_manufacturer: Option<String>,
    /// System product name string (type 1).
    pub system_product: Option<String>,
    /// System serial number string (type 1).
    pub system_serial: Option<String>,
    /// System UUID, canonical hyphenated lowercase (type 1).
    pub system_uuid: Option<String>,
    /// System SKU number string (type 1).
    pub system_sku: Option<String>,
    /// System family string (type 1).
    pub system_family: Option<String>,
}

/// One populated memory-module row (SMBIOS type 17). Every optional fact is
/// `None` when the source record did not carry it — never a zero or filler.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SmbiosModuleRow {
    /// SMBIOS slot index (the `17-N` entry suffix).
    pub slot: u32,
    /// Module capacity in MB.
    pub size_mb: Option<u32>,
    /// Maximum speed in MT/s.
    pub speed_mts: Option<u32>,
    /// Currently configured speed in MT/s (the live speed).
    pub configured_speed_mts: Option<u32>,
    /// Manufacturer label, e.g. `"Samsung"`.
    pub manufacturer: Option<String>,
    /// Module serial number.
    pub serial_number: Option<String>,
    /// Module part number.
    pub part_number: Option<String>,
    /// Form factor label, e.g. `"SODIMM"`.
    pub form_factor: Option<String>,
    /// Memory type label, e.g. `"DDR5"`.
    pub memory_type: Option<String>,
    /// Device locator string, e.g. `"ChannelA-DIMM0"`.
    pub locator: Option<String>,
}

/// The answer to one memory-inventory request.
///
/// `modules` carries only populated slots; `slots_total`/`slots_used` count
/// the full type-17 population. `identity` carries the system/board facts
/// from the same walk (`None` when the host has no type-0/1/2 entries).
/// Any `failure` means no row in this snapshot is real.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SmbiosMemorySnapshot {
    /// Total type-17 records seen (populated and empty slots).
    pub slots_total: u32,
    /// Records reporting a populated module.
    pub slots_used: u32,
    /// The populated-module rows, sorted by slot.
    pub modules: Vec<SmbiosModuleRow>,
    /// System/board identity facts; `#[serde(default)]` so snapshots
    /// serialized before the field existed still decode.
    #[serde(default)]
    pub identity: Option<DmiIdentityFacts>,
    pub failure: Option<SmbiosMemoryFailure>,
}

impl SmbiosMemorySnapshot {
    /// Successful read: real inventory, never a failure tag.
    #[must_use]
    pub fn success(
        slots_total: u32,
        slots_used: u32,
        modules: Vec<SmbiosModuleRow>,
        identity: Option<DmiIdentityFacts>,
    ) -> Self {
        Self {
            slots_total,
            slots_used,
            modules,
            identity,
            failure: None,
        }
    }

    /// Failed read: a typed reason, never a fabricated row.
    #[must_use]
    pub fn failed(kind: FailureKind, detail: impl Into<String>) -> Self {
        Self {
            slots_total: 0,
            slots_used: 0,
            modules: Vec::new(),
            identity: None,
            failure: Some(SmbiosMemoryFailure {
                kind,
                detail: detail.into(),
            }),
        }
    }

    /// True when this snapshot carries a real inventory.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.failure.is_none()
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_smbios_memory_tests.rs"]
mod tests;

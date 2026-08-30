//! Linux chipset (platform/PCH/SoC) model resolution for the firmware
//! fragment.
//!
//! The chipset is proven from PCI bridge identity plus the shared hwdata
//! `pci.ids` database: the `0000:00:00.0` host bridge names the platform on
//! AMD SoCs, while an Intel host bridge often carries only a generic topology
//! label and the `0000:00:1f.0` PCH ISA bridge holds the marketing name
//! (e.g. "Z690 Chipset"). Every input is path-injected, so synthetic fixture
//! roots never read the host's PCI tree or database.

use std::path::Path;

use super::super::pci_ids;
use super::{FailureSummary, InventoryPaths};

/// PCI slot of the host bridge (bus 0, device 0, function 0).
const HOST_BRIDGE_SLOT: &str = "0000:00:00.0";
/// PCI slot of the classic Intel PCH ISA bridge (bus 0, device 0x1f, func 0).
const INTEL_ISA_BRIDGE_SLOT: &str = "0000:00:1f.0";
/// PCI vendor ID of Intel host bridges.
const PCI_VENDOR_INTEL: u16 = 0x8086;

/// Words that describe a PCI bridge's role without naming a platform. A hwdata
/// label made only of these tokens is topology filler ("Host Bridge",
/// "Uncore"), not a chipset marketing name.
const GENERIC_LABEL_WORDS: &[&str] = &[
    "host",
    "bridge",
    "uncore",
    "dram",
    "registers",
    "register",
    "processor",
    "processing",
    "cpu",
    "core",
    "controller",
    "lpc",
    "espi",
    "isa",
    "pci",
    "pcie",
    "express",
    "root",
    "complex",
    "port",
    "ports",
    "hub",
    "device",
    "devices",
    "memory",
    "interface",
    "family",
    "generation",
    "gen",
    "series",
    "pch",
    "soc",
    "chipset",
    "communication",
    "interconnect",
];

/// Whether a cleaned hwdata label carries at least one platform-identifying
/// token (e.g. "Z690", "Starship") rather than only generic role words.
fn has_marketing_token(label: &str) -> bool {
    label
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .any(|token| !GENERIC_LABEL_WORDS.contains(&token.to_ascii_lowercase().as_str()))
}

/// Cleaned hwdata label for one bridge identity, or `None` when the database
/// does not cover the vendor/device pair.
fn bridge_label(pci_ids_text: &str, vendor: u16, device: u16) -> Option<String> {
    pci_ids::pci_ids_device_name(pci_ids_text, vendor, device)
        .as_deref()
        .and_then(pci_ids::marketing_name_from_pci_label)
}

/// Pure chipset-model selection over the two candidate bridge identities.
///
/// Priority, first honest hit wins: the host bridge at `00:00.0` names the
/// platform on AMD SoCs and on Intel parts whose hwdata label carries a
/// marketing token; an Intel host bridge with only a generic topology label
/// falls back to the PCH ISA bridge at `00:1f.0`, which hwdata names after the
/// chipset. A final label that is empty or still generic is an honest `None` —
/// the raw hwdata marketing string is the authority and nothing is prepended.
pub(super) fn chipset_model_from_bridges(
    pci_ids_text: &str,
    host_bridge: Option<(u16, u16)>,
    isa_bridge: Option<(u16, u16)>,
) -> Option<String> {
    let (host_vendor, host_device) = host_bridge?;
    let host_label = bridge_label(pci_ids_text, host_vendor, host_device);
    let candidate = if host_vendor == PCI_VENDOR_INTEL {
        if host_label
            .as_ref()
            .is_some_and(|label| has_marketing_token(label))
        {
            host_label
        } else {
            isa_bridge.and_then(|(vendor, device)| bridge_label(pci_ids_text, vendor, device))
        }
    } else {
        host_label
    };
    candidate.filter(|label| has_marketing_token(label))
}

/// Read one hex `u16` sysfs attribute (`vendor`/`device`) of a PCI slot.
fn read_slot_id(
    pci_devices_root: &Path,
    slot: &str,
    file: &str,
    failures: &mut FailureSummary,
) -> Option<u16> {
    let raw = super::read_optional_text(&pci_devices_root.join(slot).join(file), failures)?;
    pci_ids::parse_pci_id(&raw)
}

/// Collect the chipset model from one PCI devices root plus the injected
/// pci.ids database candidates. A missing root, slot, or database is an honest
/// `None`; the database lookup happens only after the host-bridge identity
/// was proven, so an absent tree never touches pci.ids files.
pub(super) fn collect_chipset_model(
    paths: &InventoryPaths,
    failures: &mut FailureSummary,
) -> Option<String> {
    let root = &paths.pci_devices_root;
    let host_bridge = read_slot_id(root, HOST_BRIDGE_SLOT, "vendor", failures).zip(read_slot_id(
        root,
        HOST_BRIDGE_SLOT,
        "device",
        failures,
    ));
    let isa_bridge = read_slot_id(root, INTEL_ISA_BRIDGE_SLOT, "vendor", failures).zip(
        read_slot_id(root, INTEL_ISA_BRIDGE_SLOT, "device", failures),
    );
    // The database is read only after a host bridge was proven, so an absent
    // PCI tree never touches pci.ids files. The `?` discards the value on
    // purpose: the selection below receives it again by argument.
    host_bridge?;
    let pci_ids_text = pci_ids::read_pci_ids_text(&paths.pci_ids_candidates)?;
    chipset_model_from_bridges(&pci_ids_text, host_bridge, isa_bridge)
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_hardware_inventory_chipset_tests.rs"]
mod tests;

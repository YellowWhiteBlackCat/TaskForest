//! Shared, bounded `pci.ids` (hwdata) device-name resolution.
//!
//! One authority for turning a PCI vendor/device pair into the hwdata device
//! label and its marketing segment. The GPU identity reader and the chipset
//! (platform/PCH) inventory both consume this module; neither keeps a private
//! copy of the database format.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

/// Upper bound accepted for one pci.ids database read. The packaged hwdata
/// file is ~1.2 MiB; 8 MiB admits distribution variants while rejecting
/// host-specific dumps without unbounded allocation.
const MAX_PCI_IDS_BYTES: usize = 8 * 1024 * 1024;

/// The packaged pci.ids locations, in hwdata-first distribution order.
pub(super) fn native_pci_ids_candidates() -> [PathBuf; 3] {
    [
        PathBuf::from("/usr/share/hwdata/pci.ids"),
        PathBuf::from("/usr/share/misc/pci.ids"),
        PathBuf::from("/usr/share/pci.ids"),
    ]
}

/// Parse one PCI ID from sysfs or a pci.ids token without accepting overflow
/// or trailing syntax.
pub(super) fn parse_pci_id(raw: &str) -> Option<u16> {
    let value = raw.trim().strip_prefix("0x").unwrap_or(raw.trim());
    (value.len() == 4)
        .then(|| u16::from_str_radix(value, 16).ok())
        .flatten()
}

/// Resolve a device name from the bounded, read-only `pci.ids` text format.
/// Device names are only accepted under the requested vendor; subsystem lines
/// are intentionally ignored.
pub(super) fn pci_ids_device_name(text: &str, vendor: u16, device: u16) -> Option<String> {
    let mut vendor_match = false;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with(['\t', ' ']) {
            vendor_match = line.split_whitespace().next().and_then(parse_pci_id) == Some(vendor);
            continue;
        }
        if !vendor_match || !line.starts_with('\t') || line.starts_with("\t\t") {
            continue;
        }
        let mut fields = line.trim().splitn(2, char::is_whitespace);
        let Some(raw_device) = fields.next() else {
            continue;
        };
        if parse_pci_id(raw_device) != Some(device) {
            continue;
        }
        return fields
            .next()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned);
    }
    None
}

/// Prefer the bracketed marketing name used by modern pci.ids entries, while
/// preserving the complete device label for entries without one.
pub(super) fn marketing_name_from_pci_label(label: &str) -> Option<String> {
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    if let Some(start) = label.rfind('[')
        && let Some(end) = label[start + 1..].find(']')
    {
        let marketing = label[start + 1..start + 1 + end].trim();
        if !marketing.is_empty() {
            return Some(marketing.to_owned());
        }
    }
    Some(label.to_owned())
}

/// Read the first bounded, decodable pci.ids database among `candidates`.
/// `None` when no candidate exists or parses — an honest absence, never a
/// partial or fabricated database.
pub(super) fn read_pci_ids_text(candidates: &[PathBuf]) -> Option<String> {
    for path in candidates {
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_PCI_IDS_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() > MAX_PCI_IDS_BYTES
        {
            continue;
        }
        if let Ok(text) = String::from_utf8(bytes) {
            return Some(text);
        }
    }
    None
}

/// Resolve a marketing device name from the packaged pci.ids databases.
/// `None` when no database is installed or the pair is unknown.
pub(super) fn read_pci_marketing_name(vendor: u16, device: u16) -> Option<String> {
    let text = read_pci_ids_text(&native_pci_ids_candidates())?;
    pci_ids_device_name(&text, vendor, device)
        .and_then(|label| marketing_name_from_pci_label(&label))
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_hardware_pci_ids_tests.rs"]
mod tests;

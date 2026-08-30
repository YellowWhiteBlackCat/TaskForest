//! Bounded firmware/DMI collection helpers for the Linux hardware inventory.

use std::fs;
use std::io::Read;
use std::path::Path;

use taskmanager_core::core::hardware::FirmwareInfo;

use super::inventory::{FailureSummary, read_optional_text};

/// The seven optional DMI text facts the firmware source folds into
/// `FirmwareInfo`; each is independently absent when the sysfs node cannot be
/// read.
pub type DmiTextFacts = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Read product/firmware/board DMI facts from one sysfs DMI root.
#[must_use]
pub fn read_dmi_facts(root: &Path, failures: &mut FailureSummary) -> DmiTextFacts {
    let mut text = |name: &str| read_optional_text(&root.join(name), failures);
    (
        text("product_name"),
        text("product_version"),
        text("bios_vendor"),
        text("bios_version"),
        text("board_vendor"),
        text("board_name"),
        text("bios_date"),
        text("board_version"),
    )
}

/// Collect the firmware fragment (DMI text facts + PCI chipset model +
/// Secure Boot probe) and count the proven fields for the inventory source
/// outcome.
#[must_use]
pub fn collect_firmware_facts(
    virtualization: Option<&str>,
    chipset: Option<String>,
    dmi_root: Option<&Path>,
    efivars_root: &Path,
    failures: &mut FailureSummary,
) -> (FirmwareInfo, usize) {
    let facts = dmi_root.map_or((None, None, None, None, None, None, None, None), |root| {
        read_dmi_facts(root, failures)
    });
    let secure_boot = probe_secure_boot_from(efivars_root);
    let observed = [
        virtualization.is_some(),
        chipset.is_some(),
        facts.0.is_some(),
        facts.1.is_some(),
        facts.2.is_some(),
        facts.3.is_some(),
        facts.4.is_some(),
        facts.5.is_some(),
        facts.6.is_some(),
        facts.7.is_some(),
        secure_boot.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    (
        FirmwareInfo {
            virtualization: virtualization.map(str::to_string),
            chipset,
            product_name: facts.0,
            product_version: facts.1,
            firmware_vendor: facts.2,
            firmware_version: facts.3,
            motherboard_vendor: facts.4,
            motherboard_model: facts.5,
            firmware_release_date: facts.6,
            motherboard_version: facts.7,
            secure_boot,
        },
        observed,
    )
}

/// Read the UEFI `SecureBoot` variable through sysfs when the kernel exposes
/// it and the caller may read it. Format: 4-byte variable attributes followed
/// by a single value byte (`1` = enabled, `0` = disabled).
///
/// A permission-denied or absent efivar is `None` — unknown, never a guessed
/// value. Bounded to the fixed 5-byte header+value read.
#[must_use]
pub fn probe_secure_boot_from(efivars_root: &Path) -> Option<bool> {
    let mut bytes = [0_u8; 5];
    let mut file =
        fs::File::open(efivars_root.join("SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c"))
            .ok()?;
    let mut read = 0_usize;
    loop {
        let chunk = file.read(&mut bytes[read..]).ok()?;
        if chunk == 0 {
            break;
        }
        read += chunk;
        if read >= bytes.len() {
            break;
        }
    }
    match bytes.get(4).copied()? {
        1 => Some(true),
        0 => Some(false),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_hardware_firmware_tests.rs"]
mod tests;

use std::fs;
use std::path::PathBuf;

use super::super::{
    FirmwareSource, InventoryContext, InventoryPaths, InventorySource, SystemProbe,
};
use super::*;
use taskmanager_core::core::source::SourceOutcome;

/// hwdata-style fixture database: an Intel host bridge with only a generic
/// topology label plus a Z690 PCH ISA bridge carrying the marketing name, and
/// an AMD SoC host bridge that names the platform itself.
const MINI_PCI_IDS: &str = "\
8086  Intel Corporation
\t1237  Host Bridge
\t460d  Alder Lake-S Host Bridge
\t7a00  Z690 Chipset LPC/eSPI Controller
\t7a01  Intel Chipset [Z790]
\t7a02  ISA Bridge
1022  Advanced Micro Devices, Inc. [AMD]
\t1480  Starship/Matisse Root Complex
\t1483  Host Bridge
";

#[test]
fn intel_generic_host_bridge_falls_back_to_the_pch_isa_bridge() {
    assert_eq!(
        chipset_model_from_bridges(MINI_PCI_IDS, Some((0x8086, 0x1237)), Some((0x8086, 0x7a00)))
            .as_deref(),
        Some("Z690 Chipset LPC/eSPI Controller")
    );
}

#[test]
fn intel_host_bridge_with_a_marketing_token_wins_before_the_isa_bridge() {
    assert_eq!(
        chipset_model_from_bridges(MINI_PCI_IDS, Some((0x8086, 0x460d)), Some((0x8086, 0x7a00)))
            .as_deref(),
        Some("Alder Lake-S Host Bridge")
    );
}

#[test]
fn bracketed_marketing_segment_is_preferred_like_the_gpu_path() {
    assert_eq!(
        chipset_model_from_bridges(MINI_PCI_IDS, Some((0x8086, 0x1237)), Some((0x8086, 0x7a01)))
            .as_deref(),
        Some("Z790")
    );
}

#[test]
fn amd_host_bridge_label_is_the_chipset_without_isa_fallback() {
    assert_eq!(
        chipset_model_from_bridges(MINI_PCI_IDS, Some((0x1022, 0x1480)), None).as_deref(),
        Some("Starship/Matisse Root Complex")
    );
    // A generic non-Intel host label never falls back to the ISA bridge: the
    // PCH naming rule is Intel-specific.
    assert_eq!(
        chipset_model_from_bridges(MINI_PCI_IDS, Some((0x1022, 0x1483)), Some((0x8086, 0x7a00))),
        None
    );
}

#[test]
fn generic_or_unknown_labels_stay_an_honest_absence() {
    // Every candidate label is topology filler only.
    assert_eq!(
        chipset_model_from_bridges(MINI_PCI_IDS, Some((0x8086, 0x1237)), Some((0x8086, 0x7a02))),
        None
    );
    // Unknown host bridge identity: no host, no chipset.
    assert_eq!(
        chipset_model_from_bridges(MINI_PCI_IDS, None, Some((0x8086, 0x7a00))),
        None
    );
    // A device missing from the database cannot be named.
    assert_eq!(
        chipset_model_from_bridges(MINI_PCI_IDS, Some((0x8086, 0xFFFF)), None),
        None
    );
}

struct FixtureDir(PathBuf);

impl FixtureDir {
    fn new(name: &str) -> Self {
        let path = crate::test_support::repo_temp_dir()
            .join(format!("taskmanager-chipset-{name}-{}", std::process::id()));
        fs::create_dir_all(&path).expect("fixture directory should be created");
        Self(path)
    }

    fn write_pci_slot(&self, slot: &str, vendor: &str, device: &str) {
        let dir = self.0.join("pci-devices").join(slot);
        fs::create_dir_all(&dir).expect("PCI slot fixture should be created");
        fs::write(dir.join("vendor"), vendor).expect("vendor fixture");
        fs::write(dir.join("device"), device).expect("device fixture");
    }

    fn write_pci_ids(&self) {
        fs::write(self.0.join("pci.ids"), MINI_PCI_IDS).expect("pci.ids fixture");
    }

    fn paths(&self) -> InventoryPaths {
        InventoryPaths {
            dmi_roots: [self.0.join("dmi"), PathBuf::new()],
            efivars_root: self.0.join("efivars"),
            pci_devices_root: self.0.join("pci-devices"),
            pci_ids_candidates: [self.0.join("pci.ids"), PathBuf::new(), PathBuf::new()],
            ..InventoryPaths::default()
        }
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn chipset_collection_reads_sysfs_bridge_identity_and_the_database() {
    let fixture = FixtureDir::new("z690");
    fixture.write_pci_slot("0000:00:00.0", "0x8086\n", "0x1237\n");
    fixture.write_pci_slot("0000:00:1f.0", "0x8086\n", "0x7a00\n");
    fixture.write_pci_ids();

    assert_eq!(
        collect_chipset_model(&fixture.paths(), &mut FailureSummary::default()).as_deref(),
        Some("Z690 Chipset LPC/eSPI Controller")
    );
}

#[test]
fn chipset_collection_needs_the_host_bridge_the_isa_bridge_and_the_database() {
    let mut failures = FailureSummary::default();
    // No PCI tree at all: an honest absence that must not surface as a
    // firmware-source failure.
    let empty = FixtureDir::new("empty");
    assert_eq!(collect_chipset_model(&empty.paths(), &mut failures), None);
    assert!(
        failures.failure.is_none(),
        "a missing PCI tree is an absence, not a failure"
    );
    // Host bridge present but no database file installed.
    let no_ids = FixtureDir::new("no-ids");
    no_ids.write_pci_slot("0000:00:00.0", "0x1022\n", "0x1480\n");
    assert_eq!(
        collect_chipset_model(&no_ids.paths(), &mut FailureSummary::default()),
        None
    );
    // Database present but the host bridge identity is unparseable.
    let malformed = FixtureDir::new("malformed");
    malformed.write_pci_slot("0000:00:00.0", "not-a-pci-id\n", "0x1480\n");
    malformed.write_pci_ids();
    assert_eq!(
        collect_chipset_model(&malformed.paths(), &mut FailureSummary::default()),
        None
    );
}

#[test]
fn firmware_source_carries_the_chipset_fact_into_the_fragment() {
    let fixture = FixtureDir::new("source");
    fixture.write_pci_slot("0000:00:00.0", "0x1022\n", "0x1480\n");
    fixture.write_pci_ids();
    let context = InventoryContext {
        system: &SystemProbe::default(),
        paths: &fixture.paths(),
        virtualization: None,
    };

    let fragment = FirmwareSource.collect(&context);

    assert_eq!(
        fragment.value.chipset.as_deref(),
        Some("Starship/Matisse Root Complex")
    );
    assert_eq!(fragment.status.outcome, SourceOutcome::Available);
    assert!(fragment.status.item_count >= 1);
}

/// The fixture roots are authoritative: a synthetic DMI root with no PCI tree
/// must leave the chipset honestly absent (no host reads in tests).
#[test]
fn firmware_source_without_a_pci_tree_leaves_the_chipset_absent() {
    let fixture = FixtureDir::new("absent");
    let context = InventoryContext {
        system: &SystemProbe::default(),
        paths: &fixture.paths(),
        virtualization: None,
    };

    let fragment = FirmwareSource.collect(&context);

    assert_eq!(fragment.value.chipset, None);
}

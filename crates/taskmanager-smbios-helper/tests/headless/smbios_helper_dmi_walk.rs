use super::*;
use crate::test_support::repo_temp_dir;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// A unique fixture entries-root under the repository `.tmp/` scratch.
fn fixture_root(tag: &str) -> PathBuf {
    repo_temp_dir().join(format!("tm_smbios_walk_{tag}"))
}

/// Write one `<root>/<name>/raw` entry file.
fn write_entry(root: &Path, name: &str, raw: &[u8]) {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("raw"), raw).unwrap();
}

/// A populated-module raw type-17 record: 34-byte formatted area (SMBIOS 2.6+)
/// followed by a four-string set, decoded by `taskmanager-smbios-tables`.
fn populated_raw(size_mb: u16, speed_mts: u16, configured_mts: u16) -> Vec<u8> {
    let mut bytes = vec![0u8; 34];
    bytes[0] = 17;
    bytes[1] = 34;
    bytes[12..14].copy_from_slice(&size_mb.to_le_bytes());
    bytes[14] = 0x0D; // SODIMM
    bytes[15] = 1; // device locator -> string 1
    bytes[18] = 0x1A; // DDR4
    bytes[21..23].copy_from_slice(&speed_mts.to_le_bytes());
    bytes[23] = 2; // manufacturer -> string 2
    bytes[24] = 3; // serial number -> string 3
    bytes[26] = 4; // part number -> string 4
    bytes[32..34].copy_from_slice(&configured_mts.to_le_bytes());
    bytes.extend_from_slice(b"ChannelA-DIMM0\0Crucial\0SER1\0CT8G4\0\0");
    bytes
}

/// An empty-socket record: type 17, size word 0 ("No Module Installed").
fn empty_slot_raw() -> Vec<u8> {
    let mut bytes = vec![0u8; 21];
    bytes[0] = 17;
    bytes[1] = 21;
    bytes
}

/// A malformed entry: wrong structure type byte.
fn malformed_raw() -> Vec<u8> {
    let mut bytes = vec![0u8; 34];
    bytes[0] = 4;
    bytes[1] = 34;
    bytes
}

/// Append a NUL-separated, double-NUL-terminated string set to `bytes`
/// (explicit per-string pushes: `\0`-run literals with digit-leading strings
/// read as octal escapes).
fn append_string_set(bytes: &mut Vec<u8>, strings: &[&str]) {
    for string in strings {
        bytes.extend_from_slice(string.as_bytes());
        bytes.push(0);
    }
    bytes.push(0);
}

/// A populated type-0 (BIOS Information) record: vendor/version/date strings.
fn bios_raw() -> Vec<u8> {
    let mut bytes = vec![0u8; 0x12];
    bytes[0] = 0;
    bytes[1] = 0x12;
    bytes[0x04] = 1;
    bytes[0x05] = 2;
    bytes[0x08] = 3;
    append_string_set(&mut bytes, &["AMI", "P1.27", "04/17/2024"]);
    bytes
}

/// A populated type-1 (System Information) record: serial + a real-shaped
/// UUID whose canonical rendering is `4c4c4544-0042-3510-8054-b7c04f4d3532`.
fn system_raw() -> Vec<u8> {
    let mut bytes = vec![0u8; 0x1B];
    bytes[0] = 1;
    bytes[1] = 0x1B;
    bytes[0x04] = 1;
    bytes[0x05] = 2;
    bytes[0x06] = 3;
    bytes[0x07] = 4;
    bytes[0x08..0x18].copy_from_slice(&[
        0x44, 0x45, 0x4C, 0x4C, 0x42, 0x00, 0x10, 0x35, 0x80, 0x54, 0xB7, 0xC0, 0x4F, 0x4D, 0x35,
        0x32,
    ]);
    bytes[0x19] = 5;
    bytes[0x1A] = 6;
    append_string_set(
        &mut bytes,
        &["LENOVO", "21JX", "v1", "PF3XYZ42", "SKU-AB", "ThinkPad"],
    );
    bytes
}

/// A populated type-2 (Base Board Information) record with an asset tag; the
/// serial string is parameterized so the ordering test can build two boards.
fn board_raw_with_serial(serial: &str) -> Vec<u8> {
    let mut bytes = vec![0u8; 0x09];
    bytes[0] = 2;
    bytes[1] = 0x09;
    bytes[0x04] = 1;
    bytes[0x05] = 2;
    bytes[0x07] = 3;
    bytes[0x08] = 4;
    let set = ["ASUSTeK", "X670E", serial, "ASSET-42"];
    append_string_set(&mut bytes, &set);
    bytes
}

/// The default board fixture: serial `"MB-SN-1"`.
fn board_raw() -> Vec<u8> {
    board_raw_with_serial("MB-SN-1")
}

/// A malformed type-1 record: declared length below the 0x08 minimum.
fn malformed_system_raw() -> Vec<u8> {
    let mut bytes = vec![0u8; 0x07];
    bytes[0] = 1;
    bytes[1] = 0x07;
    bytes
}

fn success_slots(outcome: WalkOutcome) -> (Vec<u32>, u32, u32) {
    match outcome {
        WalkOutcome::Success {
            modules,
            slots_total,
            slots_used,
            ..
        } => (
            modules.iter().map(|m| m.slot).collect(),
            slots_total,
            slots_used,
        ),
        WalkOutcome::Error(error) => {
            panic!("expected modules, got {:?}: {}", error.kind, error.detail)
        }
    }
}

/// The identity object of a success outcome.
fn success_identity(outcome: WalkOutcome) -> Option<DmiIdentityJson> {
    match outcome {
        WalkOutcome::Success { identity, .. } => identity.map(|boxed| *boxed),
        WalkOutcome::Error(error) => {
            panic!("expected modules, got {:?}: {}", error.kind, error.detail)
        }
    }
}

#[test]
fn populated_and_empty_slots_report_counts_and_fields() {
    let root = fixture_root("counts");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "17-0", &populated_raw(16384, 5600, 4800));
    write_entry(&root, "17-1", &empty_slot_raw());

    let outcome = collect_dmi_facts(&root);
    let (slots, slots_total, slots_used) = success_slots(outcome);
    assert_eq!(slots, [0], "only the populated slot becomes a module");
    assert_eq!(slots_total, 2, "the empty socket still counts as a slot");
    assert_eq!(slots_used, 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn module_carries_the_decoded_record_fields() {
    let root = fixture_root("fields");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "17-3", &populated_raw(8192, 5600, 4800));

    let WalkOutcome::Success { modules, .. } = collect_dmi_facts(&root) else {
        panic!("expected modules");
    };
    let module = &modules[0];
    assert_eq!(module.slot, 3, "slot is the 17-N numeric suffix");
    assert_eq!(module.size_mb, Some(8192));
    assert_eq!(module.speed_mts, Some(5600));
    assert_eq!(module.configured_speed_mts, Some(4800));
    assert_eq!(module.manufacturer.as_deref(), Some("Crucial"));
    assert_eq!(module.serial_number.as_deref(), Some("SER1"));
    assert_eq!(module.part_number.as_deref(), Some("CT8G4"));
    assert_eq!(module.form_factor, Some("SODIMM"));
    assert_eq!(module.memory_type, Some("DDR4"));
    assert_eq!(module.locator.as_deref(), Some("ChannelA-DIMM0"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn modules_are_sorted_by_slot_suffix() {
    let root = fixture_root("sorting");
    fs::create_dir_all(&root).unwrap();
    // Created in descending slot order; readdir order is arbitrary, so the
    // contract's numeric-suffix sort is what makes this assertion stable.
    write_entry(&root, "17-2", &populated_raw(4096, 3200, 3200));
    write_entry(&root, "17-1", &empty_slot_raw());
    write_entry(&root, "17-0", &populated_raw(16384, 5600, 4800));

    let (slots, slots_total, slots_used) = success_slots(collect_dmi_facts(&root));
    assert_eq!(slots, [0, 2]);
    assert_eq!(slots_total, 3);
    assert_eq!(slots_used, 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn malformed_record_counts_as_a_slot_but_not_a_module() {
    let root = fixture_root("malformed");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "17-9", &malformed_raw());

    let (slots, slots_total, slots_used) = success_slots(collect_dmi_facts(&root));
    assert!(slots.is_empty(), "malformed record is skipped");
    assert_eq!(slots_total, 1, "the firmware still described the slot");
    assert_eq!(slots_used, 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_type17_and_unparsable_entries_are_ignored() {
    let root = fixture_root("ignored");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "4-0", &malformed_raw());
    write_entry(&root, "17-x", &populated_raw(16384, 5600, 4800));
    write_entry(&root, "bios-0", &[]);

    let (slots, slots_total, slots_used) = success_slots(collect_dmi_facts(&root));
    assert!(slots.is_empty());
    assert_eq!(
        slots_total, 0,
        "17-x is not a slot; other types are not slots"
    );
    assert_eq!(slots_used, 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn empty_entries_root_is_an_honest_empty_success() {
    let root = fixture_root("empty_root");
    fs::create_dir_all(&root).unwrap();

    let (slots, slots_total, slots_used) = success_slots(collect_dmi_facts(&root));
    assert!(slots.is_empty());
    assert_eq!(slots_total, 0);
    assert_eq!(slots_used, 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn missing_entries_root_is_no_dmi() {
    let root = fixture_root("missing");
    match collect_dmi_facts(&root) {
        WalkOutcome::Error(error) => {
            assert_eq!(error.kind, ErrorKindJson::NoDmi);
            assert!(
                error.detail.contains("missing"),
                "detail names the missing path: {}",
                error.detail
            );
        }
        WalkOutcome::Success { .. } => panic!("missing root must not be a success"),
    }
}

#[test]
fn missing_raw_file_is_read_failed() {
    let root = fixture_root("missing_raw");
    fs::create_dir_all(root.join("17-0")).unwrap();

    match collect_dmi_facts(&root) {
        WalkOutcome::Error(error) => {
            assert_eq!(error.kind, ErrorKindJson::ReadFailed);
            assert!(
                error.detail.contains("17-0"),
                "detail names the unreadable entry: {}",
                error.detail
            );
        }
        WalkOutcome::Success { .. } => panic!("unreadable raw file must not be a success"),
    }
    let _ = fs::remove_dir_all(&root);
}

/// Restore search/reading permissions so the fixture cleanup can run.
fn restore(path: &Path) {
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
}

#[test]
fn denied_entries_root_is_permission_denied() {
    let root = fixture_root("denied_root");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "17-0", &populated_raw(16384, 5600, 4800));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();

    let outcome = collect_dmi_facts(&root);
    restore(&root);
    let WalkOutcome::Error(error) = &outcome else {
        // Root bypasses DAC modes; the denial classification is observable
        // only on unprivileged runners.
        eprintln!("SKIP: running privileged; the 0o000 entries dir still read");
        let _ = fs::remove_dir_all(&root);
        return;
    };
    assert_eq!(error.kind, ErrorKindJson::PermissionDenied);
    assert!(
        error.detail.contains("denied_root"),
        "detail names the denied path: {}",
        error.detail
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn denied_raw_file_is_permission_denied() {
    let root = fixture_root("denied_raw");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "17-0", &populated_raw(16384, 5600, 4800));
    let raw = root.join("17-0").join("raw");
    fs::set_permissions(&raw, fs::Permissions::from_mode(0o000)).unwrap();

    let outcome = collect_dmi_facts(&root);
    restore(&raw);
    let WalkOutcome::Error(error) = &outcome else {
        eprintln!("SKIP: running privileged; the 0o000 raw file still read");
        let _ = fs::remove_dir_all(&root);
        return;
    };
    assert_eq!(error.kind, ErrorKindJson::PermissionDenied);
    assert!(
        error.detail.contains("raw"),
        "detail names the denied file: {}",
        error.detail
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn identity_entries_decode_into_the_identity_object() {
    let root = fixture_root("identity");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "0-0", &bios_raw());
    write_entry(&root, "1-0", &system_raw());
    write_entry(&root, "2-0", &board_raw());
    write_entry(&root, "17-0", &populated_raw(16384, 5600, 4800));

    let identity = success_identity(collect_dmi_facts(&root)).expect("identity present");
    assert_eq!(identity.bios_vendor.as_deref(), Some("AMI"));
    assert_eq!(identity.bios_version.as_deref(), Some("P1.27"));
    assert_eq!(identity.bios_date.as_deref(), Some("04/17/2024"));
    assert_eq!(identity.board_manufacturer.as_deref(), Some("ASUSTeK"));
    assert_eq!(identity.board_product.as_deref(), Some("X670E"));
    assert_eq!(identity.board_serial.as_deref(), Some("MB-SN-1"));
    assert_eq!(identity.board_asset_tag.as_deref(), Some("ASSET-42"));
    assert_eq!(identity.system_manufacturer.as_deref(), Some("LENOVO"));
    assert_eq!(identity.system_product.as_deref(), Some("21JX"));
    assert_eq!(identity.system_serial.as_deref(), Some("PF3XYZ42"));
    assert_eq!(
        identity.system_uuid.as_deref(),
        Some("4c4c4544-0042-3510-8054-b7c04f4d3532")
    );
    assert_eq!(identity.system_sku.as_deref(), Some("SKU-AB"));
    assert_eq!(identity.system_family.as_deref(), Some("ThinkPad"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn missing_identity_entries_leave_the_object_null_not_fabricated() {
    let root = fixture_root("no_identity");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "17-0", &populated_raw(16384, 5600, 4800));

    let outcome = collect_dmi_facts(&root);
    assert!(
        success_identity(outcome).is_none(),
        "no 0/1/2 entries means an honest null identity"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_partial_identity_set_decodes_only_the_present_tables() {
    let root = fixture_root("partial_identity");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "1-0", &system_raw());

    let identity = success_identity(collect_dmi_facts(&root)).expect("identity present");
    assert_eq!(identity.system_serial.as_deref(), Some("PF3XYZ42"));
    assert_eq!(identity.bios_vendor, None);
    assert_eq!(identity.board_asset_tag, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_lowest_numbered_identity_entry_wins() {
    let root = fixture_root("identity_order");
    fs::create_dir_all(&root).unwrap();
    // A second board instance with a different serial; readdir order is
    // arbitrary, so the contract's lowest-suffix rule is what makes this
    // assertion stable.
    write_entry(&root, "2-1", &board_raw_with_serial("OTHER-S"));
    write_entry(&root, "2-0", &board_raw());

    let identity = success_identity(collect_dmi_facts(&root)).expect("identity present");
    assert_eq!(identity.board_serial.as_deref(), Some("MB-SN-1"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_malformed_identity_record_keeps_its_fields_null_but_the_object_honest() {
    let root = fixture_root("malformed_identity");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "0-0", &bios_raw());
    write_entry(&root, "1-0", &malformed_system_raw());

    let identity = success_identity(collect_dmi_facts(&root)).expect("identity present");
    assert_eq!(identity.bios_vendor.as_deref(), Some("AMI"));
    assert_eq!(identity.system_serial, None);
    assert_eq!(identity.system_uuid, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unparsable_identity_entry_names_are_ignored() {
    let root = fixture_root("identity_names");
    fs::create_dir_all(&root).unwrap();
    write_entry(&root, "1-x", &system_raw());
    write_entry(&root, "10-0", &system_raw());
    write_entry(&root, "17-0", &populated_raw(16384, 5600, 4800));

    assert!(success_identity(collect_dmi_facts(&root)).is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_missing_identity_raw_file_is_read_failed() {
    let root = fixture_root("identity_raw_missing");
    fs::create_dir_all(root.join("1-0")).unwrap();
    write_entry(&root, "17-0", &populated_raw(16384, 5600, 4800));

    match collect_dmi_facts(&root) {
        WalkOutcome::Error(error) => {
            assert_eq!(error.kind, ErrorKindJson::ReadFailed);
            assert!(
                error.detail.contains("1-0"),
                "detail names the unreadable identity entry: {}",
                error.detail
            );
        }
        WalkOutcome::Success { .. } => panic!("an unreadable identity entry must not be a success"),
    }
    let _ = fs::remove_dir_all(&root);
}

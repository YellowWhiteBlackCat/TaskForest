use super::*;

#[test]
fn pci_ids_resolution_keeps_vendor_scope_and_extracts_marketing_sku() {
    let ids = "\
8086  Intel Corporation
\tB080  Panther Lake [Arc B390]
\tB081  Panther Lake [Arc B370]
\t\t0000  Reference subsystem
10DE  NVIDIA Corporation
\tB080  Unrelated vendor device
";

    assert_eq!(
        pci_ids_device_name(ids, 0x8086, 0xB080).as_deref(),
        Some("Panther Lake [Arc B390]")
    );
    assert_eq!(
        marketing_name_from_pci_label("Panther Lake [Arc B390]").as_deref(),
        Some("Arc B390")
    );
    assert_eq!(
        pci_ids_device_name(ids, 0x10DE, 0xB080).as_deref(),
        Some("Unrelated vendor device")
    );
    assert_eq!(pci_ids_device_name(ids, 0x8086, 0x1234), None);
}

#[test]
fn pci_id_parsing_rejects_overflow_and_trailing_syntax() {
    assert_eq!(parse_pci_id("0x8086\n"), Some(0x8086));
    assert_eq!(parse_pci_id(" 8086 "), Some(0x8086));
    assert_eq!(parse_pci_id("808"), None);
    assert_eq!(parse_pci_id("0x80860"), None);
    assert_eq!(parse_pci_id("zzzz"), None);
}

/// Historical nouveau/radeon-era boards (NVIDIA GK106M/GM204/GP107, AMD
/// Tahiti/Ellesmere/R300, bracket-less APU codenames) resolve through the same
/// bounded pci.ids rules as current boards: vendor-scoped device lookup, the
/// bracketed marketing SKU preferred, subsystem lines ignored, and a
/// bracket-less codename kept whole instead of being eaten by the cleaning
/// step. Labels are the real hwdata entries for those vendor:device pairs.
#[test]
fn nouveau_and_radeon_era_devices_resolve_to_marketing_names() {
    let ids = "\
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t1305  Kaveri
\t4164  R300 [Radeon 9500 PRO] (Secondary)
\t6798  Tahiti XT [Radeon HD 7970/8970 OEM / R9 280X]
\t\t174B  E188  Tahiti XT subsession board
\t67df  Ellesmere [Radeon RX 470/480/570/570X/580/580X/590]
10DE  NVIDIA Corporation
\t11e0  GK106M [GeForce GTX 770M]
\t13c2  GM204 [GeForce GTX 970]
\t1c82  GP107 [GeForce GTX 1050 Ti]
";

    let resolved = |vendor: u16, device: u16| {
        pci_ids_device_name(ids, vendor, device).map(|label| {
            let marketing = marketing_name_from_pci_label(&label);
            (label, marketing)
        })
    };

    // radeon-driver AMD families.
    assert_eq!(
        resolved(0x1002, 0x6798),
        Some((
            "Tahiti XT [Radeon HD 7970/8970 OEM / R9 280X]".to_string(),
            Some("Radeon HD 7970/8970 OEM / R9 280X".to_string()),
        ))
    );
    assert_eq!(
        resolved(0x1002, 0x67DF),
        Some((
            "Ellesmere [Radeon RX 470/480/570/570X/580/580X/590]".to_string(),
            Some("Radeon RX 470/480/570/570X/580/580X/590".to_string()),
        ))
    );
    // The trailing `(Secondary)` annotation stays outside the marketing SKU.
    assert_eq!(
        resolved(0x1002, 0x4164),
        Some((
            "R300 [Radeon 9500 PRO] (Secondary)".to_string(),
            Some("Radeon 9500 PRO".to_string()),
        ))
    );
    // A bracket-less APU codename is the whole honest name, never truncated.
    assert_eq!(
        resolved(0x1002, 0x1305),
        Some(("Kaveri".to_string(), Some("Kaveri".to_string())))
    );

    // nouveau-driver NVIDIA families.
    assert_eq!(
        resolved(0x10DE, 0x11E0),
        Some((
            "GK106M [GeForce GTX 770M]".to_string(),
            Some("GeForce GTX 770M".to_string()),
        ))
    );
    assert_eq!(
        resolved(0x10DE, 0x13C2),
        Some((
            "GM204 [GeForce GTX 970]".to_string(),
            Some("GeForce GTX 970".to_string()),
        ))
    );
    assert_eq!(
        resolved(0x10DE, 0x1C82),
        Some((
            "GP107 [GeForce GTX 1050 Ti]".to_string(),
            Some("GeForce GTX 1050 Ti".to_string()),
        ))
    );

    // A device listed only under another vendor stays unresolved, and the
    // subsystem line never substitutes for the device-level entry.
    assert_eq!(pci_ids_device_name(ids, 0x1002, 0x11E0), None);
    assert_eq!(pci_ids_device_name(ids, 0x10DE, 0x174B), None);
}

#[test]
fn pci_ids_database_loading_reads_the_first_candidate_only() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_pci_ids_load_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("fixture root should be created");
    std::fs::write(root.join("first.ids"), "8086  Intel Corporation\n").expect("first database");
    std::fs::write(root.join("second.ids"), "1002  AMD\n").expect("second database");

    let first = read_pci_ids_text(&[root.join("first.ids"), root.join("second.ids")])
        .expect("first candidate should load");
    assert!(first.contains("Intel Corporation"));
    // No installed database is an honest absence, never a fabricated one.
    assert_eq!(read_pci_ids_text(&[root.join("missing.ids")]), None);
    assert_eq!(read_pci_ids_text(&[]), None);

    std::fs::remove_dir_all(root).ok();
}

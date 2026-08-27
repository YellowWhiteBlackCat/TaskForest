use std::fs;

use super::*;

fn synthetic_edid() -> Vec<u8> {
    let mut edid = vec![0_u8; 128];
    edid[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    let manufacturer = (4_u16 << 10) | (5 << 5) | 12; // DEL
    edid[8..10].copy_from_slice(&manufacturer.to_be_bytes());
    edid[10..12].copy_from_slice(&0x1234_u16.to_le_bytes());
    edid[12..16].copy_from_slice(&0x0102_0304_u32.to_le_bytes());
    edid[21] = 60; // 600 mm
    edid[22] = 34; // 340 mm

    // 148.5 MHz, 1920x1080, 280/45 blanking => 60 Hz.
    let dtd = &mut edid[54..72];
    dtd[..2].copy_from_slice(&14_850_u16.to_le_bytes());
    dtd[2] = (1920 & 0xff) as u8;
    dtd[3] = (280 & 0xff) as u8;
    dtd[4] = (((1920 >> 8) & 0x0f) << 4 | ((280 >> 8) & 0x0f)) as u8;
    dtd[5] = (1080 & 0xff) as u8;
    dtd[6] = (45 & 0xff) as u8;
    dtd[7] = (((1080 >> 8) & 0x0f) << 4) as u8;

    let name = &mut edid[90..108];
    name[3] = 0xfc;
    name[5..15].copy_from_slice(b"TaskPanel\n");
    let serial = &mut edid[108..126];
    serial[3] = 0xff;
    serial[5..11].copy_from_slice(b"SN-42\n");

    let checksum = edid[..127]
        .iter()
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
    edid[127] = 0_u8.wrapping_sub(checksum);
    edid
}

#[test]
fn edid_parser_keeps_identity_dimensions_and_preferred_timing_typed() {
    let display = parse_edid("DP-1", &synthetic_edid()).expect("synthetic EDID is valid");

    assert_eq!(display.connector, "DP-1");
    assert_eq!(display.manufacturer.as_deref(), Some("DEL"));
    assert_eq!(display.model.as_deref(), Some("TaskPanel"));
    assert_eq!(display.serial.as_deref(), Some("SN-42"));
    assert_eq!(display.width_mm, Some(600));
    assert_eq!(display.height_mm, Some(340));
    assert_eq!(display.width_px, Some(1920));
    assert_eq!(display.height_px, Some(1080));
    assert_eq!(display.refresh_hz, Some(60.0));
    assert_eq!(display.hdr_supported, None);
}

#[test]
fn edid_cta_hdr_static_metadata_is_reported_as_capability_only() {
    let mut edid = synthetic_edid();
    edid[126] = 1;
    edid[127] = 0_u8.wrapping_sub(
        edid[..127]
            .iter()
            .fold(0_u8, |sum, byte| sum.wrapping_add(*byte)),
    );
    edid.resize(256, 0);
    let extension = &mut edid[128..256];
    extension[0] = 0x02; // CTA-861
    extension[2] = 7; // data block collection ends at byte 7
    extension[4] = 0xe2; // extended data block, two bytes
    extension[5] = 0x06; // HDR Static Metadata Data Block
    extension[6] = 0;
    extension[127] = 0_u8.wrapping_sub(
        extension[..127]
            .iter()
            .fold(0_u8, |sum, byte| sum.wrapping_add(*byte)),
    );

    let display = parse_edid("HDMI-A-1", &edid).expect("EDID with CTA extension is valid");
    assert_eq!(display.hdr_supported, Some(true));
}

#[test]
fn invalid_edid_and_mode_lines_fail_closed() {
    assert_eq!(parse_edid("HDMI-A-1", &[0_u8; 128]), None);
    assert_eq!(parse_mode("1920x1080 (preferred)"), Some((1920, 1080)));
    assert_eq!(parse_mode("0x1080"), None);
    assert_eq!(parse_mode("not-a-mode"), None);
}

#[test]
fn oversized_edid_is_rejected_before_allocation_can_escape_the_bound() {
    let path = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-oversized-edid-{}", std::process::id()));
    fs::write(&path, vec![0_u8; MAX_EDID_BYTES + 1]).expect("oversized EDID fixture");
    assert_eq!(
        read_bounded_bytes(&path, MAX_EDID_BYTES)
            .expect_err("oversized EDID must fail closed")
            .kind(),
        std::io::ErrorKind::InvalidData
    );
    let _ = fs::remove_file(path);
}

#[test]
fn display_collection_reports_connected_identity_without_fabricating_edid_fields() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-display-fixture-{}",
        std::process::id()
    ));
    let connector = root.join("card0-DP-1");
    let disconnected = root.join("card0-HDMI-A-1");
    fs::create_dir_all(&connector).expect("connector fixture should be created");
    fs::create_dir_all(&disconnected).expect("disconnected fixture should be created");
    fs::write(connector.join("status"), "connected\n").expect("status should be written");
    fs::write(connector.join("modes"), "1920x1080\n").expect("mode should be written");
    fs::write(connector.join("edid"), synthetic_edid()).expect("EDID should be written");
    fs::write(disconnected.join("status"), "disconnected\n")
        .expect("disconnected status should be written");

    let (displays, outcome) = collect_displays(&root);
    assert_eq!(outcome, SourceOutcome::Available);
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0].connector, "DP-1");
    assert_eq!(displays[0].width_px, Some(1920));
    assert_eq!(displays[0].height_px, Some(1080));
    assert_eq!(displays[0].refresh_hz, Some(60.0));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_drm_root_is_typed_unsupported() {
    let (displays, outcome) = collect_displays(
        &crate::test_support::repo_temp_dir().join("taskmanager-missing-display-root"),
    );
    assert!(displays.is_empty());
    assert_eq!(
        outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
}

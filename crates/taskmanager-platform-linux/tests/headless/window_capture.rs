use std::fs;

use super::*;

// A valid 1x1 RGBA PNG. The backend tests only inspect the signature and IHDR
// geometry; the live compositor path owns full pixel validation.
const ONE_PIXEL_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 0,
    5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[test]
fn png_inspection_accepts_a_real_header_and_dimensions() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp")
        .join(format!(
            "taskforest-window-capture-test-{}.png",
            std::process::id()
        ));
    fs::write(&path, ONE_PIXEL_PNG).expect("write fixture");
    assert_eq!(inspect_png(&path).expect("PNG receipt"), (1, 1));
    fs::remove_file(path).expect("remove fixture");
}

#[test]
fn png_inspection_rejects_non_png_output() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp")
        .join(format!(
            "taskforest-window-capture-invalid-{}.png",
            std::process::id()
        ));
    fs::write(&path, [0_u8; 64]).expect("write fixture");
    let error = inspect_png(&path).expect_err("invalid PNG");
    assert_eq!(error.kind(), WindowCaptureFailureKind::InvalidImage);
    fs::remove_file(path).expect("remove fixture");
}

#[test]
fn png_inspection_rejects_zero_dimensions_and_invalid_ihdr_length() {
    for (label, offset, value) in [
        ("zero-width", 16, [0, 0, 0, 0]),
        ("bad-ihdr-length", 8, [0, 0, 0, 12]),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.tmp")
            .join(format!(
                "taskforest-window-capture-{label}-{}.png",
                std::process::id()
            ));
        let mut bytes = ONE_PIXEL_PNG.to_vec();
        bytes[offset..offset + 4].copy_from_slice(&value);
        fs::write(&path, bytes).expect("write malformed PNG");
        let error = inspect_png(&path).expect_err("malformed PNG");
        assert_eq!(error.kind(), WindowCaptureFailureKind::InvalidImage);
        fs::remove_file(path).expect("remove malformed PNG");
    }
}

#[test]
fn png_inspection_rejects_dimensions_above_the_native_bound() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp")
        .join(format!(
            "taskforest-window-capture-oversized-{}.png",
            std::process::id()
        ));
    let mut bytes = ONE_PIXEL_PNG.to_vec();
    bytes[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    fs::write(&path, bytes).expect("write oversized PNG");
    let error = inspect_png(&path).expect_err("oversized PNG");
    assert_eq!(error.kind(), WindowCaptureFailureKind::InvalidImage);
    fs::remove_file(path).expect("remove oversized PNG");
}

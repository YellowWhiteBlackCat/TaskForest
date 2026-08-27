#[cfg(windows)]
use super::*;

/// Real Win32 icon extraction only exists on Windows; the cross-platform
/// build must not carry a vacuous always-green copy of this test.
#[cfg(windows)]
#[test]
fn extract_explorer_icon() {
    let res = extract_process_icon_bmp("C:\\Windows\\explorer.exe");
    if let Ok(bytes) = res {
        assert!(bytes.starts_with(b"BM"));
        assert!(bytes.len() > 54);
        eprintln!("EXTRACTED EXPLORER ICON BMP BYTES: {}", bytes.len());
    }
}

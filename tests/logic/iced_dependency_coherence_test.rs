//! Lock the Iced renderer's Windows dependency family (ADR-028).
//!
//! The workspace also contains an optional GPUI screen-capture dependency
//! whose older `sysinfo` edge keeps `windows` 0.57 available. Iced's wgpu-hal
//! 27 DX12 path and gpu-allocator 0.27 must instead share `windows` 0.58; a
//! mixed edge creates distinct D3D12 interface types and fails compilation.

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn lock_package<'a>(lock: &'a str, name: &str) -> &'a str {
    lock.split("[[package]]")
        .find(|block| {
            block
                .lines()
                .any(|line| line.trim() == format!("name = \"{name}\""))
        })
        .unwrap_or_else(|| panic!("Cargo.lock is missing package {name}"))
}

#[test]
fn iced_wgpu_hal_and_allocator_share_windows_058() {
    let root = repository();
    let manifest = fs::read_to_string(root.join("crates/taskmanager-iced/Cargo.toml"))
        .expect("Iced manifest should be readable");
    let lock = fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock should be readable");

    assert!(
        manifest.contains("windows = { version = \"0.58.0\""),
        "Iced must keep its Windows renderer family explicit"
    );

    let iced = lock_package(&lock, "taskmanager-iced");
    assert!(
        iced.contains("\"windows 0.58.0\""),
        "taskmanager-iced must resolve its renderer pin to windows 0.58"
    );

    let allocator = lock_package(&lock, "gpu-allocator");
    assert!(
        allocator.contains("\"windows 0.58.0\""),
        "gpu-allocator's D3D12 edge must use the wgpu-hal windows family"
    );
    assert!(
        !allocator.contains("\"windows 0.57.0\""),
        "gpu-allocator must not reuse the unrelated GPUI screen-capture windows edge"
    );
}

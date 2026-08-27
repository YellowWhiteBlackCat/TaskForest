//! Scratch-directory helpers used only by GPUI unit tests.

#![cfg(test)]

use std::path::PathBuf;

/// Unique per-process scratch root under the repository `.tmp/` (never the
/// system temp dir). Unit tests create their own subdirectories under it and
/// must remove them; the root itself lives in gitignored `.tmp/`.
pub(crate) fn repo_temp_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = if manifest.parent().and_then(std::path::Path::file_name)
            == Some(std::ffi::OsStr::new("crates"))
        {
            manifest
                .parent()
                .and_then(std::path::Path::parent)
                .unwrap_or(manifest)
        } else {
            manifest
        };
        let root = workspace.join(".tmp").join("test-scratch");
        std::fs::create_dir_all(&root).expect("create repository test scratch");
        let unique = root.join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&unique).expect("create unique test scratch");
        unique
    })
    .to_path_buf()
}

pub(crate) fn scratch_dir(label: &str) -> PathBuf {
    let root = repo_temp_dir().join(format!(
        "taskmanager-gpui-test-{}-{label}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

pub(crate) fn remove_scratch(root: &std::path::Path) {
    let _ = std::fs::remove_dir_all(root);
}

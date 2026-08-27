//! Shared test support for the Iced frontend: an isolated, collision-free
//! temp-dir factory (unique per label, process, and call sequence).

/// Unique per-process scratch root under the repository `.tmp/` (never the
/// system temp dir). Unit tests create their own subdirectories under it and
/// must remove them; the root itself lives in gitignored `.tmp/`.
pub(crate) fn repo_temp_dir() -> std::path::PathBuf {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
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

pub(crate) fn temp_dir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
    let path = repo_temp_dir().join(format!(
        "taskmanager-iced-{label}-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("create isolated Iced test directory");
    path
}

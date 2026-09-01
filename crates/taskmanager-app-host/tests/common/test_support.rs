//! Shared test-only scratch support under the repository `.tmp/` directory.

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

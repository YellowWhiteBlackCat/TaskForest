//! Test-side page surface: the full route list the page-signature
//! reservation gate walks. The rendered strip shows only
//! [`crate::app::NAV_TABS`]; this list keeps every route (including
//! Alerts/Settings) under the shared-order law.

use crate::app::Page;

impl Page {
    /// The full route surface, in the shared order.
    pub(crate) const ALL: &'static [Page] = &[
        Page::Processes,
        Page::Performance,
        Page::Services,
        Page::System,
        Page::Startup,
        Page::Sessions,
        Page::Alerts,
        Page::Settings,
        Page::AppHistory,
    ];
}

/// Unique per-process scratch root under the repository `.tmp/` (never the
/// system temp dir — the headless side-effect guard rejects OS-visible test
/// writes). Callers create their own subdirectories under it and must remove
/// them; the root itself lives in gitignored `.tmp/`.
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
    .clone()
}

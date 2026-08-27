//! Shared test-only support for the TUI render tests.

/// Serializes the one language-flipping i18n test against the render tests.
///
/// `t()` resolves against a process-global active language, so a test that
/// calls `set_language(Zh)` would otherwise leak translated text into a
/// concurrently-rendering English assertion in the same test binary. Render
/// test helpers (`frame_text`) acquire this guard for the duration of their
/// draw; the language test holds it across its En/Zh cycle. `Mutex` is not
/// reentrant, so a holder must not call a guarded helper inline.
pub(crate) static LANG_TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

/// Inject an isolated application-owned coordinator client. Production TUI
/// code never receives or discovers a filesystem path; only this fixture
/// composition seam opens the test store.
pub(crate) fn set_config_store_client(
    app: &mut crate::TuiApp,
    path: impl Into<std::path::PathBuf>,
) {
    let coordinator = taskmanager_application::ConfigCoordinator::start_path(path.into())
        .expect("start isolated TUI configuration coordinator");
    app.config_client = Some(coordinator.client());
    app.applied_config_revision = None;
}

/// Inject a fixture coordinator and apply its initial publication.
pub(crate) fn install_config_store(app: &mut crate::TuiApp, path: impl Into<std::path::PathBuf>) {
    set_config_store_client(app, path);
    app.load_config();
}

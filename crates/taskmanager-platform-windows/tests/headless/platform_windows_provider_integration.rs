use super::*;
use std::io;
use std::sync::Mutex;

/// Mock spawner that records the select argument and returns a chosen
/// outcome. Injected instead of the real `Command` so the provider's mapping
/// logic is asserted on every host without spawning a real File Explorer
/// window (the previous unit test did spawn `explorer.exe` fire-and-forget
/// on real Windows; the Linux CI gate only passed because `explorer` was
/// absent there).
struct RecordingSpawner {
    args: Mutex<Vec<String>>,
    error_kind: Mutex<Option<io::ErrorKind>>,
}

impl RecordingSpawner {
    fn new() -> Self {
        Self {
            args: Mutex::new(Vec::new()),
            error_kind: Mutex::new(None),
        }
    }

    fn failing(kind: io::ErrorKind) -> Self {
        Self {
            args: Mutex::new(Vec::new()),
            error_kind: Mutex::new(Some(kind)),
        }
    }
}

impl ExplorerSpawn for RecordingSpawner {
    fn reveal(&self, select_arg: &str) -> io::Result<()> {
        self.args
            .lock()
            .expect("recorder args")
            .push(select_arg.to_owned());
        match *self.error_kind.lock().expect("recorder error kind") {
            Some(kind) => Err(io::Error::new(kind, "boom")),
            None => Ok(()),
        }
    }
}

#[test]
fn reveal_without_cached_path_is_temporarily_unavailable() {
    let provider = WinResourceRevealProvider::new();
    // No cached executable -> nothing to reveal -> honest
    // TemporarilyUnavailable (NOT Unsupported: stays in the pending set).
    assert_eq!(
        provider.reveal_with(None),
        Err(ProviderFailure::TemporarilyUnavailable)
    );
}

#[test]
fn reveal_with_cached_path_dispatches_the_explorer_select_spawn() {
    let recorder = Arc::new(RecordingSpawner::new());
    let provider = WinResourceRevealProvider {
        spawner: recorder.clone(),
    };
    let cached = std::path::Path::new(r"C:\Program Files\TaskForest\taskforest-g.exe");
    let result = provider.reveal_with(Some(cached));
    assert_eq!(result, Ok(()));
    assert_eq!(
        recorder.args.lock().expect("recorder args").as_slice(),
        &[format!("/select,{}", cached.display())],
        "the provider must dispatch explorer with the exact /select argument"
    );
}

#[test]
fn reveal_maps_spawn_not_found_to_missing_dependency() {
    // `explorer` absent (the Linux CI gate shape): NotFound -> the honest
    // MissingDependency mapping, never a fabricated Unsupported.
    let recorder = Arc::new(RecordingSpawner::failing(io::ErrorKind::NotFound));
    let provider = WinResourceRevealProvider {
        spawner: recorder.clone(),
    };
    let result = provider.reveal_with(Some(std::path::Path::new(r"C:\nonexistent")));
    assert_eq!(result, Err(ProviderFailure::MissingDependency));
}

#[test]
fn reveal_maps_other_spawn_failures_to_permission_denied() {
    let recorder = Arc::new(RecordingSpawner::failing(io::ErrorKind::PermissionDenied));
    let provider = WinResourceRevealProvider {
        spawner: recorder.clone(),
    };
    let result = provider.reveal_with(Some(std::path::Path::new(r"C:\nonexistent")));
    assert_eq!(result, Err(ProviderFailure::PermissionDenied));
}

#[cfg(windows)]
#[test]
fn reveal_rejects_a_reused_pid_before_spawning_explorer() {
    let recorder = Arc::new(RecordingSpawner::new());
    let mut provider = WinResourceRevealProvider {
        spawner: recorder.clone(),
    };
    let wrong =
        FrozenProcessIdentity::from_authoritative_parts(std::process::id(), "test", u64::MAX, 1)
            .expect("valid wrong-token identity");
    let before = recorder.args.lock().expect("recorder args").len() as u64;
    let result = provider.reveal_process(&wrong, Some(std::path::Path::new(r"C:\target.exe")));
    let after = recorder.args.lock().expect("recorder args").len() as u64;
    taskmanager_platform_conformance::assert_identity_change_is_side_effect_free(
        &result, before, after,
    )
    .expect("reveal must reject a replacement before spawning explorer");
}

#[test]
fn map_high_contrast_translates_dword_correctly() {
    // DWORD 1 => high contrast on; any other present value => off; a
    // missing value (registry key/value absent) => no reading.
    assert_eq!(map_high_contrast(Some(1)), Some(true));
    assert_eq!(map_high_contrast(Some(0)), Some(false));
    assert_eq!(map_high_contrast(Some(2)), Some(false));
    assert_eq!(map_high_contrast(None), None);
}

#[test]
fn pending_setup_complete_with_typed_unsupported() {
    // Every SetupScriptAction must resolve to the same honest Unsupported
    // outcome — enumerated from the full action set, never a sample.
    for action in [
        SetupScriptAction::Observe,
        SetupScriptAction::View,
        SetupScriptAction::Run,
        SetupScriptAction::Revert,
        SetupScriptAction::Restart,
    ] {
        let mut setup = PendingSetupScriptProvider;
        assert_eq!(
            setup.perform(action),
            Err(ProviderFailure::Unsupported),
            "{action:?} must complete with a typed Unsupported outcome"
        );
    }
}

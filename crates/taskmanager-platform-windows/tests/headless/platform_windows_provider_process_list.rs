use super::*;

#[test]
fn unknown_process_status_has_a_safe_display_fallback() {
    assert_eq!(process_status_label(sysinfo::ProcessStatus::Run), "Running");
}

#[test]
fn pid_reuse_drops_the_previous_disk_baseline() {
    let mut rates = WinProcessDiskRateState::default();
    let first = rates.observe(Some(10), 4_096, 8_192, 100);
    assert_eq!(
        first.0,
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
    );
    let current = rates.observe(Some(10), 6_144, 10_240, 2_100);
    assert_eq!(current.0, ScalarObservation::available(1_024, 2_100));
    let reused = rates.observe(Some(11), 1, 1, 3_100);
    assert_eq!(
        reused.0,
        ScalarObservation::unavailable(FailureKind::IdentityChanged)
    );
    let unknown = rates.observe(None, 2, 2, 4_100);
    assert_eq!(
        unknown.0,
        ScalarObservation::unavailable(FailureKind::IdentityChanged)
    );
}

#[test]
fn deferred_handle_count_retains_only_the_same_native_identity() {
    let previous = Some((10, ScalarObservation::available(42, 100)));
    let retained = retain_fd_count(
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        Some(10),
        previous,
    );
    assert_eq!(retained.last_known_value(), Some(&42));
    assert!(retained.current_value().is_none());
    assert_eq!(
        retain_fd_count(
            ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            Some(11),
            previous,
        )
        .last_known_value(),
        None
    );
}

#[test]
fn windows_process_list_populates_threads_and_users() {
    let mut provider = WinProcessListProvider::new();
    let snapshot = provider
        .refresh(1000)
        .expect("process refresh should succeed");
    assert!(!snapshot.items.is_empty());
    let current_pid = std::process::id();
    if let Some(item) = snapshot.items.iter().find(|p| p.pid == current_pid) {
        let current_user = item.current_user();
        eprintln!(
            "CURRENT PROCESS IN LIST: pid={}, name={}, threads={}, user={}",
            item.pid,
            item.name,
            item.current_threads().unwrap_or_default(),
            current_user.as_deref().unwrap_or("")
        );
        // Thread counts come from the audited ToolHelp boundary: real on
        // Windows, typed `Unsupported` (never a fabricated 1) on the
        // cross-target build where that boundary has no implementation.
        #[cfg(windows)]
        assert!(item.current_threads().is_some_and(|threads| threads > 0));
        #[cfg(not(windows))]
        assert_eq!(
            item.scalar_observations().threads,
            ScalarObservation::unavailable(FailureKind::Unsupported)
        );
        // Owner and icon facts come from the native token/SID lookup and the
        // Win32 icon extraction, which only exist behind the Windows API
        // boundary. On the cross-target build both queries return their
        // typed `Unsupported`, so the row keeps an empty legacy owner with
        // the typed PermissionDenied metadata failure and no icon asset —
        // never a guessed user or a blank placeholder bitmap.
        #[cfg(windows)]
        assert!(current_user.is_some());
        #[cfg(not(windows))]
        {
            assert!(current_user.is_none());
            assert_eq!(
                item.metadata_observations().owner,
                ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PermissionDenied)
            );
        }
    }

    let has_app_id = snapshot
        .items
        .iter()
        .any(|p| p.current_application_identity().is_some());
    assert!(
        has_app_id,
        "At least one process must have application_identity"
    );

    let has_icons = snapshot.items.iter().any(|p| {
        p.application_identity_observation()
            .current_value()
            .and_then(|id| id.icon_asset.as_ref())
            .is_some()
    });
    eprintln!(
        "PROCESSES POPULATED WITH APP IDENTITY: has_app_id={}, has_icons={}",
        has_app_id, has_icons
    );
    #[cfg(windows)]
    assert!(
        has_icons,
        "At least one process must have an extracted icon asset"
    );
    #[cfg(not(windows))]
    assert!(
        !has_icons,
        "without the Win32 icon boundary no icon asset may be fabricated"
    );
}

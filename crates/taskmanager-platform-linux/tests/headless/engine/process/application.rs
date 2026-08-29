use std::path::PathBuf;

use taskmanager_core::{ProcessMetadataAvailability, ProcessScalarObservations, ScalarObservation};

use super::super::ProcessManager;
use super::*;
use std::fs;

fn identity(id: &str, name: &str, icon: Option<&str>) -> ProcessApplicationIdentity {
    ProcessApplicationIdentity::new(id, name, icon.map(str::to_owned))
        .expect("identity fixture must be valid")
}

fn catalog_entry(
    id: &str,
    name: &str,
    executable: &str,
    args: &[&str],
    icon: Option<&str>,
) -> CatalogEntry {
    CatalogEntry {
        identity: identity(id, name, icon),
        executable: executable_selector(executable, None).expect("executable fixture"),
        exec_args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

fn snap_catalog_entry(
    id: &str,
    name: &str,
    executable: &str,
    package: &str,
    args: &[&str],
) -> CatalogEntry {
    CatalogEntry {
        identity: identity(id, name, Some(package)),
        executable: executable_selector(executable, Some(package)).expect("snap fixture"),
        exec_args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

fn available_executable(path: &str) -> ProcessMetadataObservation<PathBuf> {
    ProcessMetadataObservation::available(PathBuf::from(path), 10)
}

#[test]
fn desktop_entry_parser_keeps_stable_id_and_icon_token() {
    let parsed = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=Example Editor\nExec=/usr/bin/example-editor %U\nIcon=example-editor\n",
            "org.example.Editor.desktop",
        )
        .expect("valid desktop entry should resolve");
    assert_eq!(parsed.executable, "/usr/bin/example-editor");
    assert_eq!(parsed.exec_args, vec!["%U"]);
    assert_eq!(parsed.identity.launcher_id, "org.example.Editor");
    assert_eq!(parsed.identity.display_name, "Example Editor");
    assert_eq!(
        parsed.identity.icon_token.as_deref(),
        Some("example-editor")
    );
}

#[test]
fn desktop_exec_tokenizer_preserves_quoted_pwa_arguments() {
    let parsed = parse_desktop_entry(
            "[Desktop Entry]\nName=Mail PWA\nExec=/usr/bin/google-chrome --profile-directory=\"Default Profile\" --app-id=abc123 %U\n",
            "chrome-mail.desktop",
        )
        .expect("PWA desktop entry should resolve");
    assert_eq!(parsed.executable, "/usr/bin/google-chrome");
    assert_eq!(
        parsed.exec_args,
        vec![
            "--profile-directory=Default Profile",
            "--app-id=abc123",
            "%U"
        ]
    );
}

#[test]
fn snap_and_appimage_execs_keep_a_stable_matching_selector() {
    let snap = parse_desktop_entry(
        "[Desktop Entry]\nType=Application\nName=Firefox Snap\nExec=/usr/bin/snap run firefox\n",
        "snap.firefox_firefox.desktop",
    )
    .expect("snap entry should resolve");
    assert_eq!(snap.executable, "firefox");
    assert!(snap.exec_args.is_empty());
    assert_eq!(snap.snap_package.as_deref(), Some("firefox"));

    let appimage = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=Portable Editor\nExec=/opt/PortableEditor/PortableEditor.AppImage %F\n",
            "portable-editor.desktop",
        )
        .expect("AppImage entry should resolve");
    assert_eq!(
        appimage.executable,
        "/opt/PortableEditor/PortableEditor.AppImage"
    );
    assert_eq!(appimage.exec_args, vec!["%F"]);
}

#[test]
fn snap_and_appimage_process_paths_resolve_without_cross_merging_same_names() {
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: vec![
            snap_catalog_entry("snap.firefox", "Firefox Snap", "firefox", "firefox", &[]),
            catalog_entry(
                "portable-a",
                "Portable A",
                "/opt/a/Editor.AppImage",
                &[],
                Some("portable-a"),
            ),
            catalog_entry(
                "portable-b",
                "Portable B",
                "/opt/b/Editor.AppImage",
                &[],
                Some("portable-b"),
            ),
        ],
        failure: None,
        ..Default::default()
    };

    let (snap, snap_source) = catalog.observe(
        &available_executable("/snap/firefox/123/usr/lib/firefox/firefox"),
        &["/snap/firefox/123/usr/lib/firefox/firefox".into()],
        20,
    );
    assert_eq!(snap_source, SourceOutcome::Available);
    assert_eq!(
        snap.current_value()
            .map(|identity| identity.launcher_id.as_str()),
        Some("snap.firefox")
    );

    let (appimage, appimage_source) = catalog.observe(
        &available_executable("/opt/b/Editor.AppImage"),
        &["/opt/b/Editor.AppImage".into()],
        21,
    );
    assert_eq!(appimage_source, SourceOutcome::Available);
    assert_eq!(
        appimage
            .current_value()
            .map(|identity| identity.launcher_id.as_str()),
        Some("portable-b")
    );

    let (unknown_appimage, unknown_source) = catalog.observe(
        &available_executable("/tmp/.mount-editor/Editor.AppImage"),
        &["/tmp/.mount-editor/Editor.AppImage".into()],
        22,
    );
    assert_eq!(
        unknown_appimage.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::Unsupported)
    );
    assert_eq!(
        unknown_source,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn hidden_nodisplay_and_non_application_entries_are_not_app_identities() {
    for text in [
        "[Desktop Entry]\nType=Application\nHidden=true\nName=Hidden\nExec=/bin/hidden\n",
        "[Desktop Entry]\nType=Application\nNoDisplay=true\nName=No Display\nExec=/bin/no-display\n",
        "[Desktop Entry]\nType=Link\nName=Link\nExec=/bin/link\n",
    ] {
        assert!(parse_desktop_entry(text, "ignored.desktop").is_none());
    }
}

#[test]
fn shared_browser_executable_uses_argv_to_select_the_pwa() {
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: vec![
            catalog_entry(
                "google-chrome",
                "Google Chrome",
                "/usr/bin/google-chrome",
                &["%U"],
                Some("chrome"),
            ),
            catalog_entry(
                "chrome-mail",
                "Mail PWA",
                "/usr/bin/google-chrome",
                &["--profile-directory=Default", "--app-id=abc123", "%U"],
                Some("chrome-mail"),
            ),
        ],
        failure: None,
        ..Default::default()
    };
    let (observation, source) = catalog.observe(
        &available_executable("/usr/bin/google-chrome"),
        &[
            "/usr/bin/google-chrome".into(),
            "--profile-directory=Default".into(),
            "--app-id=abc123".into(),
        ],
        20,
    );

    assert_eq!(source, SourceOutcome::Available);
    assert_eq!(
        observation
            .current_value()
            .map(|identity| identity.launcher_id.as_str()),
        Some("chrome-mail")
    );
}

#[test]
fn chrome_wrapper_path_and_split_flags_select_the_pwa() {
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: vec![
            catalog_entry(
                "google-chrome",
                "Google Chrome",
                "/usr/bin/google-chrome-stable",
                &["%U"],
                Some("chrome"),
            ),
            catalog_entry(
                "chrome-mail",
                "Mail PWA",
                "/usr/bin/google-chrome-stable",
                &["--profile-directory=Default", "--app-id=abc123", "%U"],
                Some("chrome-mail"),
            ),
        ],
        failure: None,
        ..Default::default()
    };

    let (observation, source) = catalog.observe(
        &available_executable("/opt/google/chrome/chrome"),
        &[
            "/opt/google/chrome/chrome".into(),
            "--profile-directory".into(),
            "Default".into(),
            "--app-id=abc123".into(),
        ],
        20,
    );

    assert_eq!(source, SourceOutcome::Available);
    assert_eq!(
        observation
            .current_value()
            .map(|identity| identity.launcher_id.as_str()),
        Some("chrome-mail")
    );

    let (chromium, chromium_source) = catalog.observe(
        &available_executable("/usr/lib/chromium/chromium"),
        &["/usr/lib/chromium/chromium".into()],
        21,
    );
    assert_eq!(chromium.availability(), ProcessMetadataAvailability::Absent);
    assert_eq!(chromium_source, SourceOutcome::Empty);
}

#[test]
fn missing_argv_does_not_fall_back_to_the_generic_browser_for_a_pwa_bucket() {
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: vec![
            catalog_entry(
                "google-chrome",
                "Google Chrome",
                "/usr/bin/google-chrome",
                &[],
                Some("chrome"),
            ),
            catalog_entry(
                "chrome-mail",
                "Mail PWA",
                "/usr/bin/google-chrome",
                &["--app-id=abc123"],
                Some("chrome-mail"),
            ),
        ],
        failure: None,
        ..Default::default()
    };
    let (observation, source) =
        catalog.observe(&available_executable("/usr/bin/google-chrome"), &[], 20);

    assert_eq!(
        observation.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::Unsupported)
    );
    assert_eq!(source, SourceOutcome::Unavailable(FailureKind::Unsupported));
    assert_eq!(observation.current_value(), None);
}

#[test]
fn snap_run_argv_is_identity_evidence_and_wrong_package_stays_absent() {
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: vec![snap_catalog_entry(
            "snap.firefox",
            "Firefox Snap",
            "firefox",
            "firefox",
            &[],
        )],
        failure: None,
        ..Default::default()
    };

    let (matched, matched_source) = catalog.observe(
        &available_executable("/usr/bin/snap"),
        &["/usr/bin/snap".into(), "run".into(), "firefox".into()],
        20,
    );
    assert_eq!(matched_source, SourceOutcome::Available);
    assert_eq!(
        matched
            .current_value()
            .map(|identity| identity.launcher_id.as_str()),
        Some("snap.firefox")
    );

    let (wrong, wrong_source) = catalog.observe(
        &available_executable("/usr/bin/snap"),
        &["/usr/bin/snap".into(), "run".into(), "thunderbird".into()],
        21,
    );
    assert_eq!(wrong.availability(), ProcessMetadataAvailability::Absent);
    assert_eq!(wrong_source, SourceOutcome::Empty);
}

#[test]
fn appimage_mount_path_matches_only_its_desktop_image() {
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: vec![catalog_entry(
            "portable-editor",
            "Portable Editor",
            "/opt/PortableEditor/PortableEditor.AppImage",
            &[],
            Some("portable-editor"),
        )],
        failure: None,
        ..Default::default()
    };

    let (mounted, mounted_source) = catalog.observe(
        &available_executable("/tmp/.mount_PortableEditor-abc123/AppRun"),
        &["/tmp/.mount_PortableEditor-abc123/AppRun".into()],
        20,
    );
    assert_eq!(mounted_source, SourceOutcome::Available);
    assert_eq!(
        mounted
            .current_value()
            .map(|identity| identity.launcher_id.as_str()),
        Some("portable-editor")
    );

    let (unrelated, unrelated_source) = catalog.observe(
        &available_executable("/tmp/.mount-OtherApp-abc123/AppRun"),
        &["/tmp/.mount-OtherApp-abc123/AppRun".into()],
        21,
    );
    assert_eq!(
        unrelated.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::Unsupported)
    );
    assert_eq!(
        unrelated_source,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn shared_executable_without_disambiguating_argv_is_not_assigned_arbitrarily() {
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: vec![
            catalog_entry("browser-one", "Browser One", "/usr/bin/browser", &[], None),
            catalog_entry("browser-two", "Browser Two", "/usr/bin/browser", &[], None),
        ],
        failure: None,
        ..Default::default()
    };
    let (observation, source) = catalog.observe(&available_executable("/usr/bin/browser"), &[], 20);

    assert_eq!(
        observation.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::Unsupported)
    );
    assert_eq!(source, SourceOutcome::Unavailable(FailureKind::Unsupported));
    assert_eq!(observation.current_value(), None);
}

#[test]
fn missing_icon_is_a_partial_identity_not_a_generic_icon_claim() {
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: vec![catalog_entry(
            "editor",
            "Editor",
            "/usr/bin/editor",
            &[],
            None,
        )],
        failure: None,
        ..Default::default()
    };
    let (observation, source) = catalog.observe(
        &available_executable("/usr/bin/editor"),
        &["/usr/bin/editor".into()],
        20,
    );

    assert_eq!(
        observation.availability(),
        ProcessMetadataAvailability::Partial(ProcessMetadataFailure::NotFound)
    );
    assert_eq!(
        source,
        SourceOutcome::Partial(FailureKind::MissingDependency)
    );
    let identity = observation.current_value().expect("name still resolves");
    assert_eq!(identity.display_name, "Editor");
    assert!(!identity.has_icon_token());
}

#[test]
fn catalog_failure_is_unavailable_and_does_not_become_absent() {
    let executable = available_executable("/usr/bin/editor");
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: Vec::new(),
        failure: Some(ProcessMetadataFailure::PermissionDenied),
        ..Default::default()
    };
    let (observation, source) = catalog.observe(&executable, &[], 20);

    assert_eq!(
        observation.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PermissionDenied)
    );
    assert_eq!(
        source,
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    );
}

#[test]
fn xdg_user_desktop_entry_wins_by_id_and_entries_are_deterministic() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-application-catalog-{}",
        std::process::id()
    ));
    let user = root.join("user");
    let system = root.join("system");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(user.join("applications")).expect("user fixture directory");
    fs::create_dir_all(system.join("applications")).expect("system fixture directory");
    fs::write(
        user.join("applications/org.example.Editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=User Editor\nExec=/usr/bin/editor\n",
    )
    .expect("user desktop fixture");
    fs::write(
        system.join("applications/org.example.Editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=System Editor\nExec=/usr/bin/editor\n",
    )
    .expect("system desktop fixture");
    fs::write(
        system.join("applications/org.example.Terminal.desktop"),
        "[Desktop Entry]\nType=Application\nName=Terminal\nExec=/usr/bin/terminal\n",
    )
    .expect("second desktop fixture");

    let (entries, failure) = load_catalog_from_dirs(&[user, system]);
    assert_eq!(failure, None);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.identity.launcher_id.as_str())
            .collect::<Vec<_>>(),
        vec!["org.example.Editor", "org.example.Terminal"]
    );
    assert_eq!(entries[0].identity.display_name, "User Editor");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn catalog_resolves_hicolor_asset_without_leaking_its_linux_path() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-application-icon-catalog-{}",
        std::process::id()
    ));
    let data = root.join("data");
    let applications = data.join("applications");
    let icon_dir = data.join("icons/hicolor/scalable/apps");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&applications).expect("application fixture directory");
    fs::create_dir_all(&icon_dir).expect("icon theme fixture directory");
    fs::write(
        applications.join("org.example.Editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=/usr/bin/editor\nIcon=editor\n",
    )
    .expect("desktop fixture");
    fs::write(
        icon_dir.join("editor.svg"),
        b"<?xml version=\"1.0\"?><svg></svg>",
    )
    .expect("icon fixture");

    let (entries, failure) = load_catalog_from_dirs(std::slice::from_ref(&data));
    assert_eq!(failure, None);
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries,
        failure,
        icon_dirs: vec![data.clone()],
        ..Default::default()
    };
    let (observation, source) = catalog.observe(
        &available_executable("/usr/bin/editor"),
        &["/usr/bin/editor".into()],
        20,
    );
    assert_eq!(source, SourceOutcome::Available);
    let identity = observation.current_value().expect("desktop catalog entry");
    assert_eq!(identity.icon_token.as_deref(), Some("editor"));
    assert_eq!(identity.icon_failure, None);
    let asset = identity.icon_asset.as_ref().expect("icon bytes");
    assert_eq!(asset.format, taskmanager_core::ApplicationIconFormat::Svg);
    assert!(asset.bytes.starts_with(b"<?xml"));
    let root_text = root.to_string_lossy();
    assert!(!String::from_utf8_lossy(&asset.bytes).contains(root_text.as_ref()));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn application_retention_requires_the_exact_nonzero_start_token() {
    let previous = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(7)
        .application_identity_observation(ProcessMetadataObservation::available(
            identity("editor", "Editor", Some("editor")),
            10,
        ))
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(600, 10),
            ..ProcessScalarObservations::default()
        })
        .build();
    let failed = ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PermissionDenied);

    let same = retain_for_same_identity(failed.clone(), Some(600), Some(&previous));
    assert_eq!(
        same.availability(),
        ProcessMetadataAvailability::Stale(ProcessMetadataFailure::PermissionDenied)
    );
    assert_eq!(same.current_value(), None);
    assert!(same.last_known_value().is_some());

    let reused = retain_for_same_identity(failed.clone(), Some(601), Some(&previous));
    assert_eq!(
        reused.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PermissionDenied)
    );
    assert_eq!(reused.last_known_value(), None);

    let zero = retain_for_same_identity(failed, Some(0), Some(&previous));
    assert_eq!(
        zero.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PermissionDenied)
    );
    assert_eq!(zero.last_known_value(), None);

    let raced = retain_for_same_identity(
        ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PidRace),
        Some(600),
        Some(&previous),
    );
    assert_eq!(
        raced.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PidRace)
    );
    assert_eq!(raced.last_known_value(), None);
}

#[test]
fn process_guard_uses_real_proc_start_token_and_rejects_reuse() {
    let pid = std::process::id();
    let expected = super::super::procfs::read_proc_stat(pid)
        .expect("the test process must have a readable proc stat")
        .start_ticks;
    let executable = available_executable("/usr/bin/not-in-catalog");
    let mut catalog = ApplicationCatalog {
        loaded_at_ms: Some(10),
        entries: Vec::new(),
        failure: None,
        ..Default::default()
    };

    let (current, current_source) = catalog.observe_for_process(
        pid,
        Ok(expected),
        &executable,
        &["/usr/bin/not-in-catalog".into()],
        20,
    );
    assert_eq!(current.availability(), ProcessMetadataAvailability::Absent);
    assert_eq!(current_source, SourceOutcome::Empty);

    let (reused, reused_source) = catalog.observe_for_process(
        pid,
        Ok(expected.saturating_add(1)),
        &executable,
        &["/usr/bin/not-in-catalog".into()],
        21,
    );
    assert_eq!(
        reused.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PidRace)
    );
    assert_eq!(
        reused_source,
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(reused.current_value(), None);
}

#[test]
fn live_process_refresh_publishes_explicit_application_identity_states() {
    let mut manager = ProcessManager::new();
    let snapshot = manager.refresh_at(1_000);
    let source = snapshot
        .sources
        .iter()
        .find(|source| source.provider.as_str() == "linux.process.desktop.application");
    assert!(
        source.is_some(),
        "desktop application identity source must be present in every process tick"
    );
    assert!(
        source.is_some_and(|source| source.item_count <= snapshot.items.len()),
        "application source count cannot exceed the process inventory"
    );
    assert!(
        snapshot.items.iter().all(|item| !matches!(
            item.application_identity_observation().availability(),
            ProcessMetadataAvailability::Unknown
        )),
        "a live process tick must publish an explicit application state"
    );
}

#[test]
fn live_process_refresh_publishes_pss_and_swap_as_independent_sources() {
    let mut manager = ProcessManager::new();
    let snapshot = manager.refresh_at(1_000);
    let pss = snapshot
        .sources
        .iter()
        .find(|source| source.provider.as_str() == "linux.process.procfs.memory-pss");
    let swap = snapshot
        .sources
        .iter()
        .find(|source| source.provider.as_str() == "linux.process.procfs.swap");

    assert!(
        pss.is_some(),
        "PSS source must be present in every process tick"
    );
    assert!(
        swap.is_some(),
        "Swap source must be present in every process tick"
    );
    for source in [pss, swap].into_iter().flatten() {
        assert!(
            source.item_count <= snapshot.items.len(),
            "source success count cannot exceed the process inventory"
        );
        match source.outcome {
            SourceOutcome::Available | SourceOutcome::Partial(_) => {
                assert!(source.item_count > 0);
            }
            SourceOutcome::Empty => assert!(snapshot.items.is_empty()),
            SourceOutcome::Unavailable(_) => assert_eq!(source.item_count, 0),
        }
    }
}

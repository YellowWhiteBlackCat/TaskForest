use super::*;

#[test]
fn win_environment_providers_degrade_honestly_off_windows() {
    // On Linux CI the native WTS source is dormant: MissingDependency,
    // never fabricated sessions. On a real Windows host it must enumerate
    // the live sessions honestly (the Services session always exists).
    let mut sessions = WinSessionInventoryProvider;
    if cfg!(windows) {
        let snapshot = sessions
            .refresh()
            .expect("WTS enumerates the live sessions on a real Windows host");
        assert!(
            !snapshot.items.is_empty(),
            "a real Windows host always has the Services session"
        );
    } else {
        assert_eq!(sessions.refresh(), Err(ProviderFailure::MissingDependency));
    }
    // Boot evidence reads the Diagnostics-Performance channel through the
    // winevt boundary. Off-Windows the channel is a dormant native source,
    // so the snapshot is a typed Unsupported state with no fabricated units;
    // on a live host the readable channel yields at most the single
    // documented boot node.
    let mut evidence = WinStartupEvidenceProvider;
    let snapshot = evidence
        .observe(1)
        .expect("boot evidence degrades to Ok snapshot");
    if !cfg!(windows) {
        assert_eq!(
            snapshot.state,
            DeviceState::default().transition(DeviceStatus::Unsupported, 1)
        );
        assert_eq!(
            snapshot.failed_units_failure,
            Some(StartupEvidenceFailure::Unsupported)
        );
        assert_eq!(
            snapshot.critical_chain_failure,
            Some(StartupEvidenceFailure::Unsupported)
        );
    }
    assert!(
        snapshot.failed_units.is_empty(),
        "Windows has no systemd-style failed-units concept"
    );
    assert!(
        snapshot.critical_chain.len() <= 1,
        "the Windows critical chain is at most the single boot node"
    );
    // Session control degrades honestly off-Windows: the native WTS source
    // is missing, so Disconnect is MissingDependency (NOT Unsupported). The
    // session number stays far outside the WTS range so the logoff call is
    // side-effect free on a real host.
    let mut session_control = WinSessionControlProvider;
    let disconnect = session_control.control(
        &SessionId::new("windows:session:4294967294"),
        SessionControlAction::Disconnect,
    );
    assert_ne!(
        disconnect,
        Err(ProviderFailure::Unsupported),
        "Disconnect must degrade to a non-Unsupported failure when logoff is absent"
    );
    assert!(
        disconnect.is_err(),
        "logoff of a nonexistent session must never fabricate success"
    );
    if !cfg!(windows) {
        assert_eq!(
            disconnect,
            Err(ProviderFailure::MissingDependency),
            "logoff absent on Linux CI -> MissingDependency, never fabricated success"
        );
    }
    // Lock maps to LockWorkStation on the calling interactive session. The
    // call is intentionally NOT exercised on a real host here — it would
    // lock the interactive session running the tests — so only the honest
    // off-Windows degradation is asserted.
    if !cfg!(windows) {
        assert_eq!(
            session_control.control(
                &SessionId::new("windows:session:1"),
                SessionControlAction::Lock
            ),
            Err(ProviderFailure::MissingDependency),
            "LockWorkStation absent on Linux CI -> MissingDependency, never fabricated success"
        );
    }
    // A non-conformant session id (no `windows:session:` prefix or trailing
    // non-numeric) is reported as IdentityChanged, never crashes logoff.
    assert_eq!(
        session_control.control(
            &SessionId::new("not-a-windows-session"),
            SessionControlAction::Disconnect,
        ),
        Err(ProviderFailure::IdentityChanged),
    );
}

#[cfg(windows)]
#[test]
fn startup_folder_resolution_failure_is_not_reported_as_complete() {
    assert_eq!(
        startup_inventory_outcome(0, Some(FailureKind::PermissionDenied), None),
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(
        startup_inventory_outcome(1, None, None),
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(
        startup_inventory_outcome(0, None, None),
        SourceOutcome::Available
    );
    // An unavailable Task Scheduler source is a typed partial outcome, never
    // a healthy-but-empty inventory; a missing Startup folder outranks it.
    assert_eq!(
        startup_inventory_outcome(0, None, Some(FailureKind::TemporarilyUnavailable)),
        SourceOutcome::Partial(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        startup_inventory_outcome(
            0,
            Some(FailureKind::PermissionDenied),
            Some(FailureKind::TemporarilyUnavailable)
        ),
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );
}

#[cfg(windows)]
#[test]
fn scheduled_task_rows_carry_stable_ids_and_unprovable_scope() {
    let entry = scheduled_task_entry(taskmanager_windows_api::WindowsStartupTask {
        task_path: "\\Microsoft\\Windows\\TestTask".to_string(),
        name: Some("TestTask".to_string()),
        enabled: false,
        has_logon_or_boot_trigger: true,
    })
    .expect("a bounded well-formed task maps to a row");
    assert_eq!(entry.id.as_str(), "win:task:\\Microsoft\\Windows\\TestTask");
    assert_eq!(entry.locator.as_str(), "\\Microsoft\\Windows\\TestTask");
    assert_eq!(entry.source, StartupSource::ScheduledTask);
    assert_eq!(entry.scope, StartupScope::Unknown);
    assert_eq!(entry.control_policy, StartupControlPolicy::Unsupported);
    assert!(!entry.enabled);
    // A task without a carried display name falls back to its own name
    // portion; degenerate identities stay typed ProviderFault rows.
    let nameless = scheduled_task_entry(taskmanager_windows_api::WindowsStartupTask {
        task_path: "\\TestTask".to_string(),
        name: None,
        enabled: true,
        has_logon_or_boot_trigger: true,
    })
    .expect("the path's name portion is the fallback display name");
    assert_eq!(nameless.name, "TestTask");
    assert_eq!(
        scheduled_task_entry(taskmanager_windows_api::WindowsStartupTask {
            task_path: "\\".to_string(),
            name: None,
            enabled: true,
            has_logon_or_boot_trigger: true,
        }),
        Err(FailureKind::ProviderFault),
        "an empty identity is never fabricated into a row"
    );
}

#[cfg(windows)]
#[test]
fn unsupported_startup_sources_report_typed_unsupported() {
    let mut control = WinStartupControlProvider;
    // Scheduled-task mutation is out of scope: the task store is never
    // touched through the registry/folder control provider.
    let task_entry = StartupEntry {
        id: StartupEntryId::new("win:task:\\Microsoft\\Windows\\TestTask"),
        name: "TestTask".into(),
        exec: "\\Microsoft\\Windows\\TestTask".into(),
        enabled: true,
        source: StartupSource::ScheduledTask,
        scope: StartupScope::Unknown,
        control_policy: StartupControlPolicy::Unsupported,
        locator: StartupEntryLocator::new("\\Microsoft\\Windows\\TestTask"),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: taskmanager_core::StartupImpactUnknownReason::Unsupported,
        },
    };
    assert_eq!(
        control.set_enabled(&task_entry, false),
        Err(ProviderFailure::Unsupported),
        "scheduled-task control stays unsupported until a task-store seam is chartered"
    );
    // Folder-item control validates against the live Startup folder first,
    // so an absent item degrades to IdentityChanged without any registry
    // write and without ever touching a file.
    let folder_entry = StartupEntry {
        id: StartupEntryId::new("win:folder:__taskforest_absent_item__.lnk"),
        name: "__taskforest_absent_item__".into(),
        exec: "C:\\does\\not\\exist.lnk".into(),
        enabled: true,
        source: StartupSource::StartupFolder,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new("win:folder:__taskforest_absent_item__.lnk"),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: taskmanager_core::StartupImpactUnknownReason::Unsupported,
        },
    };
    assert_eq!(
        control.set_enabled(&folder_entry, false),
        Err(ProviderFailure::IdentityChanged),
        "folder control never fabricates success for an item that left the Startup folder"
    );
    // A folder row whose id is not the provider's own scheme is rejected
    // before any native access.
    let mut foreign = folder_entry.clone();
    foreign.id = StartupEntryId::new("win:run:hkcu:not-a-folder-item");
    assert_eq!(
        control.set_enabled(&foreign, false),
        Err(ProviderFailure::IdentityChanged)
    );
}

#[cfg(windows)]
#[test]
fn startup_approval_parser_rejects_unknown_binary_without_guessing() {
    let unknown_bytes = [0_u8; 12];
    let mut unknown = windows_registry::Value::from(&unknown_bytes[..]);
    assert_eq!(decode_startup_approval(&unknown), StartupApproval::Unknown);
    unknown.set_ty(windows_registry::Type::Bytes);
    assert_eq!(decode_startup_approval(&unknown), StartupApproval::Unknown);

    let mut enabled = [0_u8; 12];
    enabled[0] = 0x02;
    let enabled = windows_registry::Value::from(&enabled[..]);
    assert_eq!(decode_startup_approval(&enabled), StartupApproval::Enabled);

    let mut disabled = [0_u8; 12];
    disabled[0] = 0x03;
    let disabled = windows_registry::Value::from(&disabled[..]);
    assert_eq!(
        decode_startup_approval(&disabled),
        StartupApproval::Disabled
    );
}

use super::*;

// ── BN-05 boot timeline ──────────────────────────────────────────────────────

/// A measured critical chain: two timed units plus one untimed node.
fn timeline_evidence() -> taskmanager_core::core::startup::StartupBootEvidenceSnapshot {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::startup::StartupCriticalChainNode;

    let healthy = DeviceState::healthy(1_785_292_800_000);
    taskmanager_core::core::startup::StartupBootEvidenceSnapshot {
        state: healthy,
        failed_units_state: healthy,
        critical_chain_state: healthy,
        failed_units_failure: None,
        critical_chain_failure: None,
        failed_units: Vec::new(),
        critical_chain: vec![
            StartupCriticalChainNode {
                unit: "dbus.service".into(),
                activated_at_ms: Some(500),
                duration_ms: Some(1_200),
            },
            StartupCriticalChainNode {
                unit: "graphical.target".into(),
                activated_at_ms: None,
                duration_ms: None,
            },
            StartupCriticalChainNode {
                unit: "multi-user.target".into(),
                activated_at_ms: Some(1_700),
                duration_ms: Some(2_500),
            },
        ],
    }
}

/// Measured state: unit windows project sorted with normalized fractions and
/// their durations; the untimed node lands in an honest listing row, never a
/// fabricated position.
#[test]
fn startup_timeline_projects_measured_windows_and_honest_unknown_rows() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    let evidence = timeline_evidence();
    let (total_ms, rows) = startup_timeline(Some(&evidence)).expect("measured evidence");
    assert_eq!(total_ms, 4_200);
    assert_eq!(rows.len(), 3);
    match &rows[0] {
        TimelineRowKind::Measured {
            unit,
            fraction,
            duration_ms,
        } => {
            assert_eq!(unit, "dbus.service");
            assert!((*fraction - 1_200.0 / 4_200.0).abs() < 0.001);
            assert_eq!(*duration_ms, 1_200);
        }
        other => panic!("expected a measured row, got {other:?}"),
    }
    match &rows[1] {
        TimelineRowKind::Measured { unit, .. } => assert_eq!(unit, "multi-user.target"),
        other => panic!("expected a measured row, got {other:?}"),
    }
    match &rows[2] {
        TimelineRowKind::Untimed { count, names } => {
            assert_eq!(*count, 1);
            assert_eq!(names.as_slice(), ["graphical.target"]);
        }
        other => panic!("expected an untimed row, got {other:?}"),
    }
}

/// No evidence (or a typed failure) keeps the block silent: no fabricated
/// zero-ms waterfall, no invented comparison baseline.
#[test]
fn startup_timeline_stays_silent_without_evidence_or_on_typed_failure() {
    assert!(startup_timeline(None).is_none(), "no evidence yet: silent");

    let mut failing = timeline_evidence();
    failing.critical_chain_failure =
        Some(taskmanager_core::core::startup::StartupEvidenceFailure::MissingTool);
    assert!(
        startup_timeline(Some(&failing)).is_none(),
        "a typed failure must suppress the waterfall, never render stale bars"
    );
}

/// A chain beyond the segment cap collapses the tail into a bounded +N row.
#[test]
fn startup_timeline_collapses_overflow_into_a_bounded_row() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::startup::StartupCriticalChainNode;

    let healthy = DeviceState::healthy(1);
    let chain: Vec<StartupCriticalChainNode> = (0..25)
        .map(|i| {
            let i: u64 = i;
            StartupCriticalChainNode {
                unit: format!("unit{i}.service"),
                activated_at_ms: Some(i * 100),
                duration_ms: Some(50),
            }
        })
        .collect();
    let evidence = taskmanager_core::core::startup::StartupBootEvidenceSnapshot {
        state: healthy,
        failed_units_state: healthy,
        critical_chain_state: healthy,
        critical_chain: chain,
        ..taskmanager_core::core::startup::StartupBootEvidenceSnapshot::default()
    };
    let (_, rows) = startup_timeline(Some(&evidence)).expect("large chain projects");
    assert_eq!(
        rows.last(),
        Some(&TimelineRowKind::Collapsed { count: 5 }),
        "the overflow tail must project as +N"
    );
}

/// The timeline block never joins the focus/selection domain: with the
/// waterfall present, ArrowDown still moves the shared table cursor and the
/// page still composes.
#[test]
fn startup_page_keyboard_navigation_skips_the_timeline_block() {
    use taskmanager_application::Modifiers;
    let mut app = crate::IcedApp::demo();
    assert!(
        app.shell.projection().startup_boot_evidence.is_some(),
        "the demo frame seeds typed boot evidence"
    );
    let _ = app.update(crate::app::Message::SelectPage(AppPage::Startup));
    assert_eq!(app.shell.selected, 0);
    let _ = app.update(crate::app::Message::Key(crate::keys::IcedKey::Fixed(
        taskmanager_shell::ShellKeyEvent::new(
            taskmanager_application::KeyCode::ArrowDown,
            Modifiers::NONE,
        ),
    )));
    assert_eq!(
        app.shell.selected, 1,
        "ArrowDown moves the table cursor; timeline rows are not focus targets"
    );
    let _ = app.update(crate::app::Message::Key(crate::keys::IcedKey::Fixed(
        taskmanager_shell::ShellKeyEvent::new(
            taskmanager_application::KeyCode::ArrowUp,
            Modifiers::NONE,
        ),
    )));
    assert_eq!(app.shell.selected, 0);
    let _view = view(&app);
}

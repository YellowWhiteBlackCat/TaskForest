//! Startup-page render tests: the Enable/Disable menu overlay, the gated
//! confirmation, the impact/source column projections, and the BN-05 boot
//! timeline waterfall.

use super::frame_text;
use crate::TuiTheme;
use crate::ui::confirmations::render_startup_control_confirmation;
use crate::ui::pages::{startup_impact_text, startup_source_text};
use crate::ui::startup_menu;

use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::startup::{
    StartupBootEvidenceSnapshot, StartupCriticalChainNode, StartupEvidenceFailure,
};

/// A measured critical chain: two timed units plus one untimed node (the
/// same shape as the live systemd-user chain the Linux provider reports).
fn evidence_fixture() -> StartupBootEvidenceSnapshot {
    let healthy = DeviceState::healthy(1_785_292_800_000);
    StartupBootEvidenceSnapshot {
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

fn entry_fixture() -> taskmanager_core::core::startup::StartupEntry {
    use taskmanager_core::core::startup::{
        StartupControlPolicy, StartupImpact, StartupImpactEvidence, StartupScope, StartupSource,
    };
    taskmanager_core::core::startup::StartupEntry {
        id: taskmanager_core::core::startup::StartupEntryId::new("fixture:demo"),
        name: "demo-autostart.desktop".into(),
        exec: "/usr/bin/demo --daemon".into(),
        enabled: true,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: "demo-autostart.desktop".into(),
        impact: StartupImpact::Low,
        impact_evidence: StartupImpactEvidence::Measured { duration_ms: 42 },
    }
}

#[test]
fn startup_impact_and_source_columns_carry_evidence_and_scope() {
    // Localized projections: serialize against the language-flipping i18n
    // test (see `ui::LANG_TEST_GUARD`) — a concurrent set_language(Zh) would
    // otherwise translate these assertions.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let entry = entry_fixture();
    // The measured boot impact renders the duration; an unmeasured one is
    // honest about it — never a fabricated number.
    assert_eq!(startup_impact_text(&entry), "Low · 42 ms");
    let mut unknown = entry.clone();
    unknown.impact_evidence = taskmanager_core::core::startup::StartupImpactEvidence::Unknown {
        reason: taskmanager_core::core::startup::StartupImpactUnknownReason::NotInstrumented,
    };
    assert_eq!(startup_impact_text(&unknown), "Low · unmeasured");
    // The source column carries the scope suffix (GPUI parity).
    assert_eq!(startup_source_text(&entry), "Desktop Entry · User");
}

#[test]
fn startup_menu_and_confirmation_render_without_panicking() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Startup,
    ));
    let theme = TuiTheme::default();
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).expect("test terminal");
    let _ = terminal
        .draw(|frame| {
            let area = frame.area();
            let menu = startup_menu::StartupMenuTarget {
                entry: entry_fixture(),
                selection: 0,
            };
            // The test-only entry takes the committed focus plan explicitly:
            // this fixture mirrors the live StartupMenu surface addressing
            // item 0, exactly as the frame plan would project it.
            startup_menu::render_startup_menu(
                frame,
                &menu,
                theme,
                crate::ui::frame_plan::TuiFocusPlan {
                    target: crate::ui::frame_plan::TuiFocusTarget::LocalSurface(
                        crate::TuiSurfaceKind::StartupMenu,
                    ),
                    order: crate::ui::frame_plan::TuiFocusOrder::None,
                    control: crate::ui::frame_plan::TuiFocusControl::MenuItem {
                        surface: crate::TuiSurfaceKind::StartupMenu,
                        index: menu.selection,
                    },
                },
                area,
            );
        })
        .expect("draw");

    // Produce a real pending gate through the shell (request ids are
    // application-owned and cannot be fabricated here).
    let _ = app.shell.request_startup_control(false);
    let pending = app
        .shell
        .pending_startup()
        .cloned()
        .expect("gated pending startup");
    let _ = terminal
        .draw(|frame| {
            render_startup_control_confirmation(frame, theme, &pending, frame.area());
        })
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(text.contains("SSH Agent"));
}

#[test]
fn startup_page_renders_the_enhanced_columns() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Startup,
    ));
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("ssh-agent"), "fixture entry name visible");
    assert!(
        text.contains("· User") || text.contains("· System"),
        "source column carries its scope"
    );
}

// ── BN-05 boot timeline ──────────────────────────────────────────────────────

/// The demo frame seeds a measured chain, so the waterfall must project the
/// measured state: unit windows with bars and durations plus the total span.
#[test]
fn boot_timeline_measured_state_projects_unit_windows_with_bars() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Startup,
    ));
    let text = frame_text(&app, 120, 40);
    assert!(
        text.contains("Boot timeline"),
        "measured evidence must render the timeline block"
    );
    assert!(text.contains("dbus.service"), "measured unit window row");
    assert!(
        text.contains("multi-user.target"),
        "measured unit window row"
    );
    assert!(text.contains("1200 ms"), "measured duration stays visible");
    assert!(text.contains("█"), "a measured window draws bar cells");
}

/// Untimed nodes are counted and listed — never placed on the time axis, and
/// never fabricated into a bar. A typed failure suppresses the whole block.
#[test]
fn boot_timeline_unknown_state_lists_untimed_units_without_bars() {
    let mut app = crate::TuiApp::from_shell(taskmanager_shell::demo_app());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupBootEvidence(Some(
            evidence_fixture(),
        )),
    );
    let _ = app.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Startup,
    ));
    let text = frame_text(&app, 120, 40);
    assert!(
        text.contains("No timing data"),
        "untimed units must be listed honestly"
    );
    assert!(
        text.contains("graphical.target"),
        "the untimed unit name is surfaced"
    );
    assert!(
        text.contains("1 · graphical.target"),
        "count plus bounded name list"
    );

    // The untimed row carries no bar: the projection never invents a position.
    let projection = crate::ui::boot_timeline::project_timeline(Some(&evidence_fixture()))
        .expect("measured fixture projects rows");
    let untimed = projection
        .rows
        .iter()
        .find(|row| row.label == "No timing data")
        .expect("untimed row present");
    assert_eq!(untimed.bar_cells, 0, "untimed rows never draw a bar");
    assert!(untimed.dim, "untimed metadata renders dim");
}

/// No evidence (or a typed failure) keeps the block silent: no fabricated
/// zero-ms waterfall, no comparison baseline invented.
#[test]
fn boot_timeline_stays_silent_without_typed_evidence_or_on_failure() {
    let mut app = crate::TuiApp::from_shell(taskmanager_shell::demo_app());
    let _ = app.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Startup,
    ));
    let text = frame_text(&app, 120, 40);
    assert!(
        !text.contains("Boot timeline"),
        "no evidence yet: the block must not render"
    );

    let mut failing = evidence_fixture();
    failing.critical_chain_failure = Some(StartupEvidenceFailure::MissingTool);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupBootEvidence(Some(failing)),
    );
    let text = frame_text(&app, 120, 40);
    assert!(
        !text.contains("Boot timeline"),
        "a typed failure must suppress the waterfall, never render stale bars"
    );
}

/// A chain beyond the segment cap collapses the tail into a bounded +N row
/// instead of growing the block without limit.
#[test]
fn boot_timeline_collapses_overflow_into_a_bounded_row() {
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
    let mut app = crate::TuiApp::from_shell(taskmanager_shell::demo_app());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupBootEvidence(Some(
            StartupBootEvidenceSnapshot {
                state: healthy,
                failed_units_state: healthy,
                critical_chain_state: healthy,
                critical_chain: chain,
                ..StartupBootEvidenceSnapshot::default()
            },
        )),
    );
    let projection = crate::ui::boot_timeline::project_timeline(
        app.shell.projection().startup_boot_evidence.as_ref(),
    )
    .expect("large chain projects rows");
    assert!(
        projection.rows.len()
            <= taskmanager_core::core::startup::DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS + 1,
        "segment rows are capped plus one collapsed row"
    );
    let collapsed = projection
        .rows
        .last()
        .filter(|row| row.detail == "+5")
        .expect("overflow projects a +N row");
    assert_eq!(collapsed.bar_cells, 0);
}

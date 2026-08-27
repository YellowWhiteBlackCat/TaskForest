//! Process Properties modal: open/switch/close behavior + per-tab render.
//!
//! These tests drive `handle_key` (the same path crossterm uses) and render the
//! real frame through `render`, asserting on the drawn frame text — not source
//! `.contains()`. The modal is the sole owner of the tab-row hint string and
//! the Performance "(peak …)" suffix, so those substrings cleanly prove the
//! modal rendered and the active tab advanced.

use super::super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyModifiers;
use taskmanager_application::AppPage;
use taskmanager_application::i18n::{Language, set_language};

use crate::render;
use crate::{TuiApp, TuiTheme};

/// Render the live frame through the same TestBackend path the render tests
/// use, pinning English + serializing against the language-flipping i18n test.
fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

/// A demo app parked on the Applications page with a selected process.
fn app_on_processes() -> TuiApp {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.shell.selected = 0;
    app.reconcile_applications_cursor();
    app
}

/// The tab-row hint is modal-unique (nowhere else in the TUI renders the
/// "scroll · Esc close" chord), so its presence proves the Properties modal
/// painted this frame.
const MODAL_HINT: &str = "scroll · Esc close";

#[test]
fn enter_opens_properties_modal_showing_process_identity() {
    let mut app = app_on_processes();
    // The selected process identity is what the modal title must surface.
    let (name, pid) = app
        .selected_detail_process()
        .map(|p| (p.name.clone(), p.pid))
        .expect("demo Applications page has a selected process");
    assert!(app.process_properties().is_none(), "modal starts closed");

    // Enter on the Applications page opens the modal.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.process_properties().is_some(),
        "Enter must open the Properties modal"
    );

    // The drawn frame carries the modal hint, the process name, and the pid.
    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains(MODAL_HINT),
        "modal tab-row hint must render, got:\n{text}"
    );
    assert!(
        text.contains(&name) && text.contains(&pid.to_string()),
        "modal must surface process identity ({name}, {pid}), got:\n{text}"
    );
}

#[test]
fn trigger_does_nothing_when_no_process_is_selected() {
    // A fresh shell has no processes at all; on the Applications page Enter must
    // not open the modal (honest no-op, never a fabricated empty target).
    let mut app = TuiApp::from_shell(taskmanager_shell::ShellApp::new());
    app.application.active_page = AppPage::Applications;
    assert!(app.visible_processes().is_empty());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.process_properties().is_none(),
        "Enter on an empty process list must not open the modal"
    );
}

#[test]
fn tab_key_advances_to_performance_tab_with_peak_content() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // Default tab is Overview; one Tab advances to Performance.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    let target = app
        .process_properties()
        .expect("modal stays open after tab switch");
    assert_eq!(
        target.section,
        crate::ProcessDetailsSection::Performance,
        "Tab must advance to the Performance section"
    );

    // The Performance tab is the only surface that renders the "(peak …)"
    // suffix (the inline detail panel shows no peaks), so its presence proves
    // the tab content advanced, not just the section enum.
    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("(peak"),
        "Performance tab must render current+peak rows, got:\n{text}"
    );
}

#[test]
fn right_arrow_cycles_through_all_four_tabs() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // Right arrow walks Overview → Performance → Command → Insights → Overview.
    for expected in [
        crate::ProcessDetailsSection::Performance,
        crate::ProcessDetailsSection::Command,
        crate::ProcessDetailsSection::Insights,
        crate::ProcessDetailsSection::Overview,
    ] {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Right,
                KeyModifiers::NONE,
            ),
        );
        let target = app
            .process_properties()
            .expect("modal stays open while cycling");
        assert_eq!(
            target.section, expected,
            "Right must advance to {expected:?}"
        );
    }
}

#[test]
fn command_tab_renders_command_line_or_honest_dash() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // Advance to Command (two Tabs).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    let target = app.process_properties().expect("modal stays open");
    assert_eq!(target.section, crate::ProcessDetailsSection::Command);

    // The Command tab renders the "Command line" label (modal-localized kv
    // label); the value is the frozen row's cmdline or an honest dash.
    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("Command line"),
        "Command tab must render its label, got:\n{text}"
    );
}

#[test]
fn insights_tab_shows_collecting_when_no_projection_is_seeded() {
    let mut app = app_on_processes();
    // No process_insights projection seeded → the honest typed state is the
    // "collecting…" gap, never a fabricated idle.
    assert!(app.projection().process_insights.is_none());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // Advance to Insights (three Tabs).
    for _ in 0..3 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
        );
    }
    let target = app.process_properties().expect("modal stays open");
    assert_eq!(target.section, crate::ProcessDetailsSection::Insights);

    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("Loading process insights"),
        "Insights tab with no projection must render the honest gap, got:\n{text}"
    );
}

#[test]
fn insights_tab_renders_thread_list_when_projection_is_present() {
    use taskmanager_application::{
        DeviceState, ProcessIdentity, ProcessInsightFacetEvent, ProcessInsightObservation,
        ProcessInsightSnapshot, ProcessInsightsProjection, ProcessInsightsRevision,
        ProcessThreadInfo, ProcessThreads, ThreadState,
    };

    let mut app = app_on_processes();
    // Build a threads facet for the frozen selected identity and seed it as the
    // last-wins projection (the same shape a real platform batch carries).
    let target = app
        .application
        .selected_process
        .clone()
        .expect("selected row has a frozen identity");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), revision);
    let event = ProcessInsightFacetEvent::Threads(Box::new(ProcessInsightObservation {
        target: target.clone(),
        revision,
        snapshot: ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: target.pid,
                start_token: 0,
            },
            value: ProcessThreads {
                state: DeviceState::healthy(10),
                threads: vec![ProcessThreadInfo {
                    tid: 4343,
                    comm: "props-worker".to_owned(),
                    state: ThreadState::Running,
                    cpu_time_secs: Some(2.0),
                    cpu_percent: Some(37.5),
                }],
            },
        },
    }));
    let _ = tracker.apply(&event);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ProcessInsights(Box::new(
            tracker.snapshot(),
        )),
    );

    // Open the modal and advance to the Insights tab.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    for _ in 0..3 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
        );
    }
    assert_eq!(
        app.process_properties().unwrap().section,
        crate::ProcessDetailsSection::Insights
    );

    // The Threads facet renders the worker's tid + comm inside the Insights
    // tab body. (The other facets — network/gpu/resources/isolation/open_files
    // — remain Pending and honestly render their own "Loading" gap; only the
    // threads facet was projected, so only its row asserts here.)
    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("4343") && text.contains("props-worker"),
        "Insights tab must render the projected thread row, got:\n{text}"
    );
}

#[test]
fn back_tab_and_left_arrow_reverse_through_the_tabs() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // Advance to Insights (three Tabs), then BackTab walks back to Command.
    for _ in 0..3 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
        );
    }
    assert_eq!(
        app.process_properties().unwrap().section,
        crate::ProcessDetailsSection::Insights
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::BackTab,
            KeyModifiers::SHIFT,
        ),
    );
    assert_eq!(
        app.process_properties().unwrap().section,
        crate::ProcessDetailsSection::Command,
        "BackTab must reverse to the Command section"
    );
    // Left arrow reverses again to Performance.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Left, KeyModifiers::NONE),
    );
    assert_eq!(
        app.process_properties().unwrap().section,
        crate::ProcessDetailsSection::Performance,
        "Left must reverse to the Performance section"
    );
}

#[test]
fn ctrl_arrows_scroll_the_tab_body_without_switching_tabs() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    let section_before = app.process_properties().unwrap().section;

    // Ctrl+Down scrolls the body; the section never changes.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::CONTROL,
        ),
    );
    let scrolled = app.process_properties().expect("modal stays open").scroll;
    assert!(scrolled > 0, "Ctrl+Down must scroll the tab body");
    assert_eq!(
        app.process_properties().unwrap().section,
        section_before,
        "Ctrl+Down must not switch tabs"
    );

    // Ctrl+Up scrolls back.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Up,
            KeyModifiers::CONTROL,
        ),
    );
    assert!(
        app.process_properties().unwrap().scroll < scrolled,
        "Ctrl+Up must scroll the tab body back"
    );
}

#[test]
fn esc_closes_modal_and_restores_table_navigation() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.process_properties().is_some());

    // While the modal is open, a Down arrow is trapped (switches nothing — it's
    // swallowed by the modal's `_ => {}` arm) and the table cursor is frozen.
    let selected_before = app.shell.selected;
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(
        app.shell.selected, selected_before,
        "Down must not move the table cursor while the modal is open"
    );

    // Esc closes the modal.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(
        app.process_properties().is_none(),
        "Esc must close the Properties modal"
    );

    // After close, Down arrow reaches the table again (the modal no longer
    // traps navigation), proving focus restored to the process table.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_ne!(
        app.shell.selected, selected_before,
        "Down must move the table cursor after the modal closes"
    );
}

/// Seed the shared insight projection with the typed escalation-requiring
/// network facet for `app`'s selected row (the same `apply_failure` path the
/// provider takes), returning the frozen target it was seeded for.
fn seed_requires_escalation_network(app: &mut TuiApp) -> u32 {
    use taskmanager_application::i18n::set_language;
    use taskmanager_application::{
        FailureKind, ProcessInsightFacet, ProcessInsightUnavailable, ProcessInsightsProjection,
        ProcessInsightsRevision,
    };
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let target = app
        .application
        .selected_process
        .clone()
        .expect("selected row has a frozen identity");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), revision);
    let _ = tracker.apply_failure(
        &target,
        revision,
        ProcessInsightFacet::Network,
        ProcessInsightUnavailable::Provider(FailureKind::RequiresEscalation),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ProcessInsights(Box::new(
            tracker.snapshot(),
        )),
    );
    target.pid
}

/// G-04b: `e` on the Insights tab fires the shared one-shot escalation
/// effect — but only when the projected network facet reports the typed
/// `RequiresEscalation` state (the same gate as the rendered hint).
#[test]
fn e_on_insights_tab_yields_the_shared_network_escalation_effect() {
    let mut app = app_on_processes();
    let pid = seed_requires_escalation_network(&mut app);
    // Open the modal and advance to Insights (three Tabs).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    for _ in 0..3 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
        );
    }
    assert_eq!(
        app.process_properties().unwrap().section,
        crate::ProcessDetailsSection::Insights
    );

    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        effect,
        Some(PlatformEffect::ProcessNetworkEscalation),
        "`e` must produce the shared ProcessNetworkEscalation effect for pid {pid}"
    );
    // The modal stays open (the request is one-shot; Esc still closes).
    assert!(app.process_properties().is_some());
}

/// G-04b: `e` is inert without the RequiresEscalation facet — the modal
/// traps it and no effect is produced (an escalation prompt the current
/// projection did not ask for must never fire).
#[test]
fn e_without_the_requires_escalation_facet_produces_no_effect() {
    let mut app = app_on_processes();
    // No insight projection at all: the facet is Pending, not escalatable.
    assert!(app.projection().process_insights.is_none());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    for _ in 0..3 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
        );
    }
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(effect, None, "`e` must be inert without the facet");

    // The same gate holds on a PermissionDenied network facet (a plain
    // denial the per-feature seam cannot reach is not an escalation ask).
    let mut app = app_on_processes();
    let target = app
        .application
        .selected_process
        .clone()
        .expect("selected row has a frozen identity");
    use taskmanager_application::{
        FailureKind, ProcessInsightFacet, ProcessInsightUnavailable, ProcessInsightsProjection,
        ProcessInsightsRevision,
    };
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), revision);
    let _ = tracker.apply_failure(
        &target,
        revision,
        ProcessInsightFacet::Network,
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ProcessInsights(Box::new(
            tracker.snapshot(),
        )),
    );
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        effect, None,
        "a PermissionDenied facet must not fire the escalation request"
    );
}

/// G-04b: the RequiresEscalation network facet renders the typed reason line
/// plus the `e` trigger hint — never the Debug formatting of the reason (the
/// pre-fix render was `Provider(RequiresEscalation)`).
#[test]
fn insights_tab_renders_typed_escalation_reason_and_hint_never_debug() {
    let mut app = app_on_processes();
    seed_requires_escalation_network(&mut app);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    for _ in 0..3 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
        );
    }
    let text = frame_text(&app, 140, 48);
    // The typed reason line and the trigger hint render.
    assert!(
        text.contains("Per-process network capture requires authorization"),
        "typed escalation reason must render, got:\n{text}"
    );
    assert!(
        text.contains("Press e to authorize"),
        "the `e` trigger hint must render, got:\n{text}"
    );
    assert!(
        text.contains("Enable per-process network capture"),
        "the shared pill label must render alongside the key hint"
    );
    // Never the Debug formatting of the typed reason.
    assert!(
        !text.contains("RequiresEscalation") && !text.contains("Provider("),
        "the facet state must never render as Debug formatting, got:\n{text}"
    );
}

/// G-04b: the Applications-page inline insights panel binds the same `e`
/// trigger (its hint renders there too), gated on the same typed facet.
#[test]
fn e_on_the_applications_page_fires_only_for_the_escalation_facet() {
    let mut app = app_on_processes();
    // Without the facet, `e` falls through the shell router (no chord).
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(effect, None, "`e` must be inert without the facet");

    // With the typed facet on the selected row, `e` fires the shared effect.
    seed_requires_escalation_network(&mut app);
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        effect,
        Some(PlatformEffect::ProcessNetworkEscalation),
        "the inline panel's `e` must produce the shared escalation effect"
    );
}

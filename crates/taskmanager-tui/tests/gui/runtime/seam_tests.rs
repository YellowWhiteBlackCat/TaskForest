use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    Event, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use taskmanager_application::{AppAction, AppPage};

use super::seam_support::{apply_terminal_event, run_event_loop};
use super::{EventReaction, RefreshPacing, TerminalEventSource, apply_terminal_event_with_plan};
use crate::TuiApp;
use crate::ui::TuiFramePlan;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::session::{SessionControlAction, SessionIncarnation};
use taskmanager_ipc::protocol::{DaemonMessage, SubscriptionMask};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_shell::{
    FeedbackLifecycle, FeedbackSeverity, FeedbackSource, PAGE_STEP, SearchSlot,
};

/// A scripted source: pops the queued items; once empty, `poll` fails
/// with a typed error instead of blocking forever, so a regression that
/// drops the quit key FAILS the test (loop error) rather than hanging.
struct ScriptedEventSource {
    items: VecDeque<io::Result<Event>>,
}

impl ScriptedEventSource {
    fn new(events: Vec<Event>) -> Self {
        Self {
            items: events.into_iter().map(Ok).collect(),
        }
    }
}

impl TerminalEventSource for ScriptedEventSource {
    fn poll(&mut self, _timeout: Duration) -> io::Result<bool> {
        match self.items.front() {
            Some(_) => Ok(true),
            None => Err(io::Error::other("script exhausted without a quit key")),
        }
    }

    fn read(&mut self) -> io::Result<Event> {
        match self.items.pop_front() {
            Some(Ok(event)) => Ok(event),
            Some(Err(error)) => Err(error),
            None => Err(io::Error::other("read on an empty script")),
        }
    }
}

/// The frame the scripted tests "see": drive() uses a TestBackend of the
/// same size, so pointer projections address the painted layout.
const TEST_FRAME: Rect = Rect::new(0, 0, 80, 24);

fn key(code: ratatui::crossterm::event::KeyCode, kind: KeyEventKind) -> Event {
    Event::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind))
}

fn drive(app: &mut TuiApp, script: Vec<Event>) -> io::Result<()> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
    run_event_loop(
        &mut terminal,
        app,
        None,
        ScriptedEventSource::new(script),
        false,
        None,
    )
}

#[test]
fn quit_key_exits_the_loop_cleanly() {
    let mut app = crate::demo_app();
    let outcome = drive(
        &mut app,
        vec![key(
            ratatui::crossterm::event::KeyCode::Char('q'),
            KeyEventKind::Press,
        )],
    );
    assert!(outcome.is_ok());
    assert!(app.should_quit(), "'q' must set the shared quit flag");
}

#[test]
fn a_dropping_event_source_fails_the_loop_with_the_source_error_never_hangs() {
    // The remote-transport failure contract: a source that can no longer
    // deliver events surfaces as a typed loop error. The loop must
    // propagate it (never spin on a dead transport).
    let mut app = crate::demo_app();
    let outcome = drive(&mut app, vec![]);
    let error = outcome.expect_err("an exhausted script must abort the loop");
    assert!(
        error.to_string().contains("script exhausted"),
        "the loop must surface the source's own error, got: {error}"
    );
}

#[test]
fn release_events_flag_dirty_but_never_route_a_key() {
    // Terminals that emit KeyRelease must not double-advance selection:
    // the release flags a repaint (the Ctrl mirror may have changed) but
    // routes nothing.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let before = app.shell.selected;
    let outcome = drive(
        &mut app,
        vec![
            key(
                ratatui::crossterm::event::KeyCode::Down,
                KeyEventKind::Press,
            ),
            key(
                ratatui::crossterm::event::KeyCode::Down,
                KeyEventKind::Release,
            ),
            key(
                ratatui::crossterm::event::KeyCode::Down,
                KeyEventKind::Press,
            ),
            key(
                ratatui::crossterm::event::KeyCode::Char('q'),
                KeyEventKind::Press,
            ),
        ],
    );
    assert!(outcome.is_ok());
    assert_eq!(
        app.shell.selected,
        before.saturating_add(2),
        "two presses advance two rows; the release between them must not"
    );
}

fn scroll(kind: MouseEventKind, modifiers: KeyModifiers) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 40,
        row: 10,
        modifiers,
    })
}

#[test]
fn bare_wheel_pages_the_table_through_the_keyboard_paging_path() {
    // One wheel notch = one PageUp/PageDown: the wheel synthesizes the
    // same key event, so both inputs share one selection semantics
    // (same PAGE_STEP, same clamps). Scroll down pages forward by
    // PAGE_STEP rows; scrolling back up returns to the start.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let start = app.selected;
    let expected = start
        .saturating_add(PAGE_STEP)
        .min(app.visual_row_count().saturating_sub(1));
    let outcome = drive(
        &mut app,
        vec![
            scroll(MouseEventKind::ScrollDown, KeyModifiers::NONE),
            key(
                ratatui::crossterm::event::KeyCode::Char('q'),
                KeyEventKind::Press,
            ),
        ],
    );
    assert!(outcome.is_ok());
    assert_eq!(app.shell.selected, expected, "one notch pages forward");
    let outcome = drive(
        &mut app,
        vec![
            scroll(MouseEventKind::ScrollUp, KeyModifiers::NONE),
            key(
                ratatui::crossterm::event::KeyCode::Char('q'),
                KeyEventKind::Press,
            ),
        ],
    );
    assert!(outcome.is_ok());
    assert_eq!(
        app.shell.selected, start,
        "scrolling back up returns to the entry anchor"
    );
}

#[test]
fn wheel_and_keyboard_paging_prove_the_identical_outcome() {
    // The equivalence contract itself: a wheel notch and a PageDown key
    // starting from the same state must land on the same row.
    let mut wheeled = crate::demo_app();
    let _ = wheeled.apply_action(AppAction::SelectPage(AppPage::Applications));
    // A second demo app: the fixture is deterministic, so both start
    // from identical state.
    let mut keyed = crate::demo_app();
    let _ = keyed.apply_action(AppAction::SelectPage(AppPage::Applications));
    let reaction_wheel = apply_terminal_event(
        &mut wheeled,
        scroll(MouseEventKind::ScrollDown, KeyModifiers::NONE),
        TEST_FRAME,
    );
    let reaction_key = apply_terminal_event(
        &mut keyed,
        key(
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyEventKind::Press,
        ),
        TEST_FRAME,
    );
    assert_eq!(wheeled.shell.selected, keyed.shell.selected);
    assert_eq!(reaction_wheel.dirty, reaction_key.dirty);
}

#[test]
fn wheel_while_help_is_open_scrolls_help_and_never_the_table() {
    // The help overlay owns PageUp/PageDown while open, so the wheel —
    // routed through the same path — scrolls the binding list instead
    // of moving the table cursor.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let selected_before = app.selected;
    let outcome = drive(
        &mut app,
        vec![
            key(
                ratatui::crossterm::event::KeyCode::Char('?'),
                KeyEventKind::Press,
            ),
            scroll(MouseEventKind::ScrollDown, KeyModifiers::NONE),
            key(ratatui::crossterm::event::KeyCode::Esc, KeyEventKind::Press),
            key(
                ratatui::crossterm::event::KeyCode::Char('q'),
                KeyEventKind::Press,
            ),
        ],
    );
    assert!(outcome.is_ok());
    assert_eq!(
        app.shell.selected, selected_before,
        "the wheel must page the help list, never the table under it"
    );
}

#[test]
fn paste_lands_only_in_the_focused_search_field() {
    // The read side of the OSC 52 clipboard loop: paste is the search
    // box's bulk input path — sanitized and bounded by the shared shell
    // vocabulary, and an honest no-op anywhere else (never an implicit
    // search-open).
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    // Paste with the field closed: no state change, no repaint.
    let before = app.shell.query.clone();
    assert_eq!(
        apply_terminal_event(&mut app, Event::Paste("firefox".into()), TEST_FRAME),
        EventReaction::default()
    );
    assert_eq!(app.shell.query, before);
    assert!(!app.shell.search_active());
    // Open the field (Ctrl+F), then paste a multi-line block: the paste
    // must land flattened (event-level assertion).
    let ctrl_f = Event::Key(KeyEvent::new_with_kind(
        ratatui::crossterm::event::KeyCode::Char('f'),
        KeyModifiers::CONTROL,
        KeyEventKind::Press,
    ));
    assert!(apply_terminal_event(&mut app, ctrl_f.clone(), TEST_FRAME).dirty);
    let pasted = apply_terminal_event(&mut app, Event::Paste("fire\nfox 123".into()), TEST_FRAME);
    assert!(pasted.dirty, "a landed paste forces a repaint");
    assert_eq!(
        app.shell.query, "fire fox 123",
        "the pasted block must be flattened into the single-line query"
    );
    // Loop-level plumbing: the same sequence rides the full run loop —
    // open, paste, first Esc clears the non-empty query, second Esc
    // closes the field, q quits cleanly.
    let outcome = drive(
        &mut app,
        vec![
            ctrl_f,
            Event::Paste("fire\nfox 123".into()),
            key(ratatui::crossterm::event::KeyCode::Esc, KeyEventKind::Press),
            key(ratatui::crossterm::event::KeyCode::Esc, KeyEventKind::Press),
            key(
                ratatui::crossterm::event::KeyCode::Char('q'),
                KeyEventKind::Press,
            ),
        ],
    );
    assert!(outcome.is_ok());
    assert!(
        !app.shell.search_active(),
        "the second Esc must close the field"
    );
    assert_eq!(
        app.shell.query, "",
        "the first Esc cleared the pasted query"
    );
}

fn click(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

/// Render the CURRENT frame through the real `render` and return the
/// absolute row of the `› ` highlight (the painted selection marker).
fn painted_highlight_row(app: &TuiApp, width: u16, height: u16) -> Option<u16> {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::render(frame, app, crate::TuiTheme::default()))
        .expect("draw");
    for (row, line) in terminal.backend().to_string().lines().enumerate() {
        if line.contains("› ") {
            return u16::try_from(row).ok();
        }
    }
    None
}

#[test]
fn click_selects_the_painted_row_through_the_shared_selection_entry() {
    // The alignment lock: for every clickable page and several cursor
    // positions, clicking the row the renderer PAINTS the `› ` highlight
    // on must select exactly the current selection (self-click), and
    // clicking two rows below must advance the selection two rows — the
    // keyboard's own semantics through the same entry point.
    for page in [
        AppPage::Applications,
        AppPage::Services,
        AppPage::Startup,
        AppPage::Users,
    ] {
        for step in [0usize, 7] {
            let mut app = crate::demo_app();
            let _ = app.apply_action(AppAction::SelectPage(page));
            for _ in 0..step {
                app.shell.move_selection(1);
            }
            let frame = Rect::new(0, 0, 120, 40);
            let panel =
                crate::ui::table_hit::table_hit_support::table_panel_projection(&app, frame)
                    .unwrap_or_else(|| panic!("{page:?} must expose a table panel"));
            let column = panel.area.x + 2;
            let painted = painted_highlight_row(&app, 120, 40)
                .unwrap_or_else(|| panic!("{page:?} must paint a highlight"));
            let clicked =
                crate::ui::table_hit::table_hit_support::row_at(&app, frame, column, painted);
            assert_eq!(
                clicked,
                Some(app.shell.selected),
                "{page:?} at step {step}: the hit-test must address the painted highlight row"
            );
            // Self-click keeps the selection (and repaints); clicking two
            // data rows down moves the selection exactly two rows.
            let reaction = apply_terminal_event(&mut app, click(column, painted), frame);
            assert!(reaction.dirty, "{page:?}: a landed click repaints");
            let target_global = app.shell.selected + 2;
            if target_global < panel.total {
                let reaction = apply_terminal_event(&mut app, click(column, painted + 2), frame);
                assert!(reaction.dirty);
                assert_eq!(
                    app.shell.selected, target_global,
                    "{page:?}: click must select the projected global row"
                );
            }
        }
    }
}

#[test]
fn committed_frame_plan_rejects_a_click_after_page_change_before_redraw() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let plan = TuiFramePlan::build(&app, TEST_FRAME);
    let panel = plan.table_panel().expect("Applications has a table");

    // The key changed the app state, but the terminal has not painted the new
    // Services page yet. A coordinate from the old Applications frame must be
    // ignored rather than retargeting an unseen Services row.
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let reaction =
        apply_terminal_event_with_plan(&mut app, click(panel.area.x + 2, panel.area.y + 3), &plan);
    assert_eq!(reaction, EventReaction::default());
    assert_eq!(app.page(), AppPage::Services);
}

/// The pages without pointer-addressable rows project to None.
#[test]
fn pages_without_a_keyboard_addressable_table_are_click_transparent() {
    let frame = Rect::new(0, 0, 120, 40);
    for page in [AppPage::Performance, AppPage::System, AppPage::AppHistory] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(page));
        assert!(
            crate::ui::table_hit::table_hit_support::table_panel_projection(&app, frame).is_none(),
            "{page:?} must not expose pointer row selection"
        );
        let reaction = apply_terminal_event(&mut app, click(10, 10), frame);
        assert_eq!(reaction, EventReaction::default());
    }
    // Applications exposes its canonical category-tree projection to the
    // same pointer mapper used by keyboard navigation.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(crate::ui::table_hit::table_hit_support::table_panel_projection(&app, frame).is_some());
}

#[test]
fn clicks_on_headers_borders_and_outside_the_panel_are_no_ops() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    let frame = Rect::new(0, 0, 120, 40);
    let panel = crate::ui::table_hit::table_hit_support::table_panel_projection(&app, frame)
        .expect("users");
    let before = app.shell.selected;
    // Header row, header margin, top border, footer, and a column far
    // right of the panel never select.
    for (column, row) in [
        (10, panel.area.y + 1),
        (10, panel.area.y + 2),
        (10, panel.area.y),
        (10, frame.height - 1),
        (frame.width - 1, panel.area.y + 5),
    ] {
        assert_eq!(
            apply_terminal_event(&mut app, click(column, row), frame),
            EventReaction::default(),
            "click at ({column},{row}) must not select"
        );
    }
    assert_eq!(app.shell.selected, before);
}

#[test]
fn clicking_the_scrim_cancels_an_armed_confirmation_but_popup_chrome_does_not() {
    // A confirmation gate owns the keyboard (y/n); a click on its popup chrome
    // stays a blocked no-op, while a click outside the popup is the scrim
    // gesture and cancels through the same Escape path.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    let mut sessions = app.projection().sessions.clone().expect("demo sessions");
    for (index, session) in sessions.iter_mut().enumerate() {
        session.incarnation = Some(SessionIncarnation::LinuxLogind {
            boot_id: "fixture-boot".into(),
            created_monotonic_usec: u64::try_from(index + 1).unwrap_or(1),
        });
    }
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Sessions(Some(sessions)));
    let session = app
        .projection()
        .sessions
        .as_ref()
        .and_then(|sessions| sessions.first())
        .cloned()
        .expect("demo sessions");
    assert!(
        app.shell
            .select_session_control(&session, SessionControlAction::Lock)
    );
    let frame = Rect::new(0, 0, 120, 40);
    let popup = TuiFramePlan::build(&app, frame)
        .overlay()
        .expect("confirmation overlay")
        .popup;
    assert_eq!(
        apply_terminal_event(&mut app, click(popup.x, popup.y), frame),
        EventReaction::default(),
        "a click on the popup chrome must not cancel the gate"
    );
    assert!(app.shell.pending_session().is_some());

    let reaction = apply_terminal_event(&mut app, click(0, 0), frame);
    assert!(
        reaction.dirty,
        "the scrim click repaints the closed surface"
    );
    assert!(
        app.shell.pending_session().is_none(),
        "a click outside the popup must cancel the armed gate"
    );
}

#[test]
fn unsupported_pointer_events_change_nothing_and_never_flag_dirty() {
    // Modified scrolls, horizontal scrolls, non-left clicks, and drag are not
    // modeled: they neither change state nor force a repaint. This pins the
    // exact contract future pointer input work replaces
    // (left-click-to-select and bracketed paste now ARE modeled — their
    // dedicated tests own those contracts).
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    for unmodeled in [
        scroll(MouseEventKind::ScrollDown, KeyModifiers::CONTROL),
        scroll(MouseEventKind::ScrollUp, KeyModifiers::SHIFT),
        scroll(MouseEventKind::ScrollLeft, KeyModifiers::NONE),
        scroll(MouseEventKind::ScrollRight, KeyModifiers::NONE),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(ratatui::crossterm::event::MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }),
    ] {
        let before = app.shell.selected;
        assert_eq!(
            apply_terminal_event(&mut app, unmodeled.clone(), TEST_FRAME),
            EventReaction::default(),
            "unmodeled event {unmodeled:?} must be a no-op"
        );
        assert_eq!(app.shell.selected, before);
    }
}

#[test]
fn focus_loss_releases_control_hold_and_both_focus_edges_repaint() {
    let mut app = crate::demo_app();
    app.shell.set_control_held(true);
    assert!(app.shell.control_held());
    assert!(app.paused(), "Ctrl hold pauses telemetry before focus loss");

    assert_eq!(
        apply_terminal_event(&mut app, Event::FocusLost, TEST_FRAME),
        EventReaction {
            dirty: true,
            effect: None
        }
    );
    assert!(!app.shell.control_held());
    assert!(
        !app.paused(),
        "switching terminals cannot strand hold-to-pause"
    );
    assert_eq!(
        apply_terminal_event(&mut app, Event::FocusGained, TEST_FRAME),
        EventReaction {
            dirty: true,
            effect: None
        }
    );
}

#[test]
fn resize_and_release_key_events_flag_a_repaint_without_effects() {
    let mut app = crate::demo_app();
    let reaction = apply_terminal_event(&mut app, Event::Resize(100, 30), TEST_FRAME);
    assert_eq!(
        reaction,
        EventReaction {
            dirty: true,
            effect: None
        }
    );
    let reaction = apply_terminal_event(
        &mut app,
        key(
            ratatui::crossterm::event::KeyCode::Down,
            KeyEventKind::Release,
        ),
        TEST_FRAME,
    );
    assert_eq!(
        reaction,
        EventReaction {
            dirty: true,
            effect: None
        }
    );
}

#[test]
fn effects_from_keys_are_suppressed_without_a_platform_and_say_so() {
    // Demo-mode suppression is the loop's job, not the key handler's:
    // arming end-task and confirming 'y' produces a real effect, which
    // the loop must swallow with an honest status when no platform
    // client exists.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let outcome = drive(
        &mut app,
        vec![
            key(
                ratatui::crossterm::event::KeyCode::Delete,
                KeyEventKind::Press,
            ),
            key(
                ratatui::crossterm::event::KeyCode::Char('y'),
                KeyEventKind::Press,
            ),
            key(
                ratatui::crossterm::event::KeyCode::Char('q'),
                KeyEventKind::Press,
            ),
        ],
    );
    assert!(outcome.is_ok());
    assert!(
        app.feedback_text().contains("suppress"),
        "the loop must record the suppression honestly, got: {}",
        app.feedback_text()
    );
}

#[test]
fn refresh_pacing_only_owns_visible_gpu_engine_rows() {
    let app = crate::demo_app();
    let start = Instant::now();
    let pacing = RefreshPacing::starting(start);
    assert!(!pacing.due(&app, start + Duration::from_secs(60)));

    let mut gpu_app = crate::demo_app();
    let _ = gpu_app
        .shell
        .begin_gpu_engine_rows_request(DeviceId::new("gpu:0"));
    gpu_app.perf_device = crate::PerfDevice::Gpu;
    assert_eq!(
        gpu_app.apply_action(AppAction::SelectPage(AppPage::Performance)),
        None
    );
    assert!(!pacing.due(&gpu_app, start + Duration::from_secs(2)));
    assert!(pacing.due(&gpu_app, start + Duration::from_millis(2500)));
}

/// A scripted source that also counts how many BLOCKING polls (timeout >
/// zero — one per loop cycle) and how many reads the loop performed, so a
/// test can observe the per-cycle batching without a live terminal. The
/// counters are shared handles because the loop consumes the source by
/// value; single-threaded by construction, so `Rc<Cell<_>>` suffices.
struct CountingEventSource {
    items: VecDeque<io::Result<Event>>,
    blocking_polls: std::rc::Rc<std::cell::Cell<usize>>,
    reads: std::rc::Rc<std::cell::Cell<usize>>,
}

impl TerminalEventSource for CountingEventSource {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        if timeout > Duration::ZERO {
            self.blocking_polls.set(self.blocking_polls.get() + 1);
        }
        Ok(self.items.front().is_some())
    }

    fn read(&mut self) -> io::Result<Event> {
        self.reads.set(self.reads.get() + 1);
        match self.items.pop_front() {
            Some(Ok(event)) => Ok(event),
            Some(Err(error)) => Err(error),
            None => Err(io::Error::other("read on an empty script")),
        }
    }
}

/// The pump drains a ready backlog in bounded batches: a 20-key burst plus
/// the quit key is fully applied (21 reads — no event is dropped) but spans
/// exactly two loop cycles (two blocking polls) under the 16-event batch
/// cap. One-read-per-cycle would need 21 cycles; an unbounded drain would
/// collapse to 1 — both regressions fail this test. The quit key also ends
/// its batch immediately, so no event is applied after the quit request.
#[test]
fn event_bursts_drain_in_bounded_batches_per_cycle() {
    use std::rc::Rc;

    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let mut script = Vec::new();
    for _ in 0..20 {
        script.push(key(
            ratatui::crossterm::event::KeyCode::Down,
            KeyEventKind::Press,
        ));
    }
    script.push(key(
        ratatui::crossterm::event::KeyCode::Char('q'),
        KeyEventKind::Press,
    ));

    let blocking_polls = Rc::new(std::cell::Cell::new(0usize));
    let reads = Rc::new(std::cell::Cell::new(0usize));
    let source = CountingEventSource {
        items: script.into_iter().map(Ok).collect(),
        blocking_polls: blocking_polls.clone(),
        reads: reads.clone(),
    };
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
    let outcome = run_event_loop(&mut terminal, &mut app, None, source, false, None);

    assert!(outcome.is_ok());
    assert!(app.should_quit(), "the quit key must have been applied");
    assert_eq!(
        reads.get(),
        21,
        "every scripted event must be read exactly once"
    );
    assert_eq!(
        blocking_polls.get(),
        2,
        "21 events under a 16-per-cycle cap must span exactly two cycles"
    );
}

#[test]
fn seam_event_loop_expires_timed_feedback_notice() {
    let mut app = crate::demo_app();
    app.report_notice(
        FeedbackSource::Interaction,
        FeedbackSeverity::Info,
        FeedbackLifecycle::Timed(Duration::from_millis(40)),
        "Expiring in loop",
    );
    assert!(app.shell.feedback_notice().is_some());
    let outcome = drive(
        &mut app,
        vec![key(
            ratatui::crossterm::event::KeyCode::Char('q'),
            KeyEventKind::Press,
        )],
    );
    assert!(outcome.is_ok());
    assert!(app.shell.feedback_notice().is_none());
}

#[test]
fn seam_event_loop_esc_dismisses_feedback_notice() {
    let mut app = crate::demo_app();
    app.report_notice(
        FeedbackSource::Interaction,
        FeedbackSeverity::Info,
        FeedbackLifecycle::UntilReplaced,
        "Notice to dismiss in loop",
    );
    assert!(app.shell.feedback_notice().is_some());
    let outcome = drive(
        &mut app,
        vec![
            key(ratatui::crossterm::event::KeyCode::Esc, KeyEventKind::Press),
            key(
                ratatui::crossterm::event::KeyCode::Char('q'),
                KeyEventKind::Press,
            ),
        ],
    );
    assert!(outcome.is_ok());
    assert!(app.shell.feedback_notice().is_none());
}

/// The supervised capture path writes its typed frame identity once demo data
/// is ready: driving the production loop in demo mode with a marker path
/// records `demo_data_ready` plus the live page the frame shows. This is the
/// TUI's `mc07-tui-capture-visual` anchor, which proves the frame-marker
/// contract separately from terminal-profile matrix receipts.
#[test]
fn capture_marker_records_the_typed_demo_frame_identity() {
    let marker = crate::ui::test_support::repo_temp_dir()
        .join(format!("tui-capture-marker-{}", std::process::id()));
    let mut app = crate::demo_app();
    // Pin a non-default page so the assertion proves the marker reads the
    // live page instead of a constant.
    app.application.active_page = AppPage::Applications;
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
    let outcome = run_event_loop(
        &mut terminal,
        &mut app,
        None,
        ScriptedEventSource::new(vec![key(
            ratatui::crossterm::event::KeyCode::Char('q'),
            KeyEventKind::Press,
        )]),
        true,
        Some(marker.as_os_str()),
    );
    assert!(outcome.is_ok());
    assert!(app.should_quit(), "the quit key must have ended the loop");

    let text = std::fs::read_to_string(&marker).expect("capture marker file");
    std::fs::remove_file(&marker).expect("remove capture marker scratch");
    assert!(
        text.contains("TUI_CAPTURE_MARKER event=demo_data_ready mode=demo"),
        "marker must name the demo data readiness: {text:?}"
    );
    assert!(
        text.contains("TUI_CAPTURE_MARKER event=frame_ready page=applications"),
        "marker must carry the live page the captured frame shows: {text:?}"
    );
}

#[test]
fn settings_pointer_rows_focus_and_flip_only_switch_fields() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let frame = Rect::new(0, 0, 120, 40);
    let mut app = crate::demo_app();
    app.toggle_settings();

    // High contrast (field 2) is a switch: the first click focuses and flips
    // it, the second click flips it back.
    let hc_before = app.settings_form.hc;
    for expected in [!hc_before, hc_before] {
        let plan = TuiFramePlan::build(&app, frame);
        let overlay = plan.overlay().expect("settings overlay");
        let layout = overlay.settings.expect("settings field rows");
        let row = (overlay.popup.y..overlay.popup.bottom())
            .find(|row| layout.field_at(*row) == Some(2))
            .expect("the high-contrast row is painted");
        let reaction =
            apply_terminal_event_with_plan(&mut app, click(overlay.popup.x + 2, row), &plan);
        assert!(reaction.dirty, "a settings row click repaints the form");
        assert_eq!(app.settings_form.field, 2, "the clicked row takes focus");
        assert_eq!(
            app.settings_form.hc, expected,
            "each click on a switch row flips it exactly once"
        );
    }

    // A non-switch row only takes focus; its value still steps with
    // Left/Right rather than changing on the click.
    let skin_before = app.settings_form.skin;
    let plan = TuiFramePlan::build(&app, frame);
    let overlay = plan.overlay().expect("settings overlay");
    let layout = overlay.settings.expect("settings field rows");
    let row = (overlay.popup.y..overlay.popup.bottom())
        .find(|row| layout.field_at(*row) == Some(0))
        .expect("the skin row is painted");
    let reaction = apply_terminal_event_with_plan(&mut app, click(overlay.popup.x + 2, row), &plan);
    assert!(reaction.dirty);
    assert_eq!(app.settings_form.field, 0);
    assert_eq!(
        app.settings_form.skin, skin_before,
        "a choice row takes focus without changing its value"
    );

    // The popup border above the same field row, and the footer band, stay
    // blocked overlay cells: neither may address a field.
    let plan = TuiFramePlan::build(&app, frame);
    let overlay = plan.overlay().expect("settings overlay");
    let layout = overlay.settings.expect("settings field rows");
    let row = (overlay.popup.y..overlay.popup.bottom())
        .find(|row| layout.field_at(*row) == Some(2))
        .expect("the high-contrast row is painted");
    assert_eq!(
        plan.hit_target(overlay.popup.x, row),
        Some(crate::ui::TuiHitTarget::Overlay {
            scope: crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::Settings),
        }),
        "the popup border never resolves to a form field"
    );
    let blocked = apply_terminal_event_with_plan(
        &mut app,
        click(overlay.popup.x + 2, layout.body.bottom()),
        &plan,
    );
    assert_eq!(
        blocked,
        EventReaction::default(),
        "footer cells are a blocked overlay no-op"
    );
    assert_eq!(app.settings_form.field, 0);
}

#[test]
fn search_field_pointer_click_opens_focus_and_places_the_caret() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let frame = Rect::new(0, 0, 120, 40);
    let plan = TuiFramePlan::build(&app, frame);
    let search = plan.search_field().expect("the search field is painted");

    // The field's border row is inert; the content row opens search.
    assert!(plan.hit_target(search.text_x, search.row - 1).is_none());
    let reaction =
        apply_terminal_event_with_plan(&mut app, click(search.text_x + 1, search.row), &plan);
    assert!(reaction.dirty);
    assert!(app.search_active(), "clicking the field opens search");
    assert_eq!(app.query_cursor(), 0, "an empty query clamps the caret");

    app.push_search_text("dbus-broker");
    assert_eq!(app.query_cursor(), 11);
    let plan = TuiFramePlan::build(&app, frame);
    let search = plan.search_field().expect("the search field is painted");
    let reaction =
        apply_terminal_event_with_plan(&mut app, click(search.text_x + 5, search.row), &plan);
    assert!(reaction.dirty);
    assert_eq!(
        app.query_cursor(),
        5,
        "clicking the '-' cell places the caret before it"
    );
    app.push_search_char('X');
    assert_eq!(app.query, "dbus-Xbroker");
    assert_eq!(app.query_cursor(), 6);

    // A cell past the text lands at the end.
    let plan = TuiFramePlan::build(&app, frame);
    let search = plan.search_field().expect("the search field is painted");
    let _ = apply_terminal_event_with_plan(&mut app, click(search.text_x + 30, search.row), &plan);
    assert_eq!(app.query_cursor(), app.query.len());

    // Wide characters resolve to the boundary their painted cell belongs to.
    let mut wide = crate::demo_app();
    let _ = wide.apply_action(AppAction::SelectPage(AppPage::Applications));
    wide.open_search();
    wide.push_search_text("éa");
    let plan = TuiFramePlan::build(&wide, frame);
    let search = plan.search_field().expect("the search field is painted");
    let _ = apply_terminal_event_with_plan(&mut wide, click(search.text_x + 1, search.row), &plan);
    assert_eq!(
        wide.query_cursor(),
        2,
        "the caret lands on the UTF-8 boundary before 'a'"
    );
}

#[test]
fn services_search_field_pointer_click_opens_its_own_slot_and_places_the_caret() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    app.shell.set_services_query("Network");
    let frame = Rect::new(0, 0, 120, 40);
    let plan = TuiFramePlan::build(&app, frame);
    let search = plan
        .search_field()
        .expect("the Services search field is painted");
    assert_eq!(
        plan.hit_target(search.text_x, search.row - 1),
        None,
        "the field border is inert"
    );

    let reaction =
        apply_terminal_event_with_plan(&mut app, click(search.text_x + 3, search.row), &plan);
    assert!(reaction.dirty);
    assert!(app.search_active(), "clicking the field opens search");
    assert_eq!(app.shell.search_slot(), SearchSlot::Services);
    assert_eq!(
        app.shell.services_query_cursor(),
        3,
        "the caret lands before the 'w' cell"
    );
    assert_eq!(app.query, "", "the process query is untouched");

    app.push_search_char('X');
    assert_eq!(app.shell.services_query, "NetXwork");
    assert_eq!(app.query, "");
}

#[test]
fn saved_views_bracketed_paste_is_owned_only_after_native_import_key() {
    use ratatui::crossterm::event::KeyCode;
    use taskmanager_shell::saved_views::export_saved_views_json;
    let mut app = crate::demo_app();
    app.toggle_settings();
    super::handle_key(&mut app, KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
    super::handle_key(&mut app, KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    let json = export_saved_views_json(&app.saved_views.rows).expect("payload");
    apply_terminal_event(&mut app, Event::Paste(json.clone()), TEST_FRAME);
    assert_eq!(app.saved_views.rows.len(), 4);
    super::handle_key(&mut app, KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE));
    apply_terminal_event(&mut app, Event::Paste(json), TEST_FRAME);
    assert_eq!(app.saved_views.rows.len(), 5);
    assert!(
        app.shell.query.is_empty(),
        "saved-view paste cannot become a process search"
    );
}

#[test]
fn native_navigation_key_and_rail_click_share_the_painted_page_order_and_protect_edges() {
    use ratatui::crossterm::event::{KeyCode, MouseEvent};
    use taskmanager_ui_contract::navigation::NavOrientation;
    let mut app = crate::demo_app();
    super::handle_key(&mut app, KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE));
    assert_eq!(app.nav_orientation, NavOrientation::Vertical);
    for (width, height) in [
        (54, 16),
        (80, 24),
        (120, 36),
        (180, 20),
        (54, 50),
        (200, 60),
    ] {
        let area = Rect::new(0, 0, width, height);
        let plan = TuiFramePlan::build(&app, area);
        let (rail_x, rail_y) = (0..height)
            .flat_map(|row| (0..width).map(move |column| (column, row)))
            .find(|&(column, row)| {
                plan.hit_target(column, row)
                    == Some(crate::ui::TuiHitTarget::NavigationPage(
                        AppPage::Performance,
                    ))
            })
            .expect("painted rail target");
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| {
                crate::ui::render_with_plan(frame, &app, crate::TuiTheme::default(), &plan)
            })
            .expect("paint");
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("F7"), "normal visible toggle hint");
        for (index, page) in AppPage::ALL.into_iter().enumerate() {
            let reaction = apply_terminal_event_with_plan(
                &mut app,
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rail_x,
                    row: rail_y + index as u16,
                    modifiers: KeyModifiers::NONE,
                }),
                &plan,
            );
            assert!(reaction.dirty);
            assert_eq!(app.page(), page);
        }
    }
    app.toggle_settings();
    super::handle_key(&mut app, KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE));
    assert_eq!(
        app.nav_orientation,
        NavOrientation::Vertical,
        "modal owns the key"
    );
    app.close_local_overlays();
    super::handle_key(&mut app, KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE));
    assert_eq!(app.nav_orientation, NavOrientation::Horizontal);
}

#[test]
fn daemon_telemetry_stream_injection_and_drain() {
    let mut app = TuiApp::new();
    assert_eq!(app.last_daemon_telemetry_ms, 0);

    let (tx, rx) = crossbeam_channel::bounded(16);
    app.daemon_rx = Some(rx);

    let batch = DaemonMessage::TelemetryBatch {
        timestamp_ms: 123456789,
        facet_mask: SubscriptionMask::CPU.bits(),
        payload_json:
            "{\"cpu_usage_pct\":25.0,\"memory_used_bytes\":4000000,\"memory_total_bytes\":8000000}"
                .to_string(),
    };
    tx.send(batch).unwrap();

    let drained = app.drain_daemon_telemetry();
    assert!(drained);
    assert_eq!(app.last_daemon_telemetry_ms, 123456789);

    let snap = app.shell.projection().snapshot.as_ref().unwrap();
    assert_eq!(snap.timestamp_ms, 123456789);
    assert_eq!(snap.cpu.current_global_usage_pct(), Some(25.0));
    assert_eq!(snap.memory.current_used_bytes(), Some(4000000));
    assert_eq!(snap.memory.current_total_bytes(), Some(8000000));

    // Second drain when channel is empty returns false
    assert!(!app.drain_daemon_telemetry());
}

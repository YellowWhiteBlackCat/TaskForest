//! The terminal runtime seam (P0 of the TUI refinement line): everything
//! between "a real tty" and "the deterministic core" lives behind two
//! injectable abstractions so the whole run loop can be driven headlessly.
//!
//! - [`TerminalEventSource`] abstracts crossterm's global event queue.
//!   Production reads the real tty; tests script a deterministic source
//!   (and a future remote transport can substitute its own normalized
//!   events without touching the loop). A failing source surfaces as a
//!   typed loop error — the loop can never hang on a dead transport.
//! - [`RefreshPacing`] owns the one remaining page-local GPU engine watermark
//!   with an injectable clock; all general capability cadence belongs to the
//!   runtime ECS scheduler.
//!
//! [`apply_terminal_event`] is where the input line grows: the bare scroll
//! wheel already lands there (mapped onto the SAME PageUp/PageDown path the
//! keyboard uses, so there is exactly one selection semantics), and
//! bracketed paste lands there as the search box's bulk input path (the read
//! side of the OSC 52 clipboard loop). Focus events will join it. Keeping
//! every event class at this seam keeps the run loop's dirty/effect
//! bookkeeping single-sourced.

use std::ffi::OsStr;
use std::io;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::{Event, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::layout::Rect;

#[cfg(test)]
#[path = "../../tests/headless/runtime/seam_support.rs"]
pub(crate) mod seam_support;
use taskmanager_application::{PlatformClient, PlatformEffect};

use crate::ui::{TuiFramePlan, TuiHitTarget, render_with_plan};
use crate::{TuiApp, TuiTerminalProfile, TuiTheme};

use super::{
    DrawCycleInputs, EVENT_DRAIN_BATCH, EVENT_POLL, capture_page_name, drain_process_refresh,
    handle_key, should_draw, submit_alert_notifications, unix_now_ms,
};

/// The terminal event source seam. Production uses [`CrosstermEventSource`];
/// tests (and later remote transports) script their own. `pub(crate)` so the
/// crate's registered test modules can script deterministic sources against
/// the real production loop without widening the public API.
pub(crate) trait TerminalEventSource {
    /// Whether an event is ready within `timeout`. Returning `Ok(false)`
    /// means "idle tick" — the loop re-evaluates refresh pacing and draws
    /// only if the cycle was dirty.
    fn poll(&mut self, timeout: Duration) -> io::Result<bool>;
    /// Read the ready event. Only called after `poll` returned `true`.
    fn read(&mut self) -> io::Result<Event>;
}

/// Production source over crossterm's global event queue.
pub(super) struct CrosstermEventSource;

impl TerminalEventSource for CrosstermEventSource {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        ratatui::crossterm::event::poll(timeout)
    }

    fn read(&mut self) -> io::Result<Event> {
        ratatui::crossterm::event::read()
    }
}

/// Convert a terminal backend's draw error into the loop's `io::Error`.
/// CrosstermBackend errors are io errors already; TestBackend's error type
/// is `Infallible` (it cannot fail), so its arm is a never-match. Only these
/// two impls exist on purpose: a future backend with a custom error type
/// fails to compile here instead of panicking or stringifying at runtime.
/// `pub(crate)` matches the test-only `run_event_loop` bound that names it.
pub(crate) trait BackendErrorIntoIo {
    fn into_io(self) -> io::Error;
}

impl BackendErrorIntoIo for io::Error {
    fn into_io(self) -> io::Error {
        self
    }
}

impl BackendErrorIntoIo for std::convert::Infallible {
    fn into_io(self) -> io::Error {
        match self {}
    }
}

/// One terminal event's outcome, decided at the seam between crossterm
/// normalization and the run loop's dirty/effect bookkeeping. Keeping this a
/// value (not side effects on the loop) is what makes every event class
/// unit-testable without a live terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EventReaction {
    /// The event forces a repaint on the next cycle (any key event or a
    /// resize — the poll runs after the draw decision, so the signal has to
    /// survive into the next cycle).
    pub(crate) dirty: bool,
    /// A platform effect produced by the event. The caller queues it through
    /// the normal effect seam (with demo-mode suppression).
    pub(crate) effect: Option<PlatformEffect>,
}

/// Normalize ONE terminal event into app state changes. Key events mirror
/// the live Ctrl state into the shared telemetry policy (GPUI/Iced
/// hold-Ctrl-pause parity; a terminal without release events can never
/// strand the pause because the next press re-syncs) and route presses and
/// repeats through the shared key handler. A resize only flags a repaint
/// (ratatui's `Terminal::draw` re-queries the size itself). The BARE scroll
/// wheel is the terminal's PageUp/PageDown: it synthesizes the same key
/// event and routes it through the identical handler, so wheel paging and
/// keyboard paging can never diverge (same projection, same clamps, and
/// while the help overlay is open the wheel scrolls the help list exactly
/// like PageDown does). Modified scrolls and unsupported pointer gestures are
/// explicit no-ops. Focus loss releases the mirrored Ctrl hold so switching
/// terminals cannot strand telemetry in the hold-to-pause state.
/// Normalize one terminal event against the immutable plan for the frame the
/// user last saw. Pointer input must not rebuild geometry from an app state
/// that earlier events in the same burst have already changed.
pub(crate) fn apply_terminal_event_with_plan(
    app: &mut TuiApp,
    event: Event,
    plan: &TuiFramePlan,
) -> EventReaction {
    match event {
        Event::FocusGained => EventReaction {
            dirty: true,
            effect: None,
        },
        Event::FocusLost => {
            app.shell.set_control_held(false);
            EventReaction {
                dirty: true,
                effect: None,
            }
        }
        Event::Key(key) => {
            app.shell
                .set_control_held(key.modifiers.contains(KeyModifiers::CONTROL));
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                return EventReaction {
                    dirty: true,
                    effect: None,
                };
            }
            EventReaction {
                dirty: true,
                effect: handle_key(app, key),
            }
        }
        Event::Paste(text) => {
            // Bracketed paste is the search box's bulk input path (the read
            // side of the OSC 52 clipboard loop). Paste only lands while the
            // search field is focused — anywhere else it is an honest no-op,
            // not an implicit search-open.
            if app.search_active() && app.shell.push_search_text(&text) {
                return EventReaction {
                    dirty: true,
                    effect: None,
                };
            }
            EventReaction::default()
        }
        Event::Mouse(mouse) => {
            if mouse.modifiers != KeyModifiers::NONE {
                // Modified mouse events stay native (Shift bypasses mouse
                // reporting entirely in the common emulators): no state
                // change, no repaint.
                return EventReaction::default();
            }
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    let page_key = KeyEvent::new(
                        ratatui::crossterm::event::KeyCode::PageUp,
                        KeyModifiers::NONE,
                    );
                    EventReaction {
                        dirty: true,
                        effect: handle_key(app, page_key),
                    }
                }
                MouseEventKind::ScrollDown => {
                    let page_key = KeyEvent::new(
                        ratatui::crossterm::event::KeyCode::PageDown,
                        KeyModifiers::NONE,
                    );
                    EventReaction {
                        dirty: true,
                        effect: handle_key(app, page_key),
                    }
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    // Click resolution through the committed plan, in paint
                    // order: an actionable overlay control row first (the
                    // painted popup owns those cells), then every other popup
                    // cell as a blocked overlay hit, then the background
                    // table. Overlay clicks select through the SAME mutators
                    // and Enter methods the keyboard uses, so a pointer user
                    // can never reach an action the keyboard cannot, and the
                    // frozen-target/confirmation gates stay in the path.
                    match plan.hit_target(mouse.column, mouse.row) {
                        Some(TuiHitTarget::OverlayControl { surface, index }) => {
                            apply_overlay_control_click(app, plan, surface, index)
                        }
                        Some(TuiHitTarget::Overlay { .. }) => EventReaction::default(),
                        Some(TuiHitTarget::TableRow { page, index }) => {
                            // Click-to-select: the hit-test projects the
                            // clicked cell through the SAME visual row
                            // projection the renderer painted
                            // (`ui::table_hit`) and selects through the
                            // shell's `select_row` — the keyboard's own
                            // entry, with the keyboard's own boundary.
                            // Clicks that land on headers/borders/non-table
                            // pages are honest no-ops.
                            if super::modals::any_pointer_surface_open(app) {
                                return EventReaction::default();
                            }
                            if page != app.page() || !plan.page_matches(app.page()) {
                                // The screen has not painted the page selected
                                // by an earlier event in this burst yet, so a
                                // coordinate from the previous page cannot
                                // safely target it.
                                return EventReaction::default();
                            }
                            let selected = index;
                            if app.page() == taskmanager_application::AppPage::Applications {
                                let rows = app.process_rows_snapshot();
                                let process =
                                    crate::process_view::process_at(&rows, selected).cloned();
                                let row_key = crate::process_view::row_key_at(&rows, selected);
                                let _ = app.apply_selection_resolution_with_row(
                                    selected, process, row_key,
                                );
                                EventReaction {
                                    dirty: app.selected == selected,
                                    effect: None,
                                }
                            } else {
                                EventReaction {
                                    dirty: app.shell.select_row(selected),
                                    effect: None,
                                }
                            }
                        }
                        None => EventReaction::default(),
                    }
                }
                // Every remaining crossterm pointer variant is deliberately
                // unsupported: no wildcard may turn a gesture into selection
                // or a repaint loop.
                MouseEventKind::Down(MouseButton::Right | MouseButton::Middle)
                | MouseEventKind::Up(_)
                | MouseEventKind::Drag(_)
                | MouseEventKind::Moved
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight => EventReaction::default(),
            }
        }
        Event::Resize(_, _) => EventReaction {
            dirty: true,
            effect: None,
        },
    }
}

/// Apply one click on a committed plan's `OverlayControl` hit: set the named
/// surface's selection to the clicked row through the existing `pub(crate)`
/// mutator, then run the SAME production method the keyboard's Enter runs for
/// that surface. The pointer can therefore never bypass the frozen-target or
/// confirmation gates — destructive menu rows land in the shared y/n gate
/// exactly like keyboard picks.
///
/// Fail-closed against a stale committed plan: the surface the plan painted
/// must still own the keyboard (a surface switch since the last paint turns
/// the click into a no-op), and a filtered palette must still paint the same
/// row inventory the click addressed. The mutators re-check the surface
/// variant, so even a same-kind mismatch cannot misaddress state.
fn apply_overlay_control_click(
    app: &mut TuiApp,
    plan: &TuiFramePlan,
    surface: crate::TuiSurfaceKind,
    index: usize,
) -> EventReaction {
    if app.local_surface_kind() != Some(surface) {
        return EventReaction::default();
    }
    if surface == crate::TuiSurfaceKind::CommandPalette {
        let painted = plan
            .overlay_controls()
            .map_or(0, |controls| usize::from(controls.count));
        if painted != app.filtered_palette_rows().len() {
            return EventReaction::default();
        }
    }
    match surface {
        crate::TuiSurfaceKind::ServiceMenu => {
            if let Some(menu) = app.service_menu_mut() {
                menu.selection = index.min(crate::ui::service_menu::MENU_ACTIONS.len() - 1);
            }
            // Enter parity: arms the shared service-control confirmation; the
            // platform request is only emitted by the y key.
            app.service_menu_select();
            EventReaction {
                dirty: true,
                effect: None,
            }
        }
        crate::TuiSurfaceKind::ProcessMenu => {
            if let Some(menu) = app.process_menu_mut() {
                menu.selection = index.min(crate::ui::process_menu::MENU_ACTIONS.len() - 1);
            }
            EventReaction {
                dirty: true,
                effect: app.process_menu_select(),
            }
        }
        crate::TuiSurfaceKind::BatchMenu => {
            if let Some(menu) = app.batch_menu_mut() {
                menu.selection = index.min(crate::ui::batch_menu::MENU_ACTIONS.len() - 1);
            }
            EventReaction {
                dirty: true,
                effect: app.batch_menu_select(),
            }
        }
        crate::TuiSurfaceKind::SessionMenu => {
            if let Some(menu) = app.session_menu_mut() {
                menu.selection = index.min(crate::ui::session_menu::MENU_ACTIONS.len() - 1);
            }
            app.session_menu_select();
            EventReaction {
                dirty: true,
                effect: None,
            }
        }
        crate::TuiSurfaceKind::StartupMenu => {
            if let Some(menu) = app.startup_menu_mut() {
                menu.selection = index.min(crate::ui::startup_menu::MENU_ACTIONS.len() - 1);
            }
            app.startup_menu_select();
            EventReaction {
                dirty: true,
                effect: None,
            }
        }
        crate::TuiSurfaceKind::ColumnMenu => {
            if let Some(selection) = app.column_menu_selection_mut() {
                *selection = index.min(crate::TuiApp::toggleable_columns().len() - 1);
            }
            // Enter/Space parity: toggles the clicked column's hidden flag.
            app.column_menu_toggle();
            EventReaction {
                dirty: true,
                effect: None,
            }
        }
        crate::TuiSurfaceKind::CommandPalette => {
            if let Some(palette) = app.command_palette_mut() {
                palette.selection = index;
            }
            // Enter parity: runs the selected row's action and closes the
            // palette (shared action effects return through the normal seam).
            EventReaction {
                dirty: true,
                effect: app.palette_select(),
            }
        }
        // The informational surfaces paint no control rows; `hit_target`
        // cannot produce this variant for them, so the arm is a defensive
        // fail-closed no-op, not a modeled path.
        crate::TuiSurfaceKind::Settings
        | crate::TuiSurfaceKind::About
        | crate::TuiSurfaceKind::Health
        | crate::TuiSurfaceKind::Containers
        | crate::TuiSurfaceKind::ServiceDependencies
        | crate::TuiSurfaceKind::ProcessAffinity => EventReaction::default(),
    }
}

/// The one frontend-local refresh watermark. General capability cadence is
/// owned by the runtime ECS scheduler; this axis remains local because it is
/// conditional on the visible GPU performance page and its interactive
/// engine-row session.
pub(super) struct RefreshPacing {
    gpu_engine_rows: Instant,
}

impl RefreshPacing {
    pub(super) const fn starting(now: Instant) -> Self {
        Self {
            gpu_engine_rows: now,
        }
    }

    /// Whether the page-local GPU engine refresh is due as of `now`.
    pub(super) fn due(&self, app: &TuiApp, now: Instant) -> bool {
        matches!(
            app.shell.gpu_engine_rows_state(),
            taskmanager_application::GpuEngineRowsState::Loading { .. }
                | taskmanager_application::GpuEngineRowsState::Ready(_)
        ) && app.page() == taskmanager_application::AppPage::Performance
            && app.perf_device == crate::PerfDevice::Gpu
            && now.saturating_duration_since(self.gpu_engine_rows) >= super::GPU_ENGINE_ROWS_REFRESH
    }
}

/// The run loop, generic over the terminal backend and the event source so
/// the full cycle — drain, pacing, draw decision, event application, quit —
/// is drivable headlessly with a `TestBackend` and a scripted source.
/// Behavior is identical to the former inline `ratatui::run` closure.
/// `pub(crate)` (test-only) so the crate's registered test modules — not only
/// the runtime module tree — can drive the production loop with a counting
/// backend and a scripted event source.
/// Run the terminal loop with the capability profile resolved by the
/// composition edge.  The wrapper above keeps existing deterministic tests
/// on a stable Unicode/true-color profile without touching global env state.
pub(super) fn run_event_loop_with_profile<B: Backend, E: TerminalEventSource>(
    terminal: &mut Terminal<B>,
    app: &mut TuiApp,
    mut platform: Option<&mut PlatformClient>,
    mut events: E,
    demo: bool,
    capture_marker: Option<&OsStr>,
    terminal_profile: TuiTerminalProfile,
) -> io::Result<()>
where
    B::Error: BackendErrorIntoIo,
{
    let mut pacing = RefreshPacing::starting(Instant::now());
    let mut capture_marked = false;
    // The frame area the user currently SEES: captured from every paint and
    // used to project pointer clicks onto the painted rows (a resize updates
    // it on the next paint, which the resize event itself forces).
    let initial_area = {
        let size = terminal
            .backend()
            .size()
            .map_err(BackendErrorIntoIo::into_io)?;
        Rect::new(0, 0, size.width, size.height)
    };
    let mut frame_area = initial_area;
    let mut committed_plan = TuiFramePlan::build(app, initial_area);
    // Dirty-flag carry-over: the initial frame must always paint (otherwise
    // the screen stays blank until the first telemetry tick or keypress), and
    // a key/resize arrives in the poll phase — which runs *after* the draw
    // decision — so its "needs redraw" signal has to survive into the next
    // cycle. `pending_draw` carries both; the per-cycle drain/queue signals
    // live in [`DrawCycleInputs`] and are consulted by `should_draw`.
    let mut pending_draw = true;
    loop {
        // Per-cycle dirty signals. The drain/queue block below ORs the
        // individual flags in as it goes; `should_draw(cycle)` then decides
        // whether this cycle paints. The cross-cycle keypress/resize signal
        // lives in `pending_draw` (declared above the loop) because the
        // poll runs after the draw decision.
        let notice_before = app.shell.feedback_notice().is_some();
        app.shell
            .advance_feedback_time(std::time::Duration::from_millis(50));
        if notice_before && app.shell.feedback_notice().is_none() {
            pending_draw = true;
        }
        let mut cycle = DrawCycleInputs::default();
        cycle.ancillary_effect |= app.drain_config_publications();
        cycle.ancillary_effect |= app.drain_history_replay_completions();
        cycle.ancillary_effect |= app.drain_snapshot_export_completions();
        if let Some(platform) = platform.as_deref_mut() {
            cycle.ancillary_effect |= app
                .shell
                .apply_capability_snapshot(platform.capabilities().snapshot());
            match platform.try_drain() {
                Ok(batch) => {
                    // Only a non-empty batch carries new render state — an
                    // empty drain (the common idle case) leaves the screen
                    // unchanged and must not flag the cycle dirty.
                    let folded = !batch.is_empty();
                    app.apply_platform_batch(batch);
                    // Honor the post-control process-list refresh a
                    // completion requested (G-01 payoff): drain the
                    // shell's one-shot flag and submit the refresh through
                    // the same effect seam every refresh uses.
                    if drain_process_refresh(app, platform) {
                        cycle.refresh_queued = true;
                    }
                    let queued_alerts = submit_alert_notifications(&mut app.shell, platform);
                    cycle.platform_batch |= folded;
                    cycle.ancillary_effect |= queued_alerts;
                }
                Err(error) => app.report_event_port_error(error),
            }
            let now = Instant::now();
            if !app.shell.paused() {
                platform.set_telemetry_interval(app.shell.telemetry_interval());
                let scheduled = platform.run_scheduled_refresh(unix_now_ms());
                cycle.refresh_queued |= !scheduled.is_empty();
            }
            let gpu_engine_rows_due = pacing.due(app, now);
            if gpu_engine_rows_due {
                if let Some(device_id) = app.gpu_engine_rows_device_id() {
                    taskmanager_shell::queue_effect(
                        app,
                        platform,
                        taskmanager_shell::ShellApp::request_gpu_engine_rows(device_id),
                    );
                }
                cycle.refresh_queued = true;
            }
            if gpu_engine_rows_due {
                pacing.gpu_engine_rows = now;
            }
            // Per-process insights re-request (deduped on the frozen
            // identity) and the open service-log stream follow (shell
            // throttles it to 1 Hz; the wall clock lives here, not in the
            // shell or the renderer).
            if let Some(effect) = app.refresh_selected_process_insights() {
                taskmanager_shell::queue_effect(app, platform, effect);
                cycle.ancillary_effect = true;
            }
            if let Some(effect) = app.refresh_selected_service_dependencies() {
                taskmanager_shell::queue_effect(app, platform, effect);
                cycle.ancillary_effect = true;
            }
            let now_ms = unix_now_ms();
            app.service_log_now_micros = now_ms.saturating_mul(1_000);
            if let Some(effect) = app.shell.poll_service_log(now_ms) {
                taskmanager_shell::queue_effect(app, platform, effect);
                cycle.ancillary_effect = true;
            }
        }

        // Idle-skip: only paint when this cycle produced new data to show
        // (or the previous cycle queued a keypress/resize, or it's the
        // initial frame). Skipping `terminal.draw` on a pure idle cycle is
        // the whole point of the dirty flag — the TUI no longer repaints
        // at the fixed ~10 Hz poll cadence when nothing has changed.
        if pending_draw || should_draw(cycle) {
            // The terminal palette is rebuilt from the runtime theme
            // parameters on every paint, so a settings change re-skins the
            // TUI on the next draw.
            let theme = TuiTheme::from_params_with_profile(app.theme_params, terminal_profile);
            let mut painted_plan = None;
            terminal
                .draw(|frame| {
                    frame_area = frame.area();
                    let plan = TuiFramePlan::build(app, frame.area());
                    render_with_plan(frame, app, theme, &plan);
                    painted_plan = Some(plan);
                })
                .map_err(BackendErrorIntoIo::into_io)?;
            if let Some(plan) = painted_plan {
                committed_plan = plan;
            }
            pending_draw = false;
        }
        if demo
            && !capture_marked
            && let Some(path) = capture_marker
        {
            std::fs::write(
                path,
                format!(
                    "TUI_CAPTURE_MARKER event=demo_data_ready mode=demo\n\
             TUI_CAPTURE_MARKER event=frame_ready page={}\n",
                    capture_page_name(app.page())
                ),
            )?;
            capture_marked = true;
        }
        if events.poll(EVENT_POLL)? {
            // A keypress or resize always repaints on the next cycle (the
            // poll runs after the draw decision above). The confirmed-ready
            // event plus whatever else is already queued drain in one
            // bounded batch (`poll(0)` between reads): an event burst cannot
            // build a backlog behind per-cycle redraws, while the
            // [`EVENT_DRAIN_BATCH`] cap keeps the draw/quit checks live and
            // a quit key stops the batch immediately. The idle path is
            // unchanged — the blocking poll above still returns false on a
            // quiet terminal.
            for drained in 0..EVENT_DRAIN_BATCH {
                let reaction = apply_terminal_event_with_plan(app, events.read()?, &committed_plan);
                pending_draw |= reaction.dirty;
                if let Some(effect) = reaction.effect {
                    match platform.as_deref_mut() {
                        Some(platform) => taskmanager_shell::queue_effect(app, platform, effect),
                        None => app.report_notice(
                            taskmanager_shell::FeedbackSource::Demo,
                            taskmanager_shell::FeedbackSeverity::Warning,
                            taskmanager_shell::FeedbackLifecycle::UntilReplaced,
                            "Demo mode suppresses platform actions",
                        ),
                    }
                }
                if app.should_quit() || drained + 1 == EVENT_DRAIN_BATCH {
                    break;
                }
                if !events.poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        if app.should_quit() {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/runtime/seam_tests.rs"]
mod tests;

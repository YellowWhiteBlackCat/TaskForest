//! Crossterm runtime and shared app-host wiring.

use std::convert::Infallible;
use std::io;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
use taskmanager_app_host::NativeAppHost;
use taskmanager_application::{
    AppPage, KeyCode, Modifiers, PlatformClient, PlatformEffect, RefreshRequest,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

use crate::command_palette::{TuiSurfaceScope, surface_protocol_action};
use crate::{TuiApp, TuiTerminalProfile, TuiTheme};

use crate::render;

mod keys;
mod modals;
mod navigation;
mod seam;
mod semantic;

use keys::handle_key;

/// Test-only re-export of the terminal event seam: the overlay HitMap
/// behavior tests (`tests/gui/ui/tests/overlay_hit.rs`) apply pointer clicks
/// against committed frame plans through the production dispatch, and the
/// perf-budget behavior tests (`tests/gui/perf_budget_tests.rs`) drive the
/// production loop with a counting backend + scripted event source to pin the
/// dirty-repaint contract (idle cycles must not draw at all).
#[cfg(test)]
#[path = "../tests/headless/runtime/runtime_support.rs"]
pub(crate) mod runtime_support;

const EVENT_POLL: Duration = Duration::from_millis(100);
/// Upper bound on how many ready terminal events one loop cycle drains after
/// the blocking poll returns. Draining the backlog keeps a key/paste burst
/// from piling up behind one-read-per-cycle redraws; the bound guarantees the
/// loop still reaches its draw/quit checks regularly under a sustained flood
/// (leftover events drain on the next cycle, whose poll returns immediately
/// while the queue is non-empty).
const EVENT_DRAIN_BATCH: usize = 16;
/// The help overlay's PageUp/PageDown scroll stride (rows per page press).
const HELP_PAGE_STEP: usize = 8;
/// Per-engine GPU utilization re-request cadence while a session is enabled on
/// the visible Performance·GPU page (mirrors GPUI's 2.5 s panel interval; the
/// polkit `auth_admin_keep` session means post-first reads do not re-prompt).
const GPU_ENGINE_ROWS_REFRESH: Duration = Duration::from_millis(2500);

/// Per-cycle signals the dirty-flag predicate consults to decide whether the
/// TUI should repaint this iteration. The TUI used to call `terminal.draw` on
/// every loop cycle (≈10 Hz, the [`EVENT_POLL`] cadence); the runtime now
/// defers the draw until a cycle actually produces something worth painting.
///
/// The initial frame and the post-keypress/post-resize redraws are NOT here —
/// they span across cycles (the key arrives in the poll phase, which runs
/// after the draw decision) so the run loop carries them in a `pending_draw`
/// boolean instead. This struct only describes the in-cycle work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DrawCycleInputs {
    /// A non-empty platform batch was folded into the app this cycle (real new
    /// telemetry / process / hardware / service / etc. data arrived).
    platform_batch: bool,
    /// A scheduled or user-triggered request was queued this cycle. The data
    /// lands asynchronously in a later cycle's drain; queueing still marks
    /// the cycle dirty so the next paint is not delayed by a poll timeout.
    refresh_queued: bool,
    /// An ancillary effect was queued this cycle (selected-process insights
    /// refresh, service-log tail poll, desktop notification submission).
    ancillary_effect: bool,
}

/// The dirty-flag predicate: return `true` when this cycle produced new data
/// to paint or queued work that will. Pure function of [`DrawCycleInputs`] so
/// the skip-vs-draw decision is unit-testable without driving a live
/// crossterm `Terminal` (which needs a real tty).
#[must_use]
fn should_draw(inputs: DrawCycleInputs) -> bool {
    inputs.platform_batch || inputs.refresh_queued || inputs.ancillary_effect
}

pub fn run_live() -> io::Result<()> {
    run_interactive(false)
}

pub fn run_demo() -> io::Result<()> {
    run_interactive(true)
}

#[must_use]
pub fn snapshot_text(width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = unwrap_infallible(Terminal::new(backend));
    let mut app = crate::demo_app();
    app.expanded_groups = crate::default_category_expansions();
    let theme = TuiTheme::from_params(app.theme_params);
    let _ = unwrap_infallible(terminal.draw(|frame| render(frame, &app, theme)));
    terminal.backend().to_string()
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}

fn capture_page_name(page: AppPage) -> &'static str {
    match page {
        AppPage::Performance => "performance",
        AppPage::Applications => "applications",
        AppPage::Services => "services",
        AppPage::System => "system",
        AppPage::Startup => "startup",
        AppPage::Users => "users",
        AppPage::AppHistory => "app-history",
    }
}

fn unwrap_infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn run_interactive(demo: bool) -> io::Result<()> {
    let host = NativeAppHost::production();
    let mut app = if demo {
        let mut app = crate::demo_app();
        app.expanded_groups = crate::default_category_expansions();
        app
    } else {
        match host.config_client() {
            Ok(client) => TuiApp::new_with_config_client(client),
            Err(error) => {
                let mut app = TuiApp::from_shell(taskmanager_shell::ShellApp::new());
                app.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("Configuration runtime unavailable: {error}"),
                );
                app
            }
        }
    };
    app.local_time_rules = if demo {
        taskmanager_core::core::time::LocalTimeRulesObservation::current(
            taskmanager_core::core::time::LocalTimeRules::utc(),
            0,
        )
    } else {
        host.local_time_rules()
    };
    if !demo {
        app.request_history_frontend(app.history_persistence_enabled());
        app.install_history_frontend_connector(host.history_frontend_connector());
        match host.snapshot_export_client() {
            Ok(client) => app.install_snapshot_export_client(client),
            Err(error) => app.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                format!("Snapshot export runtime unavailable: {error}"),
            ),
        }
    }
    let mut platform = if demo {
        None
    } else {
        let client = host.spawn_client().map_err(|error| {
            io::Error::other(format!("native platform composition failed: {error}"))
        })?;
        Some(client)
    };
    if let Some(platform) = platform.as_mut() {
        taskmanager_shell::queue_effect(
            &mut app,
            platform,
            PlatformEffect::Refresh(RefreshRequest::Dashboard),
        );
    }
    let capture_marker = std::env::var_os("TM_TUI_CAPTURE_MARKER_FILE");
    // Resolve terminal capabilities once at the native composition edge. The
    // event loop and every renderer consume this value; no component reads
    // TERM/locale or makes its own color/glyph guess.
    let terminal_profile = TuiTerminalProfile::detect();
    // The full cycle — drain, pacing, draw decision, event application,
    // quit — lives behind the runtime seam (`runtime/seam.rs`), generic over
    // the terminal backend and the event source so it is drivable headlessly
    // (TestBackend + scripted source) and a remote transport can substitute
    // its own normalized events later without touching this composition.
    ratatui::run(|terminal| {
        // Wheel paging needs mouse reporting and search paste needs
        // bracketed-paste reporting (ratatui's init enables neither). Both
        // are enabled for the whole session and ALWAYS disabled before the
        // terminal restores, on the success and error paths alike. The
        // common emulators bypass mouse reporting while Shift is held, so
        // native click-drag text selection stays available — the btop
        // convention.
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::EnableMouseCapture,
            ratatui::crossterm::event::EnableBracketedPaste
        )?;
        let result = seam::run_event_loop_with_profile(
            terminal,
            &mut app,
            platform.as_mut(),
            seam::CrosstermEventSource,
            demo,
            capture_marker.as_deref(),
            terminal_profile,
        );
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::DisableBracketedPaste,
            ratatui::crossterm::event::DisableMouseCapture
        );
        result
    })
}

/// Submit the shared alert center's queued desktop notifications (BN-07).
/// The evaluation decision lives in the shell; the TUI only routes requests.
/// Returns whether any notification was queued, so the run loop can flag
/// the cycle as dirty (a freshly-fired alert changes the rendered chrome).
fn submit_alert_notifications(app: &mut ShellApp, platform: &mut PlatformClient) -> bool {
    let mut queued = false;
    for request in app.drain_alert_notifications() {
        taskmanager_shell::queue_effect(
            app,
            platform,
            PlatformEffect::DesktopNotification(request),
        );
        queued = true;
    }
    queued
}

/// Whether the Applications-page inline insights panel is showing the typed
/// `RequiresEscalation` network facet for the selected row — the gate for the
/// page-level `e` escalation trigger (G-04b), mirroring the Properties-modal
/// binding and the rendered hint so the key can never fire a prompt the
/// current projection did not ask for.
fn inline_network_escalation_ready(app: &TuiApp) -> bool {
    app.selected_detail_process().is_some_and(|process| {
        crate::ui::process_details::network_requires_escalation(app, process.pid)
    })
}

/// Drain the shell's post-control process-list refresh request (G-01
/// payoff): a completed End-task / signal / affinity / resource-limit action
/// asked for a fresh process list, and the shell exposes that intent as a
/// one-shot effect the frontend submits through the shared `queue_effect`
/// seam (never a frontend-owned platform call). Returns whether a refresh
/// was drained so the run loop can flag the cycle dirty.
fn drain_process_refresh(app: &mut TuiApp, platform: &mut PlatformClient) -> bool {
    match app.shell.take_process_refresh_request() {
        Some(effect) => {
            taskmanager_shell::queue_effect(&mut app.shell, platform, effect);
            true
        }
        None => false,
    }
}

pub(super) fn handle_settings_key(app: &mut TuiApp, key: KeyEvent) {
    match key.code {
        ratatui::crossterm::event::KeyCode::Tab | ratatui::crossterm::event::KeyCode::Down => {
            app.settings_form.move_field(1)
        }
        ratatui::crossterm::event::KeyCode::BackTab | ratatui::crossterm::event::KeyCode::Up => {
            app.settings_form.move_field(-1)
        }
        ratatui::crossterm::event::KeyCode::Left => {
            app.begin_settings_edit();
            app.settings_form.step_value(-1);
        }
        ratatui::crossterm::event::KeyCode::Right => {
            app.begin_settings_edit();
            app.settings_form.step_value(1);
        }
        ratatui::crossterm::event::KeyCode::Enter => {
            let _ = app.apply_settings_form();
        }
        ratatui::crossterm::event::KeyCode::Esc => app.cancel_settings(),
        // The overlay toggle chords (and the `p` self-toggle) are declared in
        // the surface-protocol table
        // ([`crate::command_palette::TUI_SURFACE_PROTOCOL`]) and resolve
        // through it, matching the `?`/`T` self-toggle precedent. The modal
        // consumes every key, so an unmatched character is a silent no-op and
        // can never double-route a global command.
        ratatui::crossterm::event::KeyCode::Char(character) => {
            if let Some(action) = surface_protocol_action(TuiSurfaceScope::Settings, character) {
                app.run_surface_protocol_action(action);
            }
        }
        _ => {}
    }
}

fn key_to_terminal(event: KeyEvent) -> Option<taskmanager_shell::ShellKeyEvent> {
    let key = match event.code {
        ratatui::crossterm::event::KeyCode::Char('f' | 'F') => KeyCode::F,
        ratatui::crossterm::event::KeyCode::Char('a' | 'A') => KeyCode::A,
        ratatui::crossterm::event::KeyCode::Char('1') => KeyCode::Digit1,
        ratatui::crossterm::event::KeyCode::Char('2') => KeyCode::Digit2,
        ratatui::crossterm::event::KeyCode::Char('3') => KeyCode::Digit3,
        ratatui::crossterm::event::KeyCode::Char('4') => KeyCode::Digit4,
        ratatui::crossterm::event::KeyCode::Char('5') => KeyCode::Digit5,
        ratatui::crossterm::event::KeyCode::Char('6') => KeyCode::Digit6,
        ratatui::crossterm::event::KeyCode::Char('7') => KeyCode::Digit7,
        ratatui::crossterm::event::KeyCode::Char('8') => KeyCode::Digit8,
        ratatui::crossterm::event::KeyCode::Char(' ') => KeyCode::Space,
        ratatui::crossterm::event::KeyCode::PageUp => KeyCode::PageUp,
        ratatui::crossterm::event::KeyCode::PageDown => KeyCode::PageDown,
        ratatui::crossterm::event::KeyCode::Home => KeyCode::Home,
        ratatui::crossterm::event::KeyCode::End => KeyCode::End,
        ratatui::crossterm::event::KeyCode::Tab | ratatui::crossterm::event::KeyCode::BackTab => {
            KeyCode::Tab
        }
        ratatui::crossterm::event::KeyCode::F(1) => KeyCode::F1,
        ratatui::crossterm::event::KeyCode::F(5) => KeyCode::F5,
        ratatui::crossterm::event::KeyCode::F(9) => KeyCode::F9,
        ratatui::crossterm::event::KeyCode::Delete => KeyCode::Delete,
        ratatui::crossterm::event::KeyCode::Enter => KeyCode::Enter,
        ratatui::crossterm::event::KeyCode::Esc => KeyCode::Escape,
        _ => return None,
    };
    let modifiers = Modifiers::new(
        event.modifiers.contains(KeyModifiers::CONTROL),
        event.modifiers.contains(KeyModifiers::ALT),
        event.modifiers.contains(KeyModifiers::SHIFT)
            || event.code == ratatui::crossterm::event::KeyCode::BackTab,
        event.modifiers.contains(KeyModifiers::SUPER),
    );
    Some(taskmanager_shell::ShellKeyEvent::new(key, modifiers))
}

#[cfg(test)]
#[path = "../tests/gui/runtime/tests.rs"]
mod tests;

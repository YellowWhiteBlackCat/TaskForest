//! Test-only runtime seams: unplan-driven wrappers so the crate's registered
//! test modules — not only the runtime module tree — can drive the production
//! event loop and event application with a counting backend and a scripted
//! event source.

use std::ffi::OsStr;
use std::io;

use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;
use taskmanager_application::PlatformClient;

use super::{
    BackendErrorIntoIo, EventReaction, TerminalEventSource, apply_terminal_event_with_plan,
    run_event_loop_with_profile,
};
use crate::ui::TuiFramePlan;
use crate::{TuiApp, TuiTerminalProfile};

pub(crate) fn apply_terminal_event(app: &mut TuiApp, event: Event, frame: Rect) -> EventReaction {
    let plan = TuiFramePlan::build(app, frame);
    apply_terminal_event_with_plan(app, event, &plan)
}

/// Behavior is identical to the former inline `ratatui::run` closure.
pub(crate) fn run_event_loop<B: Backend, E: TerminalEventSource>(
    terminal: &mut Terminal<B>,
    app: &mut TuiApp,
    platform: Option<&mut PlatformClient>,
    events: E,
    demo: bool,
    capture_marker: Option<&OsStr>,
) -> io::Result<()>
where
    B::Error: BackendErrorIntoIo,
{
    run_event_loop_with_profile(
        terminal,
        app,
        platform,
        events,
        demo,
        capture_marker,
        TuiTerminalProfile::default(),
    )
}

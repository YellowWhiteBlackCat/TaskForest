//! TUI adapter for current-window PNG capture intent and app-host session.
//!
//! The capture mechanism is toolkit-agnostic (the platform adapter captures
//! the active compositor window — for a terminal product that is the terminal
//! emulator window showing the frame). The TUI owns only the request session
//! and renders its feedback through the shared notice line.

use taskmanager_app_host::WindowCaptureClient;
use taskmanager_application::session_slot::SessionSlot;
use taskmanager_application::window_capture::{
    WindowCaptureSession, drain_window_capture_completions, request_current_window_capture,
};

use super::TuiApp;

pub(crate) type TuiWindowCaptureRuntime = SessionSlot<WindowCaptureSession<WindowCaptureClient>>;

impl TuiApp {
    pub(crate) fn install_window_capture_client(&mut self, client: WindowCaptureClient) {
        self.window_capture
            .install_with(|| WindowCaptureSession::new(client));
    }

    /// Request one current-window PNG capture. Returns whether a request was
    /// submitted; the honest unavailable/busy/failed states are reported
    /// through the shared notice line.
    pub(crate) fn request_current_window_capture(&mut self) -> bool {
        let outcome = request_current_window_capture(
            self.window_capture.active_mut(),
            std::env::var_os("TM_CAPTURE_WINDOW_OUTPUT"),
        );
        self.shell.report_window_capture_notice(&outcome.notice);
        outcome.submitted
    }

    /// Drain finished captures into the shared notice line. Returns whether the
    /// frame should be redrawn.
    pub(crate) fn drain_window_capture_completions(&mut self) -> bool {
        let (changed, notice) = drain_window_capture_completions(self.window_capture.active_mut());
        if let Some(notice) = notice {
            self.shell.report_window_capture_notice(&notice);
        }
        changed
    }
}

#[cfg(test)]
#[path = "../tests/headless/window_capture_tests.rs"]
mod tests;

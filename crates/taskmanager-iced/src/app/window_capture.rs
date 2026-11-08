//! Iced adapter for current-window PNG capture intent and app-host session.

use taskmanager_app_host::WindowCaptureClient;
use taskmanager_application::session_slot::SessionSlot;
use taskmanager_application::window_capture::{
    WindowCaptureSession, drain_window_capture_completions, request_current_window_capture,
};

use super::IcedApp;

pub(super) type IcedWindowCaptureRuntime = SessionSlot<WindowCaptureSession<WindowCaptureClient>>;

impl IcedApp {
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

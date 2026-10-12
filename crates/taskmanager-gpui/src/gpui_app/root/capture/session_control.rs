//! Deterministic, non-destructive Users confirmation capture.

use super::{CaptureEvidence, CaptureScenario};
use taskmanager_core::core::session::{SessionIncarnation, SessionItem};

impl CaptureEvidence {
    pub(crate) fn session_control_confirmation_requested(&self) -> bool {
        self.is_enabled()
            && self.telemetry_ready()
            && self.ui_data_ready()
            && !self.scenario_ready()
            && self.scenario == Some(CaptureScenario::UsersSessionConfirmation)
    }

    pub(crate) fn mark_session_control_confirmation_ready(&mut self, ready: bool) {
        if self.session_control_confirmation_requested() && ready {
            self.mark_scenario_ready();
        }
    }
}

pub(crate) fn session_control_fixture() -> SessionItem {
    SessionItem {
        id: "capture-session-7".into(),
        incarnation: Some(SessionIncarnation::LinuxLogind {
            boot_id: "capture-boot-identity".into(),
            created_monotonic_usec: 700_000,
        }),
        uid: 1000,
        user: "capture-user".into(),
        seat: Some("seat0".into()),
        tty: Some("tty7".into()),
        remote: false,
        timestamp: Some("2026-10-12 09:30".into()),
    }
}

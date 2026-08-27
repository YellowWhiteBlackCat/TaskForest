//! UI-only watchdog for the first complete telemetry frame.
//!
//! Owns [`TelemetryWarmupPhase`] and its elapsed-time derivation. Extracted
//! from `root.rs` to respect the source line budget; behavior is unchanged.

use std::time::{Duration, Instant};

use gpui::Context;
use taskmanager_application::RefreshRequest;
use taskmanager_shell::TelemetryFrameState;

use super::RootView;

/// UI-only watchdog phases for the first complete telemetry frame.
///
/// These phases never replace the shared TelemetryFrameState and never
/// claim that a provider failed. They only decide when the startup surface
/// should explain that waiting is taking longer than expected and offer the
/// same typed `RefreshRequest::All` retry used by the normal shell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TelemetryWarmupPhase {
    #[default]
    Collecting,
    Slow,
    Retryable,
}

const TELEMETRY_WARMUP_SLOW_AFTER: Duration = Duration::from_secs(5);
const TELEMETRY_WARMUP_RETRY_AFTER: Duration = Duration::from_secs(15);

impl TelemetryWarmupPhase {
    #[must_use]
    pub(crate) const fn allows_retry(self) -> bool {
        matches!(self, Self::Retryable)
    }
}

#[must_use]
pub(crate) fn telemetry_warmup_phase(elapsed: Duration) -> TelemetryWarmupPhase {
    if elapsed >= TELEMETRY_WARMUP_RETRY_AFTER {
        TelemetryWarmupPhase::Retryable
    } else if elapsed >= TELEMETRY_WARMUP_SLOW_AFTER {
        TelemetryWarmupPhase::Slow
    } else {
        TelemetryWarmupPhase::Collecting
    }
}

impl RootView {
    /// Test and fixture helper for advancing the visible frame lifecycle
    /// without bypassing the typed state itself. Production code reaches the
    /// same transition only after the shared fold reports `FrameCommit::Committed`.
    pub fn mark_telemetry_frame_ready(&mut self) {
        self.telemetry_frame_state = TelemetryFrameState::Ready;
    }

    /// Return the presentation phase of the first-frame watchdog.
    #[must_use]
    pub(crate) fn telemetry_warmup_phase(&self) -> TelemetryWarmupPhase {
        telemetry_warmup_phase(self.telemetry_warmup_started_at.elapsed())
    }

    /// Re-submit the full initial collection request and restart the UI
    /// watchdog. The shared projection remains authoritative; a retry never
    /// clears a committed snapshot or invents a new frame state.
    pub(crate) fn retry_telemetry_warmup(&mut self, cx: &mut Context<Self>) {
        self.telemetry_warmup_started_at = Instant::now();
        self.telemetry_warmup_retry_button = None;
        self.request_refresh(RefreshRequest::All);
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_telemetry_warmup_tests.rs"]
mod telemetry_warmup_tests;

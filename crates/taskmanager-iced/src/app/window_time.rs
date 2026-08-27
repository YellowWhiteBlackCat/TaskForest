//! Per-window event-time cache populated only by the Iced tick boundary.

/// Renderer filtering reads this injected timestamp; views never consult the
/// wall clock. The cache is replaced on every accepted tick and dies with its
/// window, so it cannot leak time authority across windows or sessions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct WindowTimeCache {
    service_log_now_micros: u64,
}

impl WindowTimeCache {
    pub(super) fn observe_tick_millis(&mut self, now_ms: u64) {
        self.service_log_now_micros = now_ms.saturating_mul(1_000);
    }

    pub(super) const fn service_log_now_micros(self) -> u64 {
        self.service_log_now_micros
    }
}

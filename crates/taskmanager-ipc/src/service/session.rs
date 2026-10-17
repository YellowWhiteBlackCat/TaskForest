//! Per-client connection session tracking and on-demand subscription state.

use crate::protocol::mask::SubscriptionMask;

/// State of an active client connection handled by the daemon.
#[derive(Debug, Clone)]
pub struct ClientSession {
    pub session_id: u64,
    pub client_name: String,
    pub client_version: String,
    pub pid: u32,
    pub subscription: SubscriptionMask,
    pub interval_ms: u32,
    pub last_activity_ms: u64,
}

impl ClientSession {
    #[must_use]
    pub fn new(
        session_id: u64,
        client_name: String,
        client_version: String,
        pid: u32,
        now_ms: u64,
    ) -> Self {
        Self {
            session_id,
            client_name,
            client_version,
            pid,
            subscription: SubscriptionMask::empty(),
            interval_ms: 1000,
            last_activity_ms: now_ms,
        }
    }

    pub fn update_subscription(&mut self, mask: SubscriptionMask, interval_ms: u32, now_ms: u64) {
        self.subscription = mask;
        self.interval_ms = interval_ms.max(50); // Bound minimum refresh interval to 50ms
        self.last_activity_ms = now_ms;
    }

    #[must_use]
    pub fn wants_facet(&self, facet: SubscriptionMask) -> bool {
        self.subscription.contains(facet)
    }
}

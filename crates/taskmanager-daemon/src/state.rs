//! Daemon runtime state, active client tracking, and cadence arbitrator.

use std::sync::{Arc, Mutex};

use taskmanager_ipc::{SubscriptionHub, SubscriptionMask};

/// Active execution mode of the daemon sampling engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonCadenceMode {
    /// Zero clients connected: downscaled to idle heartbeat (e.g. 10s).
    Idle,
    /// Active frontends connected: sampling active facets at the requested interval.
    Active { interval_ms: u32 },
}

/// Daemon state coordinator managing subscription hubs and sampling schedules.
#[derive(Debug)]
pub struct DaemonState {
    pub hub: Arc<Mutex<SubscriptionHub>>,
    pub active_interval_ms: u32,
    pub idle_interval_ms: u32,
}

impl DaemonState {
    #[must_use]
    pub fn new(active_interval_ms: u32, idle_interval_ms: u32) -> Self {
        Self {
            hub: Arc::new(Mutex::new(SubscriptionHub::new())),
            active_interval_ms: active_interval_ms.max(50),
            idle_interval_ms: idle_interval_ms.max(1000),
        }
    }

    /// Determine current sampling mode based on active subscriber count.
    #[must_use]
    pub fn current_cadence(&self) -> DaemonCadenceMode {
        if let Ok(hub) = self.hub.lock() {
            if hub.active_session_count() > 0 && !hub.aggregated_mask().is_empty() {
                DaemonCadenceMode::Active {
                    interval_ms: self.active_interval_ms,
                }
            } else {
                DaemonCadenceMode::Idle
            }
        } else {
            DaemonCadenceMode::Idle
        }
    }

    /// Retrieve the current aggregated subscription mask across all frontends.
    #[must_use]
    pub fn active_mask(&self) -> SubscriptionMask {
        self.hub
            .lock()
            .map(|h| h.aggregated_mask())
            .unwrap_or_else(|_| SubscriptionMask::empty())
    }

    /// Number of currently connected frontend client sessions.
    #[must_use]
    pub fn client_count(&self) -> usize {
        self.hub
            .lock()
            .map(|h| h.active_session_count())
            .unwrap_or(0)
    }
}

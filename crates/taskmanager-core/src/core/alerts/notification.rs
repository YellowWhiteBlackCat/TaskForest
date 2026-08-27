//! Pure delivery policy for desktop notifications of fired alerts.
//!
//! [`NotificationGate`] decides *whether* a fired alert should reach the
//! desktop right now: policy-enabled, outside quiet hours, and not inside the
//! per-instance cooldown. It performs no I/O and has no platform dependency,
//! so the same policy is unit-tested once and consumed by any frontend that
//! evaluates alerts (GPUI today, Iced/TUI when they adopt BN-07).

use serde::{Deserialize, Serialize};

use super::Alert;

/// Seconds of quiet hours represented as minutes after midnight; `start >= end`
/// spans midnight (e.g. 22:00–07:00 is `(1320, 420)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietHours {
    pub start_minutes: u16,
    pub end_minutes: u16,
}

impl QuietHours {
    #[must_use]
    pub fn contains_minute(&self, minute_of_day: u16) -> bool {
        if self.start_minutes == self.end_minutes {
            return false;
        }
        if self.start_minutes < self.end_minutes {
            minute_of_day >= self.start_minutes && minute_of_day < self.end_minutes
        } else {
            minute_of_day >= self.start_minutes || minute_of_day < self.end_minutes
        }
    }
}

/// Per-instance delivery gate over fired alerts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotificationPolicy {
    pub enabled: bool,
    /// Minimum milliseconds between two notifications for the same
    /// `instance_id` (rule id + target). Zero disables cooldown.
    pub cooldown_ms: u64,
    pub quiet_hours: Option<QuietHours>,
}

impl Default for NotificationPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            cooldown_ms: 5 * 60 * 1000,
            quiet_hours: None,
        }
    }
}

/// Which quiet-hours bound a settings control mutates (BN-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuietBound {
    Start,
    End,
}

/// Apply one quiet-hours bound to the current window. Equal bounds mean
/// "no quiet hours" (the gate treats them as never-suppressing), so setting
/// either bound to the other's value clears the window. Single-source
/// semantics shared by every frontend picker (GPUI selects, TUI form,
/// Iced chooser).
#[must_use]
pub fn apply_quiet_hour_bound(
    current: Option<QuietHours>,
    bound: QuietBound,
    hour: u8,
) -> Option<QuietHours> {
    let hour = u16::from(hour);
    let (start, end) = match (bound, current) {
        (QuietBound::Start, Some(hours)) => (hour, hours.end_minutes / 60),
        (QuietBound::End, Some(hours)) => (hours.start_minutes / 60, hour),
        // No window yet: the unset bound starts at 00:00 so setting ONE
        // bound creates a real window (e.g. end=07:00 → 00:00–07:00), the
        // same behavior as the TUI form's independent hour fields.
        (QuietBound::Start, None) => (hour, 0),
        (QuietBound::End, None) => (0, hour),
    };
    (start != end).then(|| QuietHours {
        start_minutes: start * 60,
        end_minutes: end * 60,
    })
}

/// Tracks the last notified timestamp per instance to enforce the cooldown.
#[derive(Clone, Debug, Default)]
pub struct NotificationGate {
    policy: NotificationPolicy,
    last_notified_ms: std::collections::HashMap<String, u64>,
}

impl NotificationGate {
    #[must_use]
    pub fn new(policy: NotificationPolicy) -> Self {
        Self {
            policy,
            last_notified_ms: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn policy(&self) -> &NotificationPolicy {
        &self.policy
    }

    pub fn set_policy(&mut self, policy: NotificationPolicy) {
        self.policy = policy;
    }

    /// True when the alert should reach the desktop at `now_ms`. A true
    /// verdict records the delivery time (once), so a second call for the same
    /// instance immediately returns false until the cooldown elapses.
    #[must_use]
    pub fn consider(&mut self, alert: &Alert, now_ms: u64) -> bool {
        if !self.policy.enabled {
            return false;
        }
        if let Some(hours) = self.policy.quiet_hours {
            let minute_of_day = ((now_ms / 60_000) % 1440) as u16;
            if hours.contains_minute(minute_of_day) {
                return false;
            }
        }
        // Entries older than the only semantic window that can consult them
        // cannot affect a future verdict. Retiring them here bounds the map by
        // alert identities notified within one cooldown window, even when
        // disks or sensor targets churn indefinitely.
        let cooldown_ms = self.policy.cooldown_ms;
        self.last_notified_ms
            .retain(|_, last| now_ms.saturating_sub(*last) <= cooldown_ms);
        if self.policy.cooldown_ms > 0
            && let Some(&last) = self.last_notified_ms.get(&alert.instance_id)
            && now_ms.saturating_sub(last) < self.policy.cooldown_ms
        {
            return false;
        }
        self.last_notified_ms
            .insert(alert.instance_id.clone(), now_ms);
        true
    }

    /// Forget the delivery history (e.g. rules changed or history cleared).
    pub fn clear(&mut self) {
        self.last_notified_ms.clear();
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_alerts_notification_tests.rs"]
mod tests;

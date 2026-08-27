//! Shared, toolkit-neutral dispatcher for alert desktop notifications (BN-07).
//!
//! # Why this module exists
//!
//! Delivery policy and the "which alert just fired" decision must live in ONE
//! place that every frontend (GPUI today, TUI/Iced when they adopt alert
//! evaluation) can call. A frontend only:
//!
//! 1. hands the previous + next active-alert sets to
//!    [`AlertDispatcher::dispatch_new`], and
//! 2. submits the returned [`DesktopNotificationRequest`]s through its own
//!    platform client.
//!
//! The dispatcher owns the [`NotificationGate`] (cooldown per instance, quiet
//! hours, the opt-in switch) so a second frontend can never diverge on policy.
//! It performs no I/O and has no platform dependency.

use taskmanager_core::alerts::{Alert, NotificationGate, NotificationPolicy};

use crate::platform::DesktopNotificationRequest;

/// Shared alert-to-notification delivery state.
#[derive(Debug, Clone)]
pub struct AlertDispatcher {
    gate: NotificationGate,
}

impl Default for AlertDispatcher {
    fn default() -> Self {
        Self::new(NotificationPolicy {
            enabled: false,
            ..NotificationPolicy::default()
        })
    }
}

impl AlertDispatcher {
    /// Build a dispatcher with an explicit policy. The built-in default is
    /// opt-out (`enabled: false`); persist/restore the user's choice through
    /// `Config::notification_policy` / `Config::apply_notification_policy`.
    #[must_use]
    pub fn new(policy: NotificationPolicy) -> Self {
        Self {
            gate: NotificationGate::new(policy),
        }
    }

    #[must_use]
    pub fn policy(&self) -> &NotificationPolicy {
        self.gate.policy()
    }

    pub fn set_policy(&mut self, policy: NotificationPolicy) {
        self.gate.set_policy(policy);
    }

    /// Turn an alert evaluation transition into desktop notifications.
    ///
    /// Only transitions INTO the active set are considered (an alert that
    /// stays active keeps its silence until it clears and re-fires); the pure
    /// gate then applies the policy (opt-in, cooldown per instance, quiet
    /// hours). A disabled policy or a gated instance yields no request.
    ///
    /// The caller owns submission: platform absence or a rejected submission
    /// is an honest no-op on its side (delivery is best-effort by design).
    pub fn dispatch_new(
        &mut self,
        previous: &[Alert],
        next: &[Alert],
        now_ms: u64,
    ) -> Vec<DesktopNotificationRequest> {
        if !self.gate.policy().enabled {
            return Vec::new();
        }
        next.iter()
            .filter(|alert| {
                !previous
                    .iter()
                    .any(|old| old.instance_id == alert.instance_id)
            })
            .filter_map(|alert| {
                if !self.gate.consider(alert, now_ms) {
                    return None;
                }
                let metric_label = match alert.metric {
                    taskmanager_core::alerts::AlertMetric::CpuUsagePercent => "CPU Usage",
                    taskmanager_core::alerts::AlertMetric::MemoryUsagePercent => "Memory Usage",
                    taskmanager_core::alerts::AlertMetric::DiskTemperatureC => "Disk Temperature",
                    taskmanager_core::alerts::AlertMetric::SmartPercentUsed => "SSD Wear",
                    taskmanager_core::alerts::AlertMetric::SmartCriticalWarning => "SMART Warning",
                };
                let body = match alert.metric {
                    taskmanager_core::alerts::AlertMetric::CpuUsagePercent
                    | taskmanager_core::alerts::AlertMetric::MemoryUsagePercent
                    | taskmanager_core::alerts::AlertMetric::SmartPercentUsed => {
                        format!(
                            "{}: {:.0}% (threshold: {:.0}%)",
                            metric_label, alert.value, alert.threshold
                        )
                    }
                    taskmanager_core::alerts::AlertMetric::DiskTemperatureC => {
                        format!(
                            "{}: {:.0}°C (threshold: {:.0}°C)",
                            metric_label, alert.value, alert.threshold
                        )
                    }
                    taskmanager_core::alerts::AlertMetric::SmartCriticalWarning => {
                        format!("{}: Warning flag reported by drive", metric_label)
                    }
                };
                Some(DesktopNotificationRequest {
                    instance_id: alert.instance_id.clone(),
                    title: alert.target.clone(),
                    body,
                    severity: alert.severity,
                    target: alert.target.clone(),
                })
            })
            .collect()
    }

    /// Forget the delivery history (e.g. rules changed or history cleared).
    pub fn clear(&mut self) {
        self.gate.clear();
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_alert_dispatch_tests.rs"]
mod tests;

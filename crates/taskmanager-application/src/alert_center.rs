//! Shared alert evaluation + notification dispatch (BN-07).
//!
//! # Frontend contract (three lines)
//!
//! Every shell track owns one [`AlertCenter`] and, whenever a
//! fresh [`SystemSnapshot`] arrives, calls
//!
//! ```text
//! let evaluation = center.evaluate(&snapshot, now_ms);
//! // 1. render evaluation.active (in-app alert surface)
//! // 2. submit evaluation.notifications through the frontend's platform client
//! ```
//!
//! The center owns the complete managed-rule state, the enabled-rule
//! `AlertEngine` projection, the delivery policy
//! (`NotificationGate`), the previous-active set, and bounded transition
//! history, so "which alert just fired", "what cleared", and "should the
//! desktop be told" can never diverge between frontends. Rule editing goes
//! through [`AlertCenter::edit_rules`]; disabled rules remain visible in
//! [`AlertCenter::managed_rules`] but never enter the evaluator. Notification
//! policy changes go through [`AlertCenter::set_policy`].

use taskforest_core::core::alerts::{
    Alert, AlertEngine, AlertEvent, AlertEventId, AlertEventKind, AlertInstanceId, AlertRule,
    AlertRuleId, AlertRuleTransferError, NotificationPolicy, default_rules,
};
use taskforest_core::core::metrics::SystemSnapshot;

use crate::platform::DesktopNotificationRequest;

use super::AlertDispatcher;
use crate::managed_alert_rules::reduce_managed_alert_rules;
use crate::{ManagedAlertRule, ManagedAlertRuleEdit, ManagedAlertRuleEditOutcome};
use std::collections::HashSet;
use taskforest_core::core::alerts::MAX_ALERT_EVENTS;

/// One evaluation pass: the current active alerts plus the requests the
/// frontend should submit to the desktop notification service.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertEvaluation {
    /// Alerts currently above their thresholds (with hysteresis), sorted in
    /// engine order. Frontends render this as their in-app alert surface.
    pub active: Vec<Alert>,
    /// Desktop notifications for alerts that newly fired since the previous
    /// evaluation and passed the delivery policy. Frontends submit these
    /// through their platform client; a missing platform or a rejected
    /// submission is an honest no-op.
    pub notifications: Vec<DesktopNotificationRequest>,
    /// Exact semantic projections changed by this evaluation.
    pub changes: AlertProjectionChanges,
}

/// Projection facets changed by one alert-center mutation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AlertProjectionChanges {
    pub rules: bool,
    pub active: bool,
    pub events: bool,
}

impl AlertProjectionChanges {
    #[must_use]
    pub const fn any(self) -> bool {
        self.rules || self.active || self.events
    }
}

/// ID-free alert transition ingress for captures and deterministic fixtures.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertEventSeed {
    pub kind: AlertEventKind,
    pub alert: Alert,
    pub observed_at_ms: u64,
}

/// Outcome and projection changes from one managed-rule edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlertRuleEditResult {
    pub outcome: ManagedAlertRuleEditOutcome,
    pub changes: AlertProjectionChanges,
}

/// Shared evaluation + delivery state.
#[derive(Debug, Clone)]
pub struct AlertCenter {
    managed_rules: Vec<ManagedAlertRule>,
    engine: AlertEngine,
    dispatcher: AlertDispatcher,
    active_alerts: Vec<Alert>,
    event_history: Vec<AlertEvent>,
    event_high_water: u64,
}

impl Default for AlertCenter {
    fn default() -> Self {
        Self::new(default_rules())
    }
}

impl AlertCenter {
    #[must_use]
    pub fn new(rules: impl IntoIterator<Item = AlertRule>) -> Self {
        let engine = AlertEngine::new(rules);
        let managed_rules = engine
            .rules()
            .iter()
            .cloned()
            .map(|rule| ManagedAlertRule::new(rule, true))
            .collect();
        Self {
            managed_rules,
            engine,
            dispatcher: AlertDispatcher::default(),
            active_alerts: Vec::new(),
            event_history: Vec::new(),
            event_high_water: 0,
        }
    }

    #[must_use]
    pub fn managed_rules(&self) -> &[ManagedAlertRule] {
        &self.managed_rules
    }

    /// Enabled-rule projection used by the evaluator. Renderers should list
    /// [`Self::managed_rules`] so disabled entries are not lost.
    #[must_use]
    pub fn enabled_rules(&self) -> &[AlertRule] {
        self.engine.rules()
    }

    /// Current active alerts, owned solely by this application projection.
    #[must_use]
    pub fn active_alerts(&self) -> &[Alert] {
        &self.active_alerts
    }

    /// Session-local alert transition history shared by every renderer.
    /// Entries are oldest-first and capped by the domain retention bound;
    /// callers receive a read-only slice and cannot create a second history
    /// authority.
    #[must_use]
    pub fn event_history(&self) -> &[AlertEvent] {
        &self.event_history
    }

    /// Clear retained transition history without changing rule state, active
    /// alerts, or notification cooldowns.
    pub fn clear_event_history(&mut self) -> AlertProjectionChanges {
        let changed = !self.event_history.is_empty();
        self.event_history.clear();
        AlertProjectionChanges {
            events: changed,
            ..AlertProjectionChanges::default()
        }
    }

    /// Install deterministic transition history for a capture/test fixture.
    /// Production evaluation remains the only source of new events; this
    /// bounded replacement exists so visual evidence can exercise the same
    /// shared projection without a fake renderer-local history.
    pub fn replace_event_history(&mut self, seeds: Vec<AlertEventSeed>) -> AlertProjectionChanges {
        let Ok(seed_count) = u64::try_from(seeds.len()) else {
            return AlertProjectionChanges::default();
        };
        if self.event_high_water.checked_add(seed_count).is_none() {
            return AlertProjectionChanges::default();
        }
        let mut events = Vec::with_capacity(seeds.len().min(MAX_ALERT_EVENTS));
        let mut next_high_water = self.event_high_water;
        for seed in seeds {
            let Some(next_id) = next_high_water.checked_add(1) else {
                return AlertProjectionChanges::default();
            };
            let Some(id) = AlertEventId::new(next_id) else {
                return AlertProjectionChanges::default();
            };
            next_high_water = next_id;
            events.push(AlertEvent {
                id,
                kind: seed.kind,
                alert: seed.alert,
                observed_at_ms: seed.observed_at_ms,
            });
        }
        self.event_high_water = next_high_water;
        let excess = events.len().saturating_sub(MAX_ALERT_EVENTS);
        if excess > 0 {
            events.drain(..excess);
        }
        let changed = self.event_history != events;
        self.event_history = events;
        AlertProjectionChanges {
            events: changed,
            ..AlertProjectionChanges::default()
        }
    }

    /// Apply one canonical rule edit. Invalid imports/updates are atomic;
    /// missing stable rule identities are typed no-ops. A real change rebuilds only
    /// the enabled engine projection while notification cooldown history
    /// survives.
    pub fn edit_rules(
        &mut self,
        edit: ManagedAlertRuleEdit,
    ) -> Result<AlertRuleEditResult, AlertRuleTransferError> {
        let outcome = reduce_managed_alert_rules(&mut self.managed_rules, edit)?;
        if outcome.changed() {
            self.engine = AlertEngine::new(
                self.managed_rules
                    .iter()
                    .filter(|managed| managed.enabled)
                    .map(|managed| managed.rule.clone()),
            );
        }
        let retained_rule_ids: HashSet<AlertRuleId> = self
            .managed_rules
            .iter()
            .map(|managed| managed.rule.id.clone())
            .collect();
        self.dispatcher.retain_rule_ids(&retained_rule_ids);
        Ok(AlertRuleEditResult {
            outcome,
            changes: AlertProjectionChanges {
                rules: outcome.changed(),
                ..AlertProjectionChanges::default()
            },
        })
    }

    /// Reset both the engine state and the delivery history (e.g. the user
    /// cleared the alert history).
    pub fn reset(&mut self) -> AlertProjectionChanges {
        self.engine = AlertEngine::new(
            self.managed_rules
                .iter()
                .filter(|managed| managed.enabled)
                .map(|managed| managed.rule.clone()),
        );
        self.dispatcher.clear();
        let active_changed = !self.active_alerts.is_empty();
        let events_changed = !self.event_history.is_empty();
        self.active_alerts.clear();
        self.event_history.clear();
        AlertProjectionChanges {
            active: active_changed,
            events: events_changed,
            ..AlertProjectionChanges::default()
        }
    }

    #[must_use]
    pub fn policy(&self) -> &NotificationPolicy {
        self.dispatcher.policy()
    }

    pub fn set_policy(&mut self, policy: NotificationPolicy) {
        self.dispatcher.set_policy(policy);
    }

    /// Evaluate the snapshot and produce the frontend payload. The previous
    /// active set is tracked inside the center, so a frontend never has to
    /// remember history to get correct "newly fired" notifications.
    pub fn evaluate(&mut self, snapshot: &SystemSnapshot, now_ms: u64) -> AlertEvaluation {
        let next = self.engine.evaluate(snapshot);
        self.dispatcher
            .retain_disk_instances(self.engine.trusted_disk_instances());
        let notifications = self
            .dispatcher
            .dispatch_new(&self.active_alerts, &next, now_ms);
        let active_changed = self.active_alerts != next;
        let events_changed = self.record_transitions(&next, snapshot.timestamp_ms);
        self.active_alerts = next;
        AlertEvaluation {
            active: self.active_alerts.clone(),
            notifications,
            changes: AlertProjectionChanges {
                active: active_changed,
                events: events_changed,
                ..AlertProjectionChanges::default()
            },
        }
    }

    fn record_transitions(&mut self, next: &[Alert], observed_at_ms: u64) -> bool {
        let previous_ids: HashSet<&AlertInstanceId> = self
            .active_alerts
            .iter()
            .map(|alert| &alert.instance_id)
            .collect();
        let next_ids: HashSet<&AlertInstanceId> =
            next.iter().map(|alert| &alert.instance_id).collect();

        let activated: Vec<Alert> = next
            .iter()
            .filter(|alert| !previous_ids.contains(&alert.instance_id))
            .cloned()
            .collect();
        let cleared: Vec<Alert> = self
            .active_alerts
            .iter()
            .filter(|alert| !next_ids.contains(&alert.instance_id))
            .cloned()
            .collect();

        let mut changed = false;
        for alert in activated {
            changed |= self.push_event(AlertEventKind::Activated, alert, observed_at_ms);
        }
        for alert in cleared {
            changed |= self.push_event(AlertEventKind::Cleared, alert, observed_at_ms);
        }
        changed
    }

    fn push_event(&mut self, kind: AlertEventKind, alert: Alert, observed_at_ms: u64) -> bool {
        let Some(next_id) = self.event_high_water.checked_add(1) else {
            return false;
        };
        let Some(id) = AlertEventId::new(next_id) else {
            return false;
        };
        self.event_high_water = next_id;
        self.event_history.push(AlertEvent {
            id,
            kind,
            alert,
            observed_at_ms,
        });
        let excess = self.event_history.len().saturating_sub(MAX_ALERT_EVENTS);
        if excess > 0 {
            self.event_history.drain(..excess);
        }
        true
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_alert_center_tests.rs"]
mod tests;

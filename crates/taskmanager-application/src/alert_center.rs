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
//! (`NotificationGate`), and the previous-active set, so "which alert just
//! fired" and "should the desktop be told" can never diverge between
//! frontends. Rule editing goes through [`AlertCenter::edit_rules`]; disabled
//! rules remain visible in [`AlertCenter::managed_rules`] but never enter the
//! evaluator. Notification policy changes go through
//! [`AlertCenter::set_policy`].

use taskmanager_core::alerts::{
    Alert, AlertEngine, AlertRule, AlertRuleTransferError, NotificationPolicy, default_rules,
};
use taskmanager_core::metrics::SystemSnapshot;

use crate::platform::DesktopNotificationRequest;

use super::AlertDispatcher;
use crate::managed_alert_rules::reduce_managed_alert_rules;
use crate::{ManagedAlertRule, ManagedAlertRuleEdit, ManagedAlertRuleEditOutcome};

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
}

/// Shared evaluation + delivery state.
#[derive(Debug, Clone)]
pub struct AlertCenter {
    managed_rules: Vec<ManagedAlertRule>,
    engine: AlertEngine,
    dispatcher: AlertDispatcher,
    last_active: Vec<Alert>,
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
            last_active: Vec::new(),
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

    /// Apply one canonical rule edit. Invalid imports/updates are atomic;
    /// missing stable rule identities are typed no-ops. A real change rebuilds only
    /// the enabled engine projection while notification cooldown history
    /// survives.
    pub fn edit_rules(
        &mut self,
        edit: ManagedAlertRuleEdit,
    ) -> Result<ManagedAlertRuleEditOutcome, AlertRuleTransferError> {
        let outcome = reduce_managed_alert_rules(&mut self.managed_rules, edit)?;
        if outcome.changed() {
            self.engine = AlertEngine::new(
                self.managed_rules
                    .iter()
                    .filter(|managed| managed.enabled)
                    .map(|managed| managed.rule.clone()),
            );
        }
        Ok(outcome)
    }

    /// Reset both the engine state and the delivery history (e.g. the user
    /// cleared the alert history).
    pub fn reset(&mut self) {
        self.engine = AlertEngine::new(
            self.managed_rules
                .iter()
                .filter(|managed| managed.enabled)
                .map(|managed| managed.rule.clone()),
        );
        self.dispatcher.clear();
        self.last_active.clear();
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
        let notifications = self
            .dispatcher
            .dispatch_new(&self.last_active, &next, now_ms);
        self.last_active = next;
        AlertEvaluation {
            active: self.last_active.clone(),
            notifications,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_alert_center_tests.rs"]
mod tests;

//! TUI-owned selection and toggle behavior for canonical managed alert rules.

use crate::TuiApp;
use taskmanager_application::ManagedAlertRuleEdit;
use taskmanager_core::core::alerts::AlertRuleId;

impl TuiApp {
    /// The currently selected rule index in the health overlay. Clamped against
    /// the current managed-rules count.
    #[must_use]
    pub fn health_rule_selection(&self) -> usize {
        let count = self.projection().alert_center.managed_rules().len();
        if count == 0 {
            0
        } else {
            self.health_rule_selection.min(count - 1)
        }
    }

    /// Move the health overlay's managed-rule selection cursor by `delta`.
    pub fn health_rule_move(&mut self, delta: isize) {
        let count = self.projection().alert_center.managed_rules().len();
        if count == 0 {
            self.health_rule_selection = 0;
            return;
        }
        let current = self.health_rule_selection();
        self.health_rule_selection = current.saturating_add_signed(delta).min(count - 1);
    }

    /// Toggle the currently selected managed alert rule in the health overlay.
    pub fn toggle_selected_alert_rule(&mut self) -> bool {
        let index = self.health_rule_selection();
        let rules = self.projection().alert_center.managed_rules();
        if let Some(managed) = rules.get(index) {
            let rule_id = managed.rule.id.clone();
            let _ = self
                .shell
                .edit_alert_rules(ManagedAlertRuleEdit::Toggle { rule_id });
            true
        } else {
            false
        }
    }

    /// Toggle a managed alert rule by ID.
    pub fn toggle_alert_rule(&mut self, rule_id: impl Into<AlertRuleId>) -> bool {
        self.shell
            .edit_alert_rules(ManagedAlertRuleEdit::Toggle {
                rule_id: rule_id.into(),
            })
            .is_ok_and(|result| result.outcome.changed())
    }

    /// Clear the recorded alert event history in the canonical alert center.
    pub fn clear_alert_event_history(&mut self) {
        self.shell.clear_alert_event_history();
    }
}

//! Terminal rule gestures reduce the current identity through the shared shell.
use crate::TuiApp;
use std::time::Duration;
use taskmanager_application::{
    AlertRuleImportMode, ManagedAlertRule, ManagedAlertRuleEdit, ManagedAlertRuleEditOutcome,
};
use taskmanager_core::core::alerts::{
    AlertMetric, AlertRule, AlertRuleConflictPolicy, AlertRuleTransferEntry,
    AlertRuleTransferError, AlertSeverity, export_alert_rules_json, import_alert_rules_json,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuleGesture {
    Add,
    Remove,
    Metric,
    Threshold(i8),
    Duration(i8),
    Hysteresis(i8),
    Severity,
    Target,
    Export,
    ImportMerge,
    ImportReplace,
}
impl TuiApp {
    pub(crate) fn edit_rule_gesture(&mut self, gesture: RuleGesture) {
        match gesture {
            RuleGesture::ImportMerge | RuleGesture::ImportReplace => {
                self.alert_import_mode = Some(if gesture == RuleGesture::ImportReplace {
                    AlertRuleImportMode::Replace
                } else {
                    AlertRuleImportMode::Merge(AlertRuleConflictPolicy::ReplaceExisting)
                });
                self.rule_notice("Paste an alert rules JSON document to import");
                return;
            }
            RuleGesture::Export => {
                match self.export_alert_rules() {
                    Ok(json) => {
                        match crate::clipboard::write_clipboard(&mut std::io::stdout(), &json) {
                            Ok(()) => self.rule_notice("Alert rules sent to terminal clipboard"),
                            Err(error) => self.rule_notice(error.to_string()),
                        }
                    }
                    Err(error) => self.rule_notice(error.to_string()),
                }
                return;
            }
            RuleGesture::Add => {
                let rules = self.projection().alert_center.managed_rules();
                let Some(id) = (0..=rules.len())
                    .map(|index| format!("custom-cpu-{index}"))
                    .find(|id| rules.iter().all(|managed| managed.rule.id != *id))
                else {
                    return;
                };
                if let Err(error) = self.add_alert_rule(AlertRule::new(
                    id,
                    AlertMetric::CpuUsagePercent,
                    AlertSeverity::Warning,
                    85.0,
                    Duration::from_secs(5),
                    5.0,
                )) {
                    self.rule_notice(error.to_string());
                }
                self.health_rule_selection = self
                    .projection()
                    .alert_center
                    .managed_rules()
                    .len()
                    .saturating_sub(1);
                return;
            }
            _ => {}
        }
        let Some(mut managed) = self
            .projection()
            .alert_center
            .managed_rules()
            .get(self.health_rule_selection())
            .cloned()
        else {
            return;
        };
        let id = managed.rule.id.clone();
        let rule = &mut managed.rule;
        let edit = match gesture {
            RuleGesture::Remove => ManagedAlertRuleEdit::Remove { rule_id: id },
            _ => {
                match gesture {
                    RuleGesture::Metric => {
                        let Some(index) = AlertMetric::ALL
                            .iter()
                            .position(|metric| *metric == rule.metric)
                        else {
                            return;
                        };
                        let Some(metric) = AlertMetric::ALL
                            .get((index + 1) % AlertMetric::ALL.len())
                            .copied()
                        else {
                            return;
                        };
                        rule.metric = metric;
                        if metric == AlertMetric::SmartCriticalWarning {
                            rule.threshold = 1.0;
                            rule.hysteresis = 1.0;
                        }
                        if matches!(
                            metric,
                            AlertMetric::CpuUsagePercent | AlertMetric::MemoryUsagePercent
                        ) {
                            rule.target = None;
                        }
                    }
                    RuleGesture::Threshold(delta) => {
                        rule.threshold = (rule.threshold + f32::from(delta)).max(0.0)
                    }
                    RuleGesture::Hysteresis(delta) => {
                        rule.hysteresis = (rule.hysteresis + f32::from(delta)).max(0.0)
                    }
                    RuleGesture::Duration(delta) => {
                        rule.for_duration = if delta < 0 {
                            rule.for_duration.saturating_sub(Duration::from_secs(1))
                        } else {
                            rule.for_duration.saturating_add(Duration::from_secs(1))
                        };
                    }
                    RuleGesture::Severity => {
                        rule.severity = match rule.severity {
                            AlertSeverity::Info => AlertSeverity::Warning,
                            AlertSeverity::Warning => AlertSeverity::Critical,
                            AlertSeverity::Critical => AlertSeverity::Info,
                        };
                    }
                    RuleGesture::Target => {
                        if matches!(
                            rule.metric,
                            AlertMetric::CpuUsagePercent | AlertMetric::MemoryUsagePercent
                        ) {
                            return;
                        }
                        let targets: Vec<_> = self
                            .projection()
                            .snapshot
                            .as_ref()
                            .into_iter()
                            .flat_map(|snapshot| snapshot.disks.iter())
                            .map(|disk| disk.device_id.clone())
                            .collect();
                        rule.target = match &rule.target {
                            None => targets.first().cloned(),
                            Some(current) => targets
                                .iter()
                                .position(|id| id == current)
                                .and_then(|index| targets.get(index + 1))
                                .cloned(),
                        };
                    }
                    _ => return,
                }
                ManagedAlertRuleEdit::Update {
                    target_id: id,
                    managed,
                }
            }
        };
        if let Err(error) = self.edit_alert_rules(edit) {
            self.rule_notice(error.to_string());
        }
    }
    pub(crate) fn paste_alert_rules(&mut self, json: &str) -> bool {
        if !self.health_open() {
            return false;
        }
        let Some(mode) = self.alert_import_mode.take() else {
            return false;
        };
        match self.import_alert_rules(json, mode) {
            Ok(_) => self.rule_notice("Alert rules imported"),
            Err(error) => self.rule_notice(error.to_string()),
        }
        true
    }
    fn rule_notice(&mut self, message: impl Into<String>) {
        self.shell.report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            message,
        );
    }
}

impl TuiApp {
    /// Apply one semantic edit to the canonical managed alert-rule set.
    pub fn edit_alert_rules(
        &mut self,
        edit: ManagedAlertRuleEdit,
    ) -> Result<ManagedAlertRuleEditOutcome, AlertRuleTransferError> {
        self.shell.edit_alert_rules(edit)
    }

    pub fn add_alert_rule(
        &mut self,
        rule: AlertRule,
    ) -> Result<ManagedAlertRuleEditOutcome, AlertRuleTransferError> {
        self.edit_alert_rules(ManagedAlertRuleEdit::Add(ManagedAlertRule::new(rule, true)))
    }

    pub fn remove_alert_rule(
        &mut self,
        rule_id: String,
    ) -> Result<ManagedAlertRuleEditOutcome, AlertRuleTransferError> {
        self.edit_alert_rules(ManagedAlertRuleEdit::Remove { rule_id })
    }

    pub fn export_alert_rules(&self) -> Result<String, AlertRuleTransferError> {
        let entries: Vec<AlertRuleTransferEntry> = self
            .projection()
            .alert_center
            .managed_rules()
            .iter()
            .map(AlertRuleTransferEntry::from)
            .collect();
        export_alert_rules_json(&entries)
    }

    pub fn import_alert_rules(
        &mut self,
        json: &str,
        mode: AlertRuleImportMode,
    ) -> Result<ManagedAlertRuleEditOutcome, AlertRuleTransferError> {
        let entries = import_alert_rules_json(json)?;
        let rules: Vec<ManagedAlertRule> =
            entries.into_iter().map(ManagedAlertRule::from).collect();
        self.edit_alert_rules(ManagedAlertRuleEdit::Import { rules, mode })
    }

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

    /// Move the health overlay's alert-rule selection cursor by `delta`.
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
            let _ = self.edit_alert_rules(ManagedAlertRuleEdit::Toggle { rule_id });
            true
        } else {
            false
        }
    }

    /// Toggle a managed alert rule by ID.
    pub fn toggle_alert_rule(&mut self, rule_id: impl Into<String>) -> bool {
        self.edit_alert_rules(ManagedAlertRuleEdit::Toggle {
            rule_id: rule_id.into(),
        })
        .is_ok_and(|outcome| outcome.changed())
    }

    /// Clear the recorded alert event history in the canonical alert center.
    pub fn clear_alert_event_history(&mut self) {
        self.shell.clear_alert_event_history();
    }
}

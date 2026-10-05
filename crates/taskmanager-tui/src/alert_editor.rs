//! Terminal rule gestures reduce the current identity through the shared shell.
use crate::TuiApp;
use std::time::Duration;
use taskmanager_application::{AlertRuleImportMode, ManagedAlertRuleEdit};
use taskmanager_core::core::alerts::{
    AlertMetric, AlertRule, AlertRuleConflictPolicy, AlertSeverity,
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

//! Rule gestures resolve stable identities against the current shared projection.
use super::IcedApp;
use std::time::Duration;
use taskmanager_application::{ManagedAlertRule, ManagedAlertRuleEdit};
use taskmanager_core::core::alerts::{AlertMetric, AlertRule, AlertSeverity};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleAdjustment {
    Metric,
    Threshold(i8),
    Duration(i8),
    Hysteresis(i8),
    Severity,
    Target,
}
impl IcedApp {
    pub(crate) fn add_default_alert_rule(&mut self) {
        let rules = self.alerts_rules();
        let Some(id) = (0..=rules.len())
            .map(|index| format!("custom-cpu-{index}"))
            .find(|id| rules.iter().all(|rule| rule.rule.id != *id))
        else {
            return;
        };
        let result = self
            .shell
            .edit_alert_rules(ManagedAlertRuleEdit::Add(ManagedAlertRule::new(
                AlertRule::new(
                    id,
                    AlertMetric::CpuUsagePercent,
                    AlertSeverity::Warning,
                    85.0,
                    Duration::from_secs(5),
                    5.0,
                ),
                true,
            )));
        if let Err(error) = result {
            self.shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Error,
                FeedbackLifecycle::SHORT,
                error.to_string(),
            );
        }
    }
    pub(crate) fn adjust_alert_rule(&mut self, id: String, adjustment: RuleAdjustment) {
        let Some(mut managed) = self
            .alerts_rules()
            .iter()
            .find(|rule| rule.rule.id == id)
            .cloned()
        else {
            return;
        };
        let rule = &mut managed.rule;
        match adjustment {
            RuleAdjustment::Metric => {
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
            RuleAdjustment::Threshold(delta) => {
                rule.threshold = (rule.threshold + f32::from(delta)).max(0.0);
            }
            RuleAdjustment::Hysteresis(delta) => {
                rule.hysteresis = (rule.hysteresis + f32::from(delta)).max(0.0);
            }
            RuleAdjustment::Duration(delta) => {
                rule.for_duration = if delta < 0 {
                    rule.for_duration.saturating_sub(Duration::from_secs(1))
                } else {
                    rule.for_duration.saturating_add(Duration::from_secs(1))
                };
            }
            RuleAdjustment::Severity => {
                rule.severity = match rule.severity {
                    AlertSeverity::Info => AlertSeverity::Warning,
                    AlertSeverity::Warning => AlertSeverity::Critical,
                    AlertSeverity::Critical => AlertSeverity::Info,
                };
            }
            RuleAdjustment::Target => {
                if matches!(
                    rule.metric,
                    AlertMetric::CpuUsagePercent | AlertMetric::MemoryUsagePercent
                ) {
                    return;
                }
                let targets: Vec<_> = self
                    .shell
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
        }
        if let Err(error) = self.shell.edit_alert_rules(ManagedAlertRuleEdit::Update {
            target_id: id,
            managed,
        }) {
            self.shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Error,
                FeedbackLifecycle::SHORT,
                error.to_string(),
            );
        }
    }
}

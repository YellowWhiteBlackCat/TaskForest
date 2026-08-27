//! Platform-neutral threshold alerting over [`SystemSnapshot`].
//!
//! The engine owns only rule state. It performs no I/O and has no GPUI dependency,
//! so collectors, a status bar, or a notification adapter can all evaluate the
//! same snapshot deterministically.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::core::metrics::{DiskMetrics, SystemSnapshot};

mod transfer;
pub use transfer::{
    ALERT_RULE_FILE_SCHEMA, ALERT_RULE_FILE_VERSION, AlertRuleConflictPolicy, AlertRuleMerge,
    AlertRuleTransferEntry, AlertRuleTransferError, MAX_TRANSFER_RULES, export_alert_rules_json,
    import_alert_rules_json, merge_alert_rule_entries,
};

mod notification;
pub use notification::{
    NotificationGate, NotificationPolicy, QuietBound, QuietHours, apply_quiet_hour_bound,
};

/// Product default alert rules, single-sourced here so every frontend
/// evaluates the same baseline policy. The GPUI rule manager edits a copy of
/// these at runtime; TUI/Iced start from the same set.
#[must_use]
pub fn default_rules() -> Vec<AlertRule> {
    use std::time::Duration;
    vec![
        AlertRule::new(
            "cpu-high",
            AlertMetric::CpuUsagePercent,
            AlertSeverity::Warning,
            90.0,
            Duration::from_secs(10),
            10.0,
        ),
        AlertRule::new(
            "memory-high",
            AlertMetric::MemoryUsagePercent,
            AlertSeverity::Warning,
            90.0,
            Duration::from_secs(10),
            10.0,
        ),
        AlertRule::new(
            "disk-temperature",
            AlertMetric::DiskTemperatureC,
            AlertSeverity::Warning,
            70.0,
            Duration::from_secs(5),
            5.0,
        ),
        AlertRule::new(
            "smart-wear",
            AlertMetric::SmartPercentUsed,
            AlertSeverity::Warning,
            90.0,
            Duration::ZERO,
            2.0,
        ),
        AlertRule::new(
            "smart-critical",
            AlertMetric::SmartCriticalWarning,
            AlertSeverity::Critical,
            1.0,
            Duration::ZERO,
            1.0,
        ),
    ]
}

mod suggest;
pub use suggest::{
    InsufficientReason, RollingStatSnapshot, SUGGESTION_CONFIDENCE_HIGH_MIN_SAMPLES,
    SUGGESTION_MIN_SAMPLES, SUGGESTION_SIGMA_K, SuggestedThreshold, SuggestionBasis,
    SuggestionConfidence,
};

/// User-facing importance of an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Snapshot value inspected by an [`AlertRule`].
///
/// Serialization uses `snake_case` so suggestion payloads agree with the
/// alert-rule transfer file spelling (`cpu_usage_percent`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertMetric {
    CpuUsagePercent,
    MemoryUsagePercent,
    DiskTemperatureC,
    SmartPercentUsed,
    SmartCriticalWarning,
}

impl AlertMetric {
    /// Canonical display/order list for every metric understood by the alert
    /// engine. Frontends must derive metric tables from this list so a new
    /// metric cannot silently disappear from one renderer.
    pub const ALL: [Self; 5] = [
        Self::CpuUsagePercent,
        Self::MemoryUsagePercent,
        Self::DiskTemperatureC,
        Self::SmartPercentUsed,
        Self::SmartCriticalWarning,
    ];
}

/// One threshold, duration, and hysteresis policy.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertRule {
    /// Stable identifier. Duplicate IDs are ignored by [`AlertEngine::new`].
    pub id: String,
    pub metric: AlertMetric,
    pub severity: AlertSeverity,
    pub threshold: f32,
    /// The metric must remain at or above `threshold` for this long.
    pub for_duration: Duration,
    /// An active alert clears only at or below `threshold - hysteresis`.
    pub hysteresis: f32,
    /// Optional disk name/device ID filter. Ignored by system-wide metrics.
    pub target: Option<String>,
}

impl AlertRule {
    pub fn new(
        id: impl Into<String>,
        metric: AlertMetric,
        severity: AlertSeverity,
        threshold: f32,
        for_duration: Duration,
        hysteresis: f32,
    ) -> Self {
        Self {
            id: id.into(),
            metric,
            severity,
            threshold,
            for_duration,
            hysteresis: hysteresis.max(0.0),
            target: None,
        }
    }

    /// Limit a disk rule to a disk name or stable device ID.
    pub fn for_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }
}

/// One currently-active rule instance.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    /// De-duplication key (`<rule id>:<target>`).
    pub instance_id: String,
    pub rule_id: String,
    pub target: String,
    pub metric: AlertMetric,
    pub severity: AlertSeverity,
    pub value: f32,
    pub threshold: f32,
    pub active_since_ms: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct RuleState {
    pending_since_ms: Option<u64>,
    active_since_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct Signal {
    key_target: String,
    display_target: String,
    value: f32,
}

/// Stateful evaluator for a set of alert rules.
#[derive(Debug, Clone, Default)]
pub struct AlertEngine {
    rules: Vec<AlertRule>,
    states: HashMap<String, RuleState>,
}

impl AlertEngine {
    /// Build an engine, dropping invalid rules and duplicate IDs. A rule is
    /// invalid when its ID is blank or its threshold/hysteresis is not finite.
    pub fn new(rules: impl IntoIterator<Item = AlertRule>) -> Self {
        let mut ids = HashSet::new();
        let rules = rules
            .into_iter()
            .filter(|rule| {
                !rule.id.trim().is_empty()
                    && rule.threshold.is_finite()
                    && rule.hysteresis.is_finite()
                    && ids.insert(rule.id.clone())
            })
            .collect();
        Self {
            rules,
            states: HashMap::new(),
        }
    }

    pub fn rules(&self) -> &[AlertRule] {
        &self.rules
    }

    /// Evaluate one snapshot and return the complete, de-duplicated active list.
    ///
    /// `snapshot.timestamp_ms` is the only clock. A timestamp regression cannot
    /// accidentally satisfy a duration because elapsed time uses saturating math.
    pub fn evaluate(&mut self, snapshot: &SystemSnapshot) -> Vec<Alert> {
        let now_ms = snapshot.timestamp_ms;
        let mut active = Vec::new();
        let mut seen_instances = HashSet::new();
        let mut emitted_instances = HashSet::new();

        for rule in &self.rules {
            for signal in signals(rule, snapshot)
                .into_iter()
                .filter(|signal| signal.value.is_finite())
            {
                let instance_id = format!("{}:{}", rule.id, signal.key_target);
                seen_instances.insert(instance_id.clone());
                let state = self.states.entry(instance_id.clone()).or_default();
                let clear_threshold = rule.threshold - rule.hysteresis;

                if state.active_since_ms.is_some() {
                    if signal.value <= clear_threshold {
                        *state = RuleState::default();
                        continue;
                    }
                } else if signal.value >= rule.threshold {
                    let pending_since = *state.pending_since_ms.get_or_insert(now_ms);
                    let required_ms =
                        u64::try_from(rule.for_duration.as_millis()).unwrap_or(u64::MAX);
                    if now_ms.saturating_sub(pending_since) >= required_ms {
                        state.active_since_ms = Some(now_ms);
                    }
                } else {
                    state.pending_since_ms = None;
                }

                if let Some(active_since_ms) = state.active_since_ms
                    && emitted_instances.insert(instance_id.clone())
                {
                    active.push(Alert {
                        instance_id,
                        rule_id: rule.id.clone(),
                        target: signal.display_target,
                        metric: rule.metric,
                        severity: rule.severity,
                        value: signal.value,
                        threshold: rule.threshold,
                        active_since_ms,
                    });
                }
            }
        }

        // A removed disk or unavailable sensor must not leave a stale active or
        // pending instance behind. It starts a fresh duration if it reappears.
        self.states.retain(|key, _| seen_instances.contains(key));
        active.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        active
    }

    pub fn clear(&mut self) {
        self.states.clear();
    }
}

fn signals(rule: &AlertRule, snapshot: &SystemSnapshot) -> Vec<Signal> {
    match rule.metric {
        AlertMetric::CpuUsagePercent => snapshot
            .cpu
            .current_global_usage_pct()
            .map(|value| {
                vec![Signal {
                    key_target: "system".into(),
                    display_target: "CPU".into(),
                    value,
                }]
            })
            .unwrap_or_default(),
        AlertMetric::MemoryUsagePercent => snapshot
            .memory
            .used_percentage_observed()
            .map(|value| {
                vec![Signal {
                    key_target: "system".into(),
                    display_target: "Memory".into(),
                    value,
                }]
            })
            .unwrap_or_default(),
        AlertMetric::DiskTemperatureC => {
            disk_signals(rule, &snapshot.disks, |disk| disk.smart_temperature_c)
        }
        AlertMetric::SmartPercentUsed => {
            disk_signals(rule, &snapshot.disks, |disk| disk.smart_percent_used)
        }
        AlertMetric::SmartCriticalWarning => disk_signals(rule, &snapshot.disks, |disk| {
            disk.smart_critical_warning
                .map(|warning| if warning { 1.0 } else { 0.0 })
        }),
    }
}

fn disk_signals(
    rule: &AlertRule,
    disks: &[DiskMetrics],
    value: impl Fn(&DiskMetrics) -> Option<f32>,
) -> Vec<Signal> {
    disks
        .iter()
        .enumerate()
        .filter_map(|(index, disk)| {
            let stable = if !disk.device_id.trim().is_empty() {
                disk.device_id.trim()
            } else if !disk.name.trim().is_empty() {
                disk.name.trim()
            } else {
                return value(disk).map(|metric| Signal {
                    key_target: format!("disk-{index}"),
                    display_target: format!("Disk {}", index + 1),
                    value: metric,
                });
            };
            let matches_target = rule.target.as_ref().is_none_or(|target| {
                target == stable || target == disk.name.trim() || target == disk.device_id.trim()
            });
            matches_target.then(|| {
                value(disk).map(|metric| Signal {
                    key_target: stable.to_string(),
                    display_target: if disk.name.trim().is_empty() {
                        stable.to_string()
                    } else {
                        disk.name.clone()
                    },
                    value: metric,
                })
            })?
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_alerts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/core_core_alerts_disk_signal_tests.rs"]
mod disk_signal_tests;

//! Stable, platform-neutral alert-rule import/export contract.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{AlertMetric, AlertRule, AlertSeverity};

/// Stable discriminator written into alert-rule transfer documents.
pub const ALERT_RULE_FILE_SCHEMA: &str = "taskforest.alert-rules";
/// Latest alert-rule transfer document version supported by this build.
pub const ALERT_RULE_FILE_VERSION: u32 = 1;

const MAX_TRANSFER_BYTES: usize = 1_048_576;
/// Hard ceiling for one alert-rule collection, enforced on imports and on
/// every merged result so the canonical list can never creep past it.
pub const MAX_TRANSFER_RULES: usize = 1_024;
const MAX_RULE_ID_BYTES: usize = 128;
const MAX_TARGET_BYTES: usize = 256;
const MAX_RULE_DURATION_MS: u64 = 3_600_000;

/// One rule in an import/export document, including its UI enabled state.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertRuleTransferEntry {
    pub rule: AlertRule,
    pub enabled: bool,
}

impl AlertRuleTransferEntry {
    pub fn new(rule: AlertRule, enabled: bool) -> Self {
        Self { rule, enabled }
    }
}

/// How an imported rule whose ID already exists is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertRuleConflictPolicy {
    /// Reject the complete import without returning a partially merged result.
    Reject,
    /// Retain the existing rule and ignore the imported rule.
    KeepExisting,
    /// Replace the existing rule in place, retaining the existing list order.
    ReplaceExisting,
}

/// Atomic result of merging a validated import into the current rules.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertRuleMerge {
    pub entries: Vec<AlertRuleTransferEntry>,
    pub added: usize,
    pub replaced: usize,
    pub kept_existing: usize,
}

/// A strict alert-rule transfer or merge failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertRuleTransferError {
    DocumentTooLarge,
    InvalidJson(String),
    UnsupportedSchema(String),
    UnsupportedVersion(u32),
    TooManyRules(usize),
    DuplicateRuleId(String),
    InvalidRule {
        index: usize,
        field: &'static str,
        reason: &'static str,
    },
    Conflict(String),
    Serialization(String),
}

impl fmt::Display for AlertRuleTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DocumentTooLarge => formatter.write_str("alert-rule document is too large"),
            Self::InvalidJson(error) => write!(formatter, "invalid alert-rule JSON: {error}"),
            Self::UnsupportedSchema(schema) => {
                write!(formatter, "unsupported alert-rule schema: {schema}")
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported alert-rule version: {version}")
            }
            Self::TooManyRules(count) => write!(formatter, "too many alert rules: {count}"),
            Self::DuplicateRuleId(id) => write!(formatter, "duplicate alert-rule ID: {id}"),
            Self::InvalidRule {
                index,
                field,
                reason,
            } => write!(
                formatter,
                "invalid alert rule {index} field {field}: {reason}"
            ),
            Self::Conflict(id) => write!(formatter, "alert-rule ID already exists: {id}"),
            Self::Serialization(error) => {
                write!(formatter, "could not serialize alert rules: {error}")
            }
        }
    }
}

impl std::error::Error for AlertRuleTransferError {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlertRuleFileV1 {
    schema: String,
    version: u32,
    rules: Vec<AlertRuleWireV1>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlertRuleWireV1 {
    id: String,
    metric: AlertMetricWireV1,
    severity: AlertSeverityWireV1,
    threshold: f32,
    for_duration_ms: u64,
    hysteresis: f32,
    target: Option<String>,
    enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AlertMetricWireV1 {
    CpuUsagePercent,
    MemoryUsagePercent,
    DiskTemperatureC,
    SmartPercentUsed,
    SmartCriticalWarning,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AlertSeverityWireV1 {
    Info,
    Warning,
    Critical,
}

/// Serialize a complete, versioned alert-rule document.
///
/// Entries are validated exactly like imports, including duplicate IDs. The
/// pretty-printed field order and trailing newline are stable for version 1.
pub fn export_alert_rules_json(
    entries: &[AlertRuleTransferEntry],
) -> Result<String, AlertRuleTransferError> {
    validate_entries(entries)?;
    let document = AlertRuleFileV1 {
        schema: ALERT_RULE_FILE_SCHEMA.to_string(),
        version: ALERT_RULE_FILE_VERSION,
        rules: entries.iter().map(AlertRuleWireV1::from).collect(),
    };
    let mut json = serde_json::to_string_pretty(&document)
        .map_err(|error| AlertRuleTransferError::Serialization(error.to_string()))?;
    json.push('\n');
    Ok(json)
}

/// Parse and strictly validate a versioned alert-rule document.
///
/// Unknown fields, unknown enum values, duplicate IDs, unsupported versions,
/// non-finite/out-of-range numbers, invalid targets, and oversized input are
/// all rejected before any application state is changed.
pub fn import_alert_rules_json(
    json: &str,
) -> Result<Vec<AlertRuleTransferEntry>, AlertRuleTransferError> {
    if json.len() > MAX_TRANSFER_BYTES {
        return Err(AlertRuleTransferError::DocumentTooLarge);
    }
    let document: AlertRuleFileV1 = serde_json::from_str(json)
        .map_err(|error| AlertRuleTransferError::InvalidJson(error.to_string()))?;
    if document.schema != ALERT_RULE_FILE_SCHEMA {
        return Err(AlertRuleTransferError::UnsupportedSchema(document.schema));
    }
    if document.version != ALERT_RULE_FILE_VERSION {
        return Err(AlertRuleTransferError::UnsupportedVersion(document.version));
    }
    let entries: Vec<_> = document.rules.into_iter().map(Into::into).collect();
    validate_entries(&entries)?;
    Ok(entries)
}

/// Merge validated imported rules into validated existing rules atomically.
///
/// New IDs append in import order. Replacements retain the existing rule's
/// position. Under [`AlertRuleConflictPolicy::Reject`], the first collision
/// returns an error and no partially merged collection is exposed.
pub fn merge_alert_rule_entries(
    existing: &[AlertRuleTransferEntry],
    imported: &[AlertRuleTransferEntry],
    policy: AlertRuleConflictPolicy,
) -> Result<AlertRuleMerge, AlertRuleTransferError> {
    validate_entries(existing)?;
    validate_entries(imported)?;

    let mut entries = existing.to_vec();
    let mut positions: HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.rule.id.clone(), index))
        .collect();
    let mut added = 0;
    let mut replaced = 0;
    let mut kept_existing = 0;

    for entry in imported {
        if let Some(index) = positions.get(&entry.rule.id).copied() {
            match policy {
                AlertRuleConflictPolicy::Reject => {
                    return Err(AlertRuleTransferError::Conflict(entry.rule.id.clone()));
                }
                AlertRuleConflictPolicy::KeepExisting => kept_existing += 1,
                AlertRuleConflictPolicy::ReplaceExisting => {
                    entries[index] = entry.clone();
                    replaced += 1;
                }
            }
        } else {
            positions.insert(entry.rule.id.clone(), entries.len());
            entries.push(entry.clone());
            added += 1;
        }
    }

    // The merged collection must satisfy the same ceiling as its inputs:
    // without this re-check, incremental adds could creep past the cap and
    // then freeze every later edit behind a TooManyRules rejection.
    validate_entries(&entries)?;
    Ok(AlertRuleMerge {
        entries,
        added,
        replaced,
        kept_existing,
    })
}

fn validate_entries(entries: &[AlertRuleTransferEntry]) -> Result<(), AlertRuleTransferError> {
    if entries.len() > MAX_TRANSFER_RULES {
        return Err(AlertRuleTransferError::TooManyRules(entries.len()));
    }
    let mut ids = HashSet::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        validate_rule(index, &entry.rule)?;
        if !ids.insert(entry.rule.id.as_str()) {
            return Err(AlertRuleTransferError::DuplicateRuleId(
                entry.rule.id.clone(),
            ));
        }
    }
    Ok(())
}

fn invalid_rule(index: usize, field: &'static str, reason: &'static str) -> AlertRuleTransferError {
    AlertRuleTransferError::InvalidRule {
        index,
        field,
        reason,
    }
}

fn validate_rule(index: usize, rule: &AlertRule) -> Result<(), AlertRuleTransferError> {
    let id = rule.id.as_str();
    if id.is_empty() || id.len() > MAX_RULE_ID_BYTES {
        return Err(invalid_rule(index, "id", "must contain 1 to 128 bytes"));
    }
    if id.trim() != id
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(invalid_rule(
            index,
            "id",
            "must be a portable identifier without surrounding whitespace",
        ));
    }
    if !rule.threshold.is_finite() {
        return Err(invalid_rule(index, "threshold", "must be finite"));
    }
    let maximum = match rule.metric {
        AlertMetric::CpuUsagePercent
        | AlertMetric::MemoryUsagePercent
        | AlertMetric::SmartPercentUsed => 100.0,
        AlertMetric::DiskTemperatureC => 200.0,
        AlertMetric::SmartCriticalWarning => 1.0,
    };
    if !(0.0..=maximum).contains(&rule.threshold) {
        return Err(invalid_rule(
            index,
            "threshold",
            "is outside the metric range",
        ));
    }
    if !rule.hysteresis.is_finite() || rule.hysteresis < 0.0 || rule.hysteresis > rule.threshold {
        return Err(invalid_rule(
            index,
            "hysteresis",
            "must be finite and between zero and threshold",
        ));
    }
    let duration_ms = u64::try_from(rule.for_duration.as_millis())
        .map_err(|_| invalid_rule(index, "for_duration_ms", "is too large"))?;
    if duration_ms > MAX_RULE_DURATION_MS {
        return Err(invalid_rule(
            index,
            "for_duration_ms",
            "must not exceed one hour",
        ));
    }
    match (&rule.metric, rule.target.as_deref()) {
        (AlertMetric::CpuUsagePercent | AlertMetric::MemoryUsagePercent, Some(_)) => {
            return Err(invalid_rule(
                index,
                "target",
                "is only supported by disk metrics",
            ));
        }
        (_, Some(target))
            if target.is_empty()
                || target.len() > MAX_TARGET_BYTES
                || target.trim() != target
                || target.chars().any(char::is_control) =>
        {
            return Err(invalid_rule(
                index,
                "target",
                "must be a trimmed, non-control string of at most 256 bytes",
            ));
        }
        _ => {}
    }
    Ok(())
}

impl From<&AlertRuleTransferEntry> for AlertRuleWireV1 {
    fn from(entry: &AlertRuleTransferEntry) -> Self {
        let duration_ms = u64::try_from(entry.rule.for_duration.as_millis()).unwrap_or(u64::MAX);
        Self {
            id: entry.rule.id.clone(),
            metric: entry.rule.metric.into(),
            severity: entry.rule.severity.into(),
            threshold: entry.rule.threshold,
            for_duration_ms: duration_ms,
            hysteresis: entry.rule.hysteresis,
            target: entry.rule.target.clone(),
            enabled: entry.enabled,
        }
    }
}

impl From<AlertRuleWireV1> for AlertRuleTransferEntry {
    fn from(wire: AlertRuleWireV1) -> Self {
        Self {
            rule: AlertRule {
                id: wire.id,
                metric: wire.metric.into(),
                severity: wire.severity.into(),
                threshold: wire.threshold,
                for_duration: Duration::from_millis(wire.for_duration_ms),
                hysteresis: wire.hysteresis,
                target: wire.target,
            },
            enabled: wire.enabled,
        }
    }
}

impl From<AlertMetric> for AlertMetricWireV1 {
    fn from(metric: AlertMetric) -> Self {
        match metric {
            AlertMetric::CpuUsagePercent => Self::CpuUsagePercent,
            AlertMetric::MemoryUsagePercent => Self::MemoryUsagePercent,
            AlertMetric::DiskTemperatureC => Self::DiskTemperatureC,
            AlertMetric::SmartPercentUsed => Self::SmartPercentUsed,
            AlertMetric::SmartCriticalWarning => Self::SmartCriticalWarning,
        }
    }
}

impl From<AlertMetricWireV1> for AlertMetric {
    fn from(metric: AlertMetricWireV1) -> Self {
        match metric {
            AlertMetricWireV1::CpuUsagePercent => Self::CpuUsagePercent,
            AlertMetricWireV1::MemoryUsagePercent => Self::MemoryUsagePercent,
            AlertMetricWireV1::DiskTemperatureC => Self::DiskTemperatureC,
            AlertMetricWireV1::SmartPercentUsed => Self::SmartPercentUsed,
            AlertMetricWireV1::SmartCriticalWarning => Self::SmartCriticalWarning,
        }
    }
}

impl From<AlertSeverity> for AlertSeverityWireV1 {
    fn from(severity: AlertSeverity) -> Self {
        match severity {
            AlertSeverity::Info => Self::Info,
            AlertSeverity::Warning => Self::Warning,
            AlertSeverity::Critical => Self::Critical,
        }
    }
}

impl From<AlertSeverityWireV1> for AlertSeverity {
    fn from(severity: AlertSeverityWireV1) -> Self {
        match severity {
            AlertSeverityWireV1::Info => Self::Info,
            AlertSeverityWireV1::Warning => Self::Warning,
            AlertSeverityWireV1::Critical => Self::Critical,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_alerts_transfer_tests.rs"]
mod tests;

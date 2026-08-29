//! Versioned export of the bounded alert-transition history.

use std::fmt;

use serde::Serialize;

use super::{AlertEvent, AlertEventKind, MAX_ALERT_EVENTS};

/// Stable document identifier for alert-event exports.
pub const ALERT_EVENT_FILE_SCHEMA: &str = "taskforest-alert-events";
/// Version of the bounded alert-event export document.
pub const ALERT_EVENT_FILE_VERSION: u8 = 1;

/// Errors that keep an alert-event export from claiming a valid document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertEventExportError {
    TooManyEvents(usize),
    ZeroEventId,
    NonMonotonicIds,
    Encode(String),
}

impl fmt::Display for AlertEventExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyEvents(count) => write!(
                formatter,
                "alert event history contains {count} entries; maximum is {MAX_ALERT_EVENTS}"
            ),
            Self::ZeroEventId => formatter.write_str("alert event id must be non-zero"),
            Self::NonMonotonicIds => {
                formatter.write_str("alert event ids must be strictly increasing")
            }
            Self::Encode(error) => {
                write!(formatter, "alert event export encoding failed: {error}")
            }
        }
    }
}

impl std::error::Error for AlertEventExportError {}

#[derive(Serialize)]
struct AlertEventFile<'a> {
    schema: &'static str,
    version: u8,
    events: Vec<AlertEventRecord<'a>>,
}

#[derive(Serialize)]
struct AlertEventRecord<'a> {
    id: u64,
    kind: &'static str,
    observed_at_ms: u64,
    instance_id: &'a str,
    rule_id: &'a str,
    target: &'a str,
    metric: &'static str,
    severity: &'static str,
    value: f32,
    threshold: f32,
    active_since_ms: u64,
}

/// Serialize the shared transition history into a bounded, versioned JSON
/// document. The input is validated rather than silently truncated so a
/// caller cannot export a misleading partial history.
pub fn export_alert_events_json(events: &[AlertEvent]) -> Result<String, AlertEventExportError> {
    if events.len() > MAX_ALERT_EVENTS {
        return Err(AlertEventExportError::TooManyEvents(events.len()));
    }
    if events.iter().any(|event| event.id == 0) {
        return Err(AlertEventExportError::ZeroEventId);
    }
    if events.windows(2).any(|pair| pair[0].id >= pair[1].id) {
        return Err(AlertEventExportError::NonMonotonicIds);
    }
    let events = events
        .iter()
        .map(|event| AlertEventRecord {
            id: event.id,
            kind: match event.kind {
                AlertEventKind::Activated => "activated",
                AlertEventKind::Cleared => "cleared",
            },
            observed_at_ms: event.observed_at_ms,
            instance_id: &event.alert.instance_id,
            rule_id: &event.alert.rule_id,
            target: &event.alert.target,
            metric: metric_name(event.alert.metric),
            severity: severity_name(event.alert.severity),
            value: event.alert.value,
            threshold: event.alert.threshold,
            active_since_ms: event.alert.active_since_ms,
        })
        .collect();
    serde_json::to_string_pretty(&AlertEventFile {
        schema: ALERT_EVENT_FILE_SCHEMA,
        version: ALERT_EVENT_FILE_VERSION,
        events,
    })
    .map_err(|error| AlertEventExportError::Encode(error.to_string()))
}

const fn metric_name(metric: super::AlertMetric) -> &'static str {
    match metric {
        super::AlertMetric::CpuUsagePercent => "cpu_usage_percent",
        super::AlertMetric::MemoryUsagePercent => "memory_usage_percent",
        super::AlertMetric::DiskTemperatureC => "disk_temperature_c",
        super::AlertMetric::SmartPercentUsed => "smart_percent_used",
        super::AlertMetric::SmartCriticalWarning => "smart_critical_warning",
    }
}

const fn severity_name(severity: super::AlertSeverity) -> &'static str {
    match severity {
        super::AlertSeverity::Info => "info",
        super::AlertSeverity::Warning => "warning",
        super::AlertSeverity::Critical => "critical",
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_alerts_events_tests.rs"]
mod tests;

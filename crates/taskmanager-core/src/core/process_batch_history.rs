//! Bounded, platform-neutral audit history for completed multi-process actions.
//!
//! This module deliberately produces strings rather than writing files. UI and
//! CLI callers can hand the deterministic payload to a clipboard, background
//! writer, or another injected sink without blocking the render thread.

use std::fmt;

use serde::Serialize;

use super::FailureKind;
use super::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessBatchResult,
    ProcessBatchTargetResult, ProcessItem, execute_process_batch_with,
    process_batch_failure_wire_code,
};

pub const DEFAULT_PROCESS_BATCH_HISTORY_CAPACITY: usize = 100;
const PROCESS_BATCH_AUDIT_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessBatchHistoryTarget {
    pub identity: FrozenProcessIdentity,
    pub result: ProcessBatchTargetResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessBatchHistoryEntry {
    /// Completion time as milliseconds since the Unix epoch. Keeping the audit
    /// timestamp numeric avoids locale/time-zone dependent exports.
    pub completed_at_unix_ms: u64,
    pub action: ProcessBatchAction,
    pub targets: Vec<ProcessBatchHistoryTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessBatchHistory {
    capacity: usize,
    entries: Vec<ProcessBatchHistoryEntry>,
}

impl Default for ProcessBatchHistory {
    fn default() -> Self {
        Self::new(DEFAULT_PROCESS_BATCH_HISTORY_CAPACITY)
    }
}

impl ProcessBatchHistory {
    #[must_use]
    pub const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries are ordered from oldest to newest. Equal timestamps retain
    /// completion/arrival order, which keeps repeated exports reproducible.
    #[must_use]
    pub fn entries(&self) -> &[ProcessBatchHistoryEntry] {
        &self.entries
    }

    /// Consume a completed worker result and append its frozen identities and
    /// per-target outcomes. Returns false only when history was configured with
    /// zero capacity.
    pub fn record_result(&mut self, completed_at_unix_ms: u64, result: ProcessBatchResult) -> bool {
        if self.capacity == 0 {
            return false;
        }

        let ProcessBatchResult { intent, targets } = result;
        let entry = ProcessBatchHistoryEntry {
            completed_at_unix_ms,
            action: intent.action,
            targets: targets
                .into_iter()
                .map(|(identity, result)| ProcessBatchHistoryTarget { identity, result })
                .collect(),
        };

        if self.entries.len() == self.capacity {
            self.entries.remove(0);
        }
        self.entries.push(entry);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessBatchHistoryFormat {
    Json,
    Csv,
}

#[derive(Debug)]
pub struct ProcessBatchHistoryExportError(serde_json::Error);

impl fmt::Display for ProcessBatchHistoryExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not serialize process batch history: {}",
            self.0
        )
    }
}

impl std::error::Error for ProcessBatchHistoryExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// Execute a frozen batch and atomically feed its completed result into the
/// supplied history before returning it to the caller.
pub fn execute_process_batch_recording_with(
    history: &mut ProcessBatchHistory,
    completed_at_unix_ms: u64,
    intent: ProcessBatchIntent,
    live: &[ProcessItem],
    execute: impl FnMut(ProcessBatchAction, &FrozenProcessIdentity) -> Result<(), FailureKind>,
) -> ProcessBatchResult {
    let result = execute_process_batch_with(intent, live, execute);
    history.record_result(completed_at_unix_ms, result.clone());
    result
}

/// Return a deterministic, UTF-8 audit payload. JSON object fields and CSV rows
/// use fixed ordering; neither format depends on locale, hash-map order, or the
/// caller's time zone.
pub fn export_process_batch_history(
    history: &ProcessBatchHistory,
    format: ProcessBatchHistoryFormat,
) -> Result<String, ProcessBatchHistoryExportError> {
    match format {
        ProcessBatchHistoryFormat::Json => export_json(history),
        ProcessBatchHistoryFormat::Csv => Ok(export_csv(history)),
    }
}

#[derive(Serialize)]
struct AuditDocument<'a> {
    schema_version: u8,
    entries: Vec<AuditEntry<'a>>,
}

#[derive(Serialize)]
struct AuditEntry<'a> {
    completed_at_unix_ms: u64,
    action: AuditAction,
    targets: Vec<AuditTarget<'a>>,
}

#[derive(Serialize)]
struct AuditAction {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<i32>,
}

#[derive(Serialize)]
struct AuditTarget<'a> {
    pid: u32,
    name: &'a str,
    start_time_secs: u64,
    result: AuditResult<'a>,
}

#[derive(Serialize)]
struct AuditResult<'a> {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

fn action_parts(action: ProcessBatchAction) -> (&'static str, Option<i32>) {
    match action {
        ProcessBatchAction::End | ProcessBatchAction::EndProcessTree => ("end", None),
        ProcessBatchAction::Kill => ("kill", None),
        ProcessBatchAction::Suspend => ("suspend", None),
        ProcessBatchAction::Resume => ("resume", None),
        // The export column keeps the legacy numeric contract: the canonical
        // nice value of the tier, not the tier's name.
        ProcessBatchAction::SetPriority(tier) => ("set_priority", Some(tier.canonical_nice())),
    }
}

fn result_parts(result: &ProcessBatchTargetResult) -> (&'static str, Option<&str>) {
    match result {
        ProcessBatchTargetResult::Applied => ("applied", None),
        ProcessBatchTargetResult::IdentityUnavailable => ("identity_unavailable", None),
        ProcessBatchTargetResult::IdentityChanged => ("identity_changed", None),
        ProcessBatchTargetResult::Failed(failure) => {
            ("failed", Some(process_batch_failure_wire_code(*failure)))
        }
    }
}

fn export_json(history: &ProcessBatchHistory) -> Result<String, ProcessBatchHistoryExportError> {
    let entries = history
        .entries()
        .iter()
        .map(|entry| {
            let (kind, priority) = action_parts(entry.action);
            AuditEntry {
                completed_at_unix_ms: entry.completed_at_unix_ms,
                action: AuditAction { kind, priority },
                targets: entry
                    .targets
                    .iter()
                    .map(|target| {
                        let (status, error) = result_parts(&target.result);
                        AuditTarget {
                            pid: target.identity.pid,
                            name: &target.identity.name,
                            start_time_secs: target.identity.start_time_secs,
                            result: AuditResult { status, error },
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    let document = AuditDocument {
        schema_version: PROCESS_BATCH_AUDIT_SCHEMA_VERSION,
        entries,
    };
    let mut payload =
        serde_json::to_string_pretty(&document).map_err(ProcessBatchHistoryExportError)?;
    payload.push('\n');
    Ok(payload)
}

fn export_csv(history: &ProcessBatchHistory) -> String {
    let mut payload = String::from(
        "schema_version,completed_at_unix_ms,action,priority,target_index,target_count,pid,name,start_time_secs,result,error\n",
    );
    for entry in history.entries() {
        let (action, priority) = action_parts(entry.action);
        let priority = priority.map_or_else(String::new, |value| value.to_string());
        let target_count = entry.targets.len().to_string();
        if entry.targets.is_empty() {
            append_csv_row(
                &mut payload,
                &[
                    PROCESS_BATCH_AUDIT_SCHEMA_VERSION.to_string(),
                    entry.completed_at_unix_ms.to_string(),
                    action.to_owned(),
                    priority,
                    String::new(),
                    target_count,
                    String::new(),
                    String::new(),
                    String::new(),
                    "no_targets".to_owned(),
                    String::new(),
                ],
            );
            continue;
        }

        for (index, target) in entry.targets.iter().enumerate() {
            let (result, error) = result_parts(&target.result);
            append_csv_row(
                &mut payload,
                &[
                    PROCESS_BATCH_AUDIT_SCHEMA_VERSION.to_string(),
                    entry.completed_at_unix_ms.to_string(),
                    action.to_owned(),
                    priority.clone(),
                    (index + 1).to_string(),
                    target_count.clone(),
                    target.identity.pid.to_string(),
                    target.identity.name.clone(),
                    target.identity.start_time_secs.to_string(),
                    result.to_owned(),
                    error.unwrap_or_default().to_owned(),
                ],
            );
        }
    }
    payload
}

fn append_csv_row(payload: &mut String, fields: &[String]) {
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            payload.push(',');
        }
        append_csv_field(payload, field);
    }
    payload.push('\n');
}

fn append_csv_field(payload: &mut String, field: &str) {
    if field.contains([',', '"', '\r', '\n']) {
        payload.push('"');
        for character in field.chars() {
            if character == '"' {
                payload.push('"');
            }
            payload.push(character);
        }
        payload.push('"');
    } else {
        payload.push_str(field);
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_process_batch_history_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/core_core_process_batch_history_audit_export_tests.rs"]
mod audit_export_tests;

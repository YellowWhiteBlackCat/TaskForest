//! Toolkit-neutral semantic projection for the Iced frontend.
//!
//! This module consumes the same `taskmanager-ui-contract` snapshot builder as
//! GPUI. It deliberately stops at a validated semantic tree: Iced has no
//! linked native accessibility bridge in this slice, so a snapshot is not
//! reported as an AT-SPI or screen-reader receipt.
//!
//! **Contract-level detached projection, by owner decision (G-15, decision
//! 10, 2026-08-14):** `semantic_snapshot` has NO live-loop call site and must
//! not gain one — wiring an AT-SPI bridge (or even a slow-tick snapshot call)
//! is frozen-domain new functionality. The projection stays reachable through
//! [`crate::IcedApp::semantic_snapshot`] (shell-level facts) and
//! [`crate::IcedApp::semantic_snapshot_with_local`] (shell facts plus the
//! frontend-local alerts route) for tests and contract validation only, so
//! the semantic vocabulary cannot drift from the GPUI surface while the
//! native bridge remains unlinked.

use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_shell::{ProcessRowKey, ShellApp};
use taskmanager_ui_contract::{
    AlertRuleInput, GraphSummary, ModalInput, ProcessRowInput, SemanticSnapshot,
    SemanticSnapshotBuilder,
};

const MAX_PUBLISHED_ROWS: usize = 64;

/// Build the current Iced semantic tree without performing native I/O.
///
/// The projection is bounded, uses the shell's active process ordering and
/// cursor, and retains unavailable CPU/memory values as `Unavailable` in the
/// contract rather than converting them to zero.
#[must_use]
pub fn semantic_snapshot(shell: &ShellApp) -> Option<SemanticSnapshot> {
    base_builder(shell).build().ok()
}

/// Build the semantic tree including the frontend-local routes: while the
/// alerts page is open, the managed rule rows publish as a named group of
/// toggleable switches (name, enabled choice, and the triggering flag folded
/// into the localized detail line). Still a detached projection — no
/// live-loop call site (see the module header).
#[must_use]
pub fn semantic_snapshot_with_local(app: &crate::IcedApp) -> Option<SemanticSnapshot> {
    let builder = base_builder(&app.shell);
    let builder = if app.alerts_page_open() {
        builder.alert_rules(t("alerts.manage"), alert_rule_inputs(app))
    } else {
        builder
    };
    builder.build().ok()
}

/// The shell-level semantic facts (table, graph, status, modal) as a
/// pre-build builder shared by both projections.
fn base_builder(shell: &ShellApp) -> SemanticSnapshotBuilder {
    let mut builder = SemanticSnapshotBuilder::new(shell.projection().refresh_count)
        .application_name(product::ICED_NAME)
        .status_announcement(status_text(shell));

    if let Some(current) = shell
        .projection()
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.cpu.current_global_usage_pct())
        .filter(|value| value.is_finite())
    {
        let current = f64::from(current.clamp(0.0, 100.0));
        builder = builder.cpu_graph(GraphSummary {
            current,
            peak: current,
            maximum: 100.0,
        });
    }

    let memory_total = shell
        .projection()
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.memory.current_total_bytes());
    let processes = shell.projection().processes.as_deref().unwrap_or_default();
    for (index, &raw_index) in shell
        .visible_process_indices()
        .iter()
        .take(MAX_PUBLISHED_ROWS)
        .enumerate()
    {
        let Some(process) = processes.get(raw_index) else {
            continue;
        };
        let name = if process.name.trim().is_empty() {
            String::from("Unnamed process")
        } else {
            process.name.clone()
        };
        builder = builder.process_row(ProcessRowInput {
            id: process.pid.to_string(),
            name,
            cpu_percent: process
                .current_cpu_percentage()
                .filter(|value| value.is_finite())
                .map(|value| f64::from(value.clamp(0.0, 100.0))),
            memory_percent: memory_percentage(process.current_memory_bytes(), memory_total),
            selected: match shell.selected_process_row {
                Some(ProcessRowKey::Process(pid)) => pid == process.pid,
                Some(ProcessRowKey::Application(_) | ProcessRowKey::Category(_)) => false,
                None => index == shell.selected,
            } || shell.selected_pids().contains(&process.pid),
        });
    }

    if let Some(modal) = active_modal(shell) {
        builder = builder.modal(modal);
    }

    builder
}

/// One semantic rule row per managed rule. The managed mirror and the pure
/// row projection share order by construction; the triggering flag comes
/// from the shell's `alert_active` mirror (`rule_id` match), rendered through
/// the shared `alert.triggered` catalog key.
fn alert_rule_inputs(app: &crate::IcedApp) -> Vec<AlertRuleInput> {
    let rows = crate::app::alerts::rule_rows(app);
    app.alerts_rules()
        .iter()
        .zip(rows)
        .map(|(managed, row)| {
            let detail = format!(
                "{} · {} · {}",
                row.severity_label, row.threshold_text, row.current_text
            );
            let triggering = app
                .shell
                .projection()
                .alert_active
                .iter()
                .any(|alert| alert.rule_id == managed.rule.id);
            let detail = if triggering {
                format!("{detail} · {}", t("alert.triggered"))
            } else {
                detail
            };
            AlertRuleInput {
                id: managed.rule.id.clone(),
                name: row.metric_label.clone(),
                enabled: managed.enabled,
                detail: Some(detail),
            }
        })
        .collect()
}

fn memory_percentage(value: Option<u64>, total: Option<u64>) -> Option<f64> {
    let (value, total) = (value?, total.filter(|total| *total > 0)?);
    // This is a bounded display conversion at the semantic edge; source
    // values remain u64 in the shared process and snapshot contracts.
    let percentage = (value as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
    percentage.is_finite().then_some(percentage)
}

fn status_text(shell: &ShellApp) -> String {
    if shell.feedback_text().trim().is_empty() {
        format!("{} processes visible", shell.visible_process_count())
    } else {
        shell.feedback_text().to_owned()
    }
}

fn active_modal(shell: &ShellApp) -> Option<ModalInput> {
    if let Some(target) = shell.pending_end() {
        return Some(ModalInput {
            id: String::from("end-task-confirmation"),
            name: String::from("End task confirmation"),
            description: Some(format!(
                "Confirm the requested action for process {} ({})",
                target.pid, target.name
            )),
        });
    }
    if let Some(target) = shell.pending_service_control() {
        return Some(ModalInput {
            id: String::from("service-control-confirmation"),
            name: String::from("Service control confirmation"),
            description: Some(format!(
                "Confirm the requested {:?} action for service {}",
                target.action, target.service_id
            )),
        });
    }
    if let Some(target) = shell.pending_batch() {
        return Some(ModalInput {
            id: String::from("batch-action-confirmation"),
            name: String::from("Batch action confirmation"),
            description: Some(format!(
                "Confirm the requested action for {} processes",
                target.targets.len()
            )),
        });
    }
    if let Some(target) = shell.pending_startup() {
        return Some(ModalInput {
            id: String::from("startup-action-confirmation"),
            name: String::from("Startup action confirmation"),
            description: Some(format!(
                "Confirm toggling startup item {}",
                target.entry.name
            )),
        });
    }
    if let Some(log) = shell.service_log.as_ref() {
        return Some(ModalInput {
            id: String::from("service-log-modal"),
            name: String::from("Service log modal"),
            description: Some(format!(
                "Live logs for {}",
                log.service_id().map_or("—", |id| id.as_str())
            )),
        });
    }
    if let Some(target) = shell.process_properties_target() {
        return Some(ModalInput {
            id: String::from("process-properties-modal"),
            name: String::from("Process properties modal"),
            description: Some(format!(
                "Properties for {} (PID {})",
                target.name, target.pid
            )),
        });
    }
    if shell.help_open() {
        return Some(ModalInput {
            id: String::from("keyboard-help"),
            name: String::from("Keyboard help"),
            description: Some(String::from("Shared command vocabulary")),
        });
    }
    if shell.suggestions_open() {
        return Some(ModalInput {
            id: String::from("threshold-suggestions"),
            name: String::from(t("alerts.threshold_suggestions")),
            description: Some(String::from("Observed samples only")),
        });
    }
    None
}

#[cfg(test)]
#[path = "../tests/gui/a11y_tests.rs"]
mod tests;

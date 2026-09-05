//! Toolkit-neutral semantic projection for the Iced frontend.
//!
//! This module consumes the same `taskmanager-ui-contract` snapshot builder as
//! GPUI. On Linux the bridge is backed by a real `accesskit_unix::Adapter`
//! (via `taskmanager-accessibility-linux`); on other targets it is the
//! contract's detached bridge.
//!
//! Publication is driven from the periodic refresh loop in `handle_tick_message`
//! so the semantic tree advances on actual data changes rather than on every
//! hover/focus repaint.

use std::cmp::Ordering;
use taskmanager_application::i18n::t;
use taskmanager_application::process_sort::{ProcessSortAxis, compare_processes};
use taskmanager_assets::product;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::{ProcessRowId, ShellApp, process_semantic_key};
use taskmanager_ui_contract::{
    AccessibilityActionRejection, AccessibilityActionRequest, AccessibilityBridge, AlertRuleInput,
    GraphSummary, ModalInput, ProcessRowInput, SemanticAction, SemanticSnapshot,
    SemanticSnapshotBuilder,
};

pub type AppAccessibilityBridge = taskmanager_accessibility_linux::LinuxAccessKitBridge;

const MAX_PUBLISHED_ROWS: usize = 64;

/// Build the current Iced semantic tree without performing native I/O.
#[must_use]
pub fn semantic_snapshot(shell: &ShellApp) -> Option<SemanticSnapshot> {
    base_builder(shell).build().ok()
}

/// Build the semantic tree including the frontend-local routes.
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

/// Build one semantic snapshot from the current view state and publish it
/// to the linked accessibility bridge, then drain any inbound AT action
/// requests.
pub fn publish_accessibility_snapshot(app: &mut crate::IcedApp) {
    app.a11y_revision = app.a11y_revision.wrapping_add(1);
    if app.a11y_bridge.capability().is_ready()
        && let Some(snapshot) = semantic_snapshot_with_local(app)
    {
        if app.a11y_bridge.try_publish(snapshot.clone()).is_ok() {
            app.a11y_snapshot = Some(snapshot);
        } else {
            app.a11y_snapshot = None;
        }
    } else {
        app.a11y_snapshot = None;
    }

    while let Ok(Some(request)) = app.a11y_bridge.try_recv_action() {
        let Some(snapshot) = app.a11y_snapshot.clone() else {
            continue;
        };
        let _ = apply_accessibility_action(app, &request, &snapshot);
    }
}

/// Validate and execute one assistive-technology action against the frozen
/// semantic snapshot.
pub fn apply_accessibility_action(
    app: &mut crate::IcedApp,
    request: &AccessibilityActionRequest,
    snapshot: &SemanticSnapshot,
) -> Result<(), AccessibilityActionRejection> {
    request.validate_against(snapshot)?;

    if let Some(identity) = app.shell.visible_processes().iter().find_map(|process| {
        (format!("row:{}", process_semantic_key(process)) == request.node.as_str())
            .then(|| ProcessLiveKey::from_process(process))
            .flatten()
    }) {
        match request.action {
            SemanticAction::Focus | SemanticAction::Select => {
                let _ = app
                    .shell
                    .apply_action(taskmanager_application::AppAction::SelectPage(
                        taskmanager_application::AppPage::Applications,
                    ));
                let _ = app.shell.select_row_id(ProcessRowId::Process(identity));
                app.sync_visual_cursor();
            }
            _ => {}
        }
        return Ok(());
    }

    if request.action == SemanticAction::Dismiss && request.node.as_str().starts_with("modal:") {
        app.close_local_modals();
        app.close_shell_modals();
    }
    Ok(())
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
    let processes = shell.projection().processes_slice();
    let mut visible_indices: Vec<usize> = shell.visible_process_indices().to_vec();
    visible_indices.sort_by(|&left, &right| {
        let left_p = processes.get(left);
        let right_p = processes.get(right);
        match (left_p, right_p) {
            (Some(l), Some(r)) => compare_processes(l, r, ProcessSortAxis::Cpu, false),
            _ => Ordering::Equal,
        }
    });

    for (index, &raw_index) in visible_indices.iter().take(MAX_PUBLISHED_ROWS).enumerate() {
        let Some(process) = processes.get(raw_index) else {
            continue;
        };
        let name = if process.name.trim().is_empty() {
            String::from("Unnamed process")
        } else {
            process.name.clone()
        };
        builder = builder.process_row(ProcessRowInput {
            id: process_semantic_key(process),
            name,
            cpu_percent: process
                .current_cpu_percentage()
                .filter(|value| value.is_finite())
                .map(|value| f64::from(value.clamp(0.0, 100.0))),
            memory_percent: memory_percentage(process.current_memory_bytes(), memory_total),
            selected: match shell.selected_row {
                Some(ProcessRowId::Process(identity)) => ProcessLiveKey::from_process(process)
                    .is_some_and(|row_identity| row_identity == identity),
                Some(ProcessRowId::Application(_) | ProcessRowId::Category(_)) => false,
                None => index == shell.selected,
            } || shell.is_process_selected(process),
        });
    }

    if let Some(modal) = active_modal(shell) {
        builder = builder.modal(modal);
    }

    builder
}

/// One semantic rule row per managed rule.
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

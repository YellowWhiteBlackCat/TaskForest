//! Accessibility bridge wiring for the root view.
//!
//! Builds the canonical `SemanticSnapshot` from live `RootView` state and
//! pushes it through the linked [`AccessibilityBridge`]. On Linux the bridge is
//! a real `accesskit_unix::Adapter` (see `taskmanager-accessibility-linux`); on
//! other targets it is the contract's `DetachedAccessibilityBridge`, which
//! honestly reports that no native bridge is linked.
//!
//! Publication is driven from the periodic refresh loop in
//! [`startup`](super::startup), not from `render`, so the semantic tree advances
//! on actual data changes rather than on every hover/focus repaint. The Linux
//! adapter's `update_if_active` is a no-op until an assistive technology
//! subscribes, so this is free when no screen reader is running.

use crate::gpui_app::processes_view::rows::aggregate_display::{
    cpu_aggregate_summary, memory_aggregate_summary,
};
use crate::gpui_app::processes_view::rows::projection::category_label;
use taskmanager_application::i18n::t;
use taskmanager_application::process_sort::{ProcessSortAxis, compare_processes};
use taskmanager_assets::product;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_core::core::units::bytes_percent;
use taskmanager_shell::process_semantic_key;
use taskmanager_shell::{
    ProcessProjectionGeneration, ProcessTreeRow, append_process_semantic_rows,
    project_process_tree_rows,
};
use taskmanager_ui_contract::{
    AccessibilityActionRejection, AccessibilityActionRequest, AccessibilityBridge, GraphSummary,
    ModalInput, ProcessRowInput, SemanticAction, SemanticSnapshot, SemanticSnapshotBuilder,
};

use super::RootView;
use taskmanager_application::PendingConfirmation;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::presentation::process_batch_action_label;

/// Maximum number of process rows published to the accessibility tree. The
/// process list can contain thousands of entries; a screen reader reads the
/// tree top-down, so the highest-CPU rows are by far the most useful and the
/// tree is kept bounded for AT responsiveness.
const MAX_PUBLISHED_ROWS: usize = 64;
/// Aggregate rows accompany the bounded process sample so screen readers get
/// the same typed availability and coverage as the visible Applications rows.
const MAX_PUBLISHED_GROUP_ROWS: usize = 64;

fn process_confirmation_copy(
    pending: Option<&PendingConfirmation>,
) -> Option<(String, u32, String, &'static str)> {
    match pending? {
        PendingConfirmation::EndTask(target) => Some((
            t("proc.end_task").to_owned(),
            target.pid,
            target.name.clone(),
            "end-task-confirmation",
        )),
        PendingConfirmation::ProcessBatch(intent) => {
            let target = intent.targets.last().or_else(|| intent.targets.first())?;
            Some((
                process_batch_action_label(intent.action),
                target.pid,
                target.name.clone(),
                "process-batch-confirmation",
            ))
        }
        PendingConfirmation::ServiceControl(_)
        | PendingConfirmation::StartupControl(_)
        | PendingConfirmation::SessionControl(_)
        | PendingConfirmation::SmartSelfTest(_) => None,
    }
}

fn session_confirmation_copy(
    pending: Option<&PendingConfirmation>,
) -> Option<(&'static str, String, String)> {
    let PendingConfirmation::SessionControl(intent) = pending? else {
        return None;
    };
    let action = match intent.action {
        SessionControlAction::Disconnect => t("users.disconnect"),
        SessionControlAction::Lock => t("users.lock"),
    };
    Some((
        action,
        intent.session.id.as_str().to_owned(),
        intent.session.user.clone(),
    ))
}

impl RootView {
    /// Build one semantic snapshot from the current view state and publish it
    /// to the linked accessibility bridge, then drain any inbound AT action
    /// requests. Safe to call on every refresh tick: on Linux the publish is
    /// inert while no AT is subscribed, and on other targets the detached
    /// bridge rejects publication without side effect.
    pub(crate) fn publish_accessibility_snapshot(&mut self) {
        self.a11y_revision = self.a11y_revision.wrapping_add(1);
        let revision = self.a11y_revision;
        // Only build (and O(n log n) sort) the semantic snapshot when an
        // assistive technology is actively subscribed: the bridge's
        // `capability()` reports Ready iff a real consumer is attached, so when
        // no AT is running we skip the per-tick process sort + snapshot
        // allocation entirely. (`try_publish` already no-ops internally without
        // a consumer; this avoids building the snapshot in the first place —
        // the sort was happening every 200ms tick regardless.)
        if self.a11y_bridge.capability().is_ready()
            && let Some(snapshot) = build_snapshot(self, revision)
        {
            if self.a11y_bridge.try_publish(snapshot.clone()).is_ok() {
                self.a11y_snapshot = Some(snapshot);
            } else {
                self.a11y_snapshot = None;
            }
        } else {
            self.a11y_snapshot = None;
        }
        // Drain inbound AT actions so the queue cannot grow unbounded. Every
        // request is checked against the exact snapshot revision before the
        // typed selection/surface action is applied.
        while let Ok(Some(request)) = self.a11y_bridge.try_recv_action() {
            let Some(snapshot) = self.a11y_snapshot.clone() else {
                tracing::debug!(
                    node = %request.node,
                    action = ?request.action,
                    "accessibility action ignored without a published snapshot",
                );
                continue;
            };
            if let Err(rejection) = apply_accessibility_action(self, &request, &snapshot) {
                tracing::debug!(
                    node = %request.node,
                    action = ?request.action,
                    ?rejection,
                    "accessibility action rejected",
                );
            }
        }
    }
}

/// Validate and execute one assistive-technology action against the frozen
/// semantic snapshot. Process rows use the same stable identity helper as the
/// published tree; selecting one is the renderer's focus proxy because the
/// periodic drain has no `Window` handle. Modal dismissal uses the shared
/// surface owner, never a parallel accessibility-only close flag.
pub(crate) fn apply_accessibility_action(
    view: &mut RootView,
    request: &AccessibilityActionRequest,
    snapshot: &SemanticSnapshot,
) -> Result<(), AccessibilityActionRejection> {
    request.validate_against(snapshot)?;

    if let Some(identity) = view.processes().iter().find_map(|process| {
        (format!("row:{}", process_semantic_key(process)) == request.node.as_str())
            .then(|| ProcessLiveKey::from_process(process))
            .flatten()
    }) {
        match request.action {
            SemanticAction::Focus | SemanticAction::Select => {
                view.page = super::TopPage::Apps;
                view.select_process_single(identity);
            }
            SemanticAction::Press
            | SemanticAction::Toggle
            | SemanticAction::Expand
            | SemanticAction::Collapse
            | SemanticAction::Increment
            | SemanticAction::Decrement
            | SemanticAction::SetValue
            | SemanticAction::Dismiss
            | SemanticAction::ReadPreviousValue
            | SemanticAction::ReadNextValue => {}
        }
        return Ok(());
    }

    if request.action == SemanticAction::Dismiss && request.node.as_str().starts_with("modal:") {
        view.dismiss_current_surface(super::WindowSurfaceDismissReason::Escape);
    }
    Ok(())
}

/// Assemble the canonical snapshot from view state. Returns `None` only if the
/// builder rejects the inputs (it never does for the values produced here, but
/// the failure is surfaced honestly rather than panicked).
fn build_snapshot(view: &RootView, revision: u64) -> Option<SemanticSnapshot> {
    let cpu_current = view
        .system_snapshot()
        .cpu
        .current_global_usage_pct()
        .map(|value| f64::from(value.clamp(0.0, 100.0)));
    let memory_total = view.system_snapshot().memory.current_total_bytes();

    // The live region announces the pending confirmation dialog while the
    // semantic builder also publishes a typed modal node below. The status is
    // retained for screen readers that announce live-region changes without
    // moving focus. Data-driven (action + target pid), never widget copy.
    let status =
        if let Some((action, pid, _, _)) = process_confirmation_copy(view.pending_confirmation()) {
            format!("confirming: {action} for {pid}")
        } else if let Some((action, session_id, user)) =
            session_confirmation_copy(view.pending_confirmation())
        {
            format!("confirming: {action} session {session_id} for {user}")
        } else {
            format!("{} processes", view.processes().len())
        };

    let mut builder = SemanticSnapshotBuilder::new(revision)
        .application_name(product::GPUI_NAME)
        .status_announcement(status);
    // The CPU graph is published only when the current scalar is observed.
    // An unavailable first frame is not a measured 0% value.
    if let Some(cpu_current) = cpu_current {
        builder = builder.cpu_graph(GraphSummary {
            current: cpu_current,
            peak: cpu_current,
            maximum: 100.0,
        });
    }

    // Publish aggregate group rows from the canonical shell tree projection.
    // Their accessible row name uses the same formatted CPU/memory state text
    // as the visual cell tooltip, while ProcessAggregateSemantics retains the
    // original typed values and coverage for semantic consumers.
    let processes: Vec<&ProcessItem> = view.processes().iter().collect();
    let group_rows: Vec<ProcessTreeRow> = project_process_tree_rows(
        &processes,
        &view.processes_state.expanded_apps,
        &view.processes_state.collapsed,
        view.effective_process_sort(),
        view.processes_observed_at_ms(),
    )
    .into_iter()
    .filter(|row| {
        matches!(
            row,
            ProcessTreeRow::Category { .. } | ProcessTreeRow::Application { .. }
        )
    })
    .take(MAX_PUBLISHED_GROUP_ROWS)
    .collect();
    if !group_rows.is_empty() {
        builder = append_process_semantic_rows(
            builder,
            ProcessProjectionGeneration::new(view.processes_generation()),
            &group_rows,
            |index| processes.get(index).copied(),
            |row, process| {
                let (name, aggregate) = match row {
                    ProcessTreeRow::Category {
                        category,
                        aggregate,
                        ..
                    } => (category_label(*category).to_owned(), aggregate),
                    ProcessTreeRow::Application { aggregate, .. } => (
                        process.map_or_else(
                            || t("proc.unknown_process").to_owned(),
                            |item| {
                                item.current_application_name()
                                    .unwrap_or(item.name.as_str())
                                    .to_owned()
                            },
                        ),
                        aggregate,
                    ),
                    ProcessTreeRow::Process { .. } => {
                        return t("proc.unknown_process").to_owned();
                    }
                };
                let units = view.display_units();
                let cpu = cpu_aggregate_summary(aggregate.cpu());
                let memory = memory_aggregate_summary(aggregate.memory(), units);
                format!(
                    "{name}; {}: {cpu}; {}: {memory}",
                    t("common.cpu"),
                    t("common.memory")
                )
            },
            |row_id| view.selected_process_row() == Some(row_id),
            None,
        );
    }

    // Publish the highest-CPU rows first; the AT reads them top-down. The
    // ordering is the shared process-sort authority on its CPU axis
    // (`total_cmp` places an unmeasured/NaN sample deterministically, and the
    // direction-independent pid tie-break keeps equal readings stable across
    // refresh ticks), never a local float compare.
    let mut rows: Vec<&ProcessItem> = view.processes().iter().collect();
    rows.sort_by(|left, right| compare_processes(left, right, ProcessSortAxis::Cpu, false));
    for item in rows.iter().take(MAX_PUBLISHED_ROWS) {
        let memory_percent = memory_total.and_then(|total| {
            item.current_memory_bytes()
                .and_then(|value| bytes_percent(value, total))
        });
        builder = builder.process_row(ProcessRowInput {
            id: process_semantic_key(item),
            name: item.name.clone(),
            cpu_percent: item
                .current_cpu_percentage()
                .map(|value| f64::from(value.clamp(0.0, 100.0))),
            memory_percent,
            selected: view.is_process_selected(item),
        });
    }

    if let Some((action, pid, name, id)) = process_confirmation_copy(view.pending_confirmation()) {
        builder = builder.modal(ModalInput {
            id: id.to_owned(),
            name: format!("{action} confirmation"),
            description: Some(format!("Confirm {action} for process {pid} ({name})")),
        });
    }
    if let Some((action, session_id, user)) = session_confirmation_copy(view.pending_confirmation())
    {
        builder = builder.modal(ModalInput {
            id: "session-control-confirmation".to_owned(),
            name: format!("{action} session confirmation"),
            description: Some(format!("Confirm {action} session {session_id} ({user})")),
        });
    }

    builder.build().ok()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_a11y_tests.rs"]
mod tests;

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

use std::cmp::Ordering;

use taskmanager_assets::product;
use taskmanager_ui_contract::{
    AccessibilityBridge, GraphSummary, ModalInput, ProcessRowInput, SemanticSnapshotBuilder,
};

use super::RootView;
use super::termination::ProcessTerminationAction;
use crate::gpui_app::formatting;

/// Maximum number of process rows published to the accessibility tree. The
/// process list can contain thousands of entries; a screen reader reads the
/// tree top-down, so the highest-CPU rows are by far the most useful and the
/// tree is kept bounded for AT responsiveness.
const MAX_PUBLISHED_ROWS: usize = 64;

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
            let _ = self.a11y_bridge.try_publish(snapshot);
        }
        // Drain inbound AT actions so the queue cannot grow unbounded. The
        // bridge fully captures and validates these into the contract's
        // `AccessibilityActionRequest`; dispatching them back into gpui focus
        // and selection state is the next wiring step and is intentionally
        // traced rather than silently dropped.
        while let Ok(Some(request)) = self.a11y_bridge.try_recv_action() {
            tracing::debug!(
                node = %request.node,
                action = ?request.action,
                "accessibility action received from assistive technology",
            );
        }
    }
}

/// Assemble the canonical snapshot from view state. Returns `None` only if the
/// builder rejects the inputs (it never does for the values produced here, but
/// the failure is surfaced honestly rather than panicked).
fn build_snapshot(
    view: &RootView,
    revision: u64,
) -> Option<taskmanager_ui_contract::SemanticSnapshot> {
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
    let status = match view.process_termination_confirmation() {
        Some(intent) => format!(
            "confirming: {} for {}",
            match intent.action {
                ProcessTerminationAction::EndTask => "end task",
                ProcessTerminationAction::ForceKill => "force kill",
                ProcessTerminationAction::EndProcessTree => "end process tree",
            },
            intent.root.pid,
        ),
        None => format!("{} processes", view.processes().len()),
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

    // Publish the highest-CPU rows first; the AT reads them top-down.
    let mut rows: Vec<&crate::core::process::ProcessItem> = view.processes().iter().collect();
    rows.sort_by(|a, b| {
        b.current_cpu_percentage()
            .unwrap_or(0.0)
            .partial_cmp(&a.current_cpu_percentage().unwrap_or(0.0))
            .unwrap_or(Ordering::Equal)
    });
    for item in rows.iter().take(MAX_PUBLISHED_ROWS) {
        let memory_percent = memory_total.filter(|total| *total > 0).and_then(|total| {
            item.current_memory_bytes()
                .map(|value| formatting::bytes_percent(value, total))
        });
        builder = builder.process_row(ProcessRowInput {
            id: item.pid.to_string(),
            name: item.name.clone(),
            cpu_percent: item
                .current_cpu_percentage()
                .map(|value| f64::from(value.clamp(0.0, 100.0))),
            memory_percent,
            selected: view.selected_pids().contains(&item.pid),
        });
    }

    if let Some(intent) = view.process_termination_confirmation() {
        builder = builder.modal(ModalInput {
            id: String::from("end-task-confirmation"),
            name: String::from("End task confirmation"),
            description: Some(format!(
                "Confirm the requested action for process {} ({})",
                intent.root.pid, intent.root.name
            )),
        });
    }

    builder.build().ok()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_a11y_tests.rs"]
mod tests;

//! Process inventory, control-correlation and insight fold systems.

use super::*;

pub(super) fn apply_process_events(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedProcessEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        if matches!(&correlated.event, ProcessEvent::Snapshot(_)) {
            fold.output.process_events.push(correlated.clone());
        }
        match correlated.event {
            ProcessEvent::Snapshot(processes) => {
                store.processes = Some(processes);
                store.processes_observed_at_ms = correlated.observed_at_ms;
                fold.output.changes.processes = true;
                fold.mark_updated();
            }
            ProcessEvent::BatchCompleted(result) => {
                fold.output
                    .batch_results
                    .push((correlated.request_id, result));
                fold.mark_updated();
            }
            ProcessEvent::EndTaskCompleted(target)
            | ProcessEvent::SignalCompleted { target, .. }
            | ProcessEvent::AffinityApplied { target, .. }
            | ProcessEvent::ResourceLimitsApplied { target, .. } => {
                fold.output.process_feedback =
                    store.apply_process_control_completion(correlated.request_id, target);
            }
            ProcessEvent::NetworkCaptureEscalated => {
                if correlated.capability
                    == taskmanager_platform_contract::CapabilityId::PROCESS_NETWORK_ESCALATION
                {
                    fold.output
                        .network_capture_escalations
                        .push(correlated.request_id);
                }
            }
        }
    }
}

pub(super) fn apply_affinity_events(
    events: Vec<taskmanager_application::CorrelatedProcessAffinityEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        let ProcessAffinityEvent::Snapshot { target, cpus } = correlated.event;
        fold.output
            .process_affinity_results
            .push(ProcessAffinityResult {
                request_id: correlated.request_id,
                target,
                cpus,
            });
    }
}

pub(super) fn apply_insight_projections(
    store: &mut SystemProjectionStore,
    projections: Vec<ProjectedProcessInsights>,
    fold: &mut FoldState,
) {
    for projection in projections {
        store.process_insights = Some(projection);
        fold.output.changes.process_insights = true;
        fold.mark_updated();
    }
}

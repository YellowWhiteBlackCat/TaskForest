//! Hardware, service, startup, session and container inventory fold systems.

use super::*;

pub(super) fn apply_hardware(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedHardwareInventoryEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        let HardwareInventoryEvent::Snapshot(snapshot) = correlated.event;
        store.hardware = Some(snapshot.value);
        store.hardware_source = Some(snapshot.sources);
        fold.output.changes.hardware = true;
        fold.mark_updated();
    }
}

pub(super) fn apply_services(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedServiceEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        match correlated.event {
            ServiceEvent::Snapshot(snapshot) => {
                if store.apply_service_snapshot(snapshot) {
                    fold.output.changes.services = true;
                    fold.mark_updated();
                }
            }
            ServiceEvent::Update(update) => {
                let Some(update) = store.apply_service_update(update) else {
                    continue;
                };
                match &update {
                    ServiceUpdate::Action(outcome) => {
                        fold.output.service_control_outcomes.push(outcome.clone());
                        fold.mark_updated();
                    }
                    ServiceUpdate::Logs(_) | ServiceUpdate::LogStream { .. } => {
                        fold.output.service_log_updates.push(update);
                    }
                    ServiceUpdate::Dependencies { .. }
                    | ServiceUpdate::DependenciesUnavailable { .. } => {
                        fold.output.service_updates.push(update);
                    }
                }
            }
        }
    }
}

pub(super) fn apply_containers(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedContainerRollupEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        let ContainerRollupEvent::Snapshot(rollup) = correlated.event;
        store.containers = Some(*rollup);
        fold.output.changes.containers = true;
        fold.mark_updated();
    }
}

pub(super) fn apply_startup_evidence(
    store: &mut SystemProjectionStore,
    projections: Vec<taskmanager_application::ProjectedStartupEvidence>,
    fold: &mut FoldState,
) {
    for projection in projections {
        store.startup_evidence_unavailable = projection.unavailable;
        store.startup_boot_evidence = Some(projection.snapshot);
        fold.output.changes.startup_evidence = true;
        fold.mark_updated();
    }
}

pub(super) fn apply_startup(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedStartupEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        match correlated.event {
            StartupEvent::Snapshot(snapshot) => {
                if store.apply_startup_snapshot(snapshot) {
                    fold.output.changes.startup = true;
                    fold.mark_updated();
                }
            }
            StartupEvent::Control(outcome) => {
                if let Some(outcome) = store.apply_startup_control_outcome(outcome) {
                    fold.output.startup_control_outcomes.push(outcome);
                    fold.mark_updated();
                }
            }
        }
    }
}

pub(super) fn apply_sessions(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedSessionEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        match correlated.event {
            SessionEvent::Snapshot(snapshot) => {
                if store.apply_session_snapshot(snapshot) {
                    fold.output.changes.sessions = true;
                    fold.mark_updated();
                }
            }
            SessionEvent::Control(outcome) => {
                if let Some(outcome) = store.apply_session_control_outcome(outcome) {
                    fold.output.session_control_outcomes.push(outcome);
                    fold.mark_updated();
                }
            }
        }
    }
}

pub(super) fn apply_npu(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedNpuInventoryEvent>,
    fold: &mut FoldState,
) {
    if store.apply_npu_inventory_events(events) {
        fold.output.changes.npu_inventory = true;
        fold.mark_updated();
    }
}

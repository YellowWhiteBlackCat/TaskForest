//! Deterministic platform-batch orchestration for the shared projection store.
//!
//! Each child module owns one independent data family. This module owns only
//! ordering, cross-domain revision advancement, alert evaluation and routed
//! side outputs.

use super::*;
use std::marker::PhantomData;

mod frontend_facts;
mod inventory;
mod on_demand;
mod processes;
mod telemetry;

/// Whether this batch advances the shell-wide update counter.
///
/// This replaces the old mutable `updated` flag with the actual two-state
/// transition. Domain change flags remain independent ECS-style invalidation
/// facts and do not participate in this lifecycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FoldActivity {
    #[default]
    Idle,
    Updated,
}

#[derive(Default)]
struct FoldState {
    activity: FoldActivity,
    output: BatchFoldOutput,
}

impl FoldState {
    fn mark_updated(&mut self) {
        self.activity = FoldActivity::Updated;
    }

    fn is_updated(&self) -> bool {
        matches!(self.activity, FoldActivity::Updated)
    }
}

struct FailureSeedPending;
struct DomainsPending;
struct RevisionsPending;
struct AlertEvaluationPending;
struct FailureFeedbackPending;
struct FoldComplete;

/// Compile-time fold precedence. Only methods on the current phase can produce
/// the next phase, so adding a field to `PlatformEventBatch` cannot silently
/// make struct layout or call-site order decide failure/success precedence.
struct BatchFoldMachine<Phase> {
    batch: PlatformEventBatch,
    fold: FoldState,
    phase: PhantomData<Phase>,
}

impl<Phase> BatchFoldMachine<Phase> {
    fn transition<Next>(self) -> BatchFoldMachine<Next> {
        BatchFoldMachine {
            batch: self.batch,
            fold: self.fold,
            phase: PhantomData,
        }
    }
}

impl BatchFoldMachine<FailureSeedPending> {
    fn new(batch: PlatformEventBatch) -> Self {
        let batch = batch.into_domain_ordered();
        let fold = FoldState {
            output: BatchFoldOutput {
                sensor_events: batch.sensor_events.clone(),
                power_supply_events: batch.power_supply_events.clone(),
                failures: batch.failures.clone(),
                ..BatchFoldOutput::default()
            },
            ..FoldState::default()
        };
        Self {
            batch,
            fold,
            phase: PhantomData,
        }
    }

    fn seed_inventory_failures(
        mut self,
        store: &mut SystemProjectionStore,
    ) -> BatchFoldMachine<DomainsPending> {
        let source_changes = store.apply_inventory_failures(&self.batch.failures);
        self.fold.output.changes.hardware |= source_changes.hardware;
        self.fold.output.changes.services |= source_changes.services;
        self.fold.output.changes.startup |= source_changes.startup;
        self.fold.output.changes.sessions |= source_changes.sessions;
        if source_changes.any() {
            self.fold.mark_updated();
        }
        self.transition()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndependentDomainSystem {
    Telemetry,
    Hardware,
    Processes,
    ProcessAffinity,
    Services,
    ProcessInsights,
    Containers,
    StartupEvidence,
    Startup,
    Sessions,
    DynamicDevices,
    DirectoryUsage,
    GpuEngineRows,
    Npu,
    DesktopAppearance,
    StorageHealth,
    Smart,
}

impl IndependentDomainSystem {
    /// Single production registry. Its order is conventional: every member
    /// must commute with every other member for disjoint batch domains.
    const ALL: [Self; 17] = [
        Self::Telemetry,
        Self::Hardware,
        Self::Processes,
        Self::ProcessAffinity,
        Self::Services,
        Self::ProcessInsights,
        Self::Containers,
        Self::StartupEvidence,
        Self::Startup,
        Self::Sessions,
        Self::DynamicDevices,
        Self::DirectoryUsage,
        Self::GpuEngineRows,
        Self::Npu,
        Self::DesktopAppearance,
        Self::StorageHealth,
        Self::Smart,
    ];

    fn apply(
        self,
        store: &mut SystemProjectionStore,
        batch: &mut PlatformEventBatch,
        fold: &mut FoldState,
    ) {
        match self {
            Self::Telemetry => telemetry::apply_system_telemetry(
                store,
                &batch.system_telemetry_outcomes,
                std::mem::take(&mut batch.system_telemetry_projections),
                fold,
            ),
            Self::Hardware => inventory::apply_hardware(
                store,
                std::mem::take(&mut batch.hardware_inventory_events),
                fold,
            ),
            Self::Processes => processes::apply_process_events(
                store,
                std::mem::take(&mut batch.process_events),
                fold,
            ),
            Self::ProcessAffinity => processes::apply_affinity_events(
                std::mem::take(&mut batch.process_affinity_events),
                fold,
            ),
            Self::Services => {
                inventory::apply_services(store, std::mem::take(&mut batch.service_events), fold)
            }
            Self::ProcessInsights => processes::apply_insight_projections(
                store,
                std::mem::take(&mut batch.process_insight_projections),
                fold,
            ),
            Self::Containers => inventory::apply_containers(
                store,
                std::mem::take(&mut batch.containers_events),
                fold,
            ),
            Self::StartupEvidence => inventory::apply_startup_evidence(
                store,
                std::mem::take(&mut batch.startup_evidence_projections),
                fold,
            ),
            Self::Startup => {
                inventory::apply_startup(store, std::mem::take(&mut batch.startup_events), fold)
            }
            Self::Sessions => {
                inventory::apply_sessions(store, std::mem::take(&mut batch.session_events), fold)
            }
            Self::DynamicDevices => telemetry::apply_dynamic_devices(
                store,
                std::mem::take(&mut batch.sensor_events),
                std::mem::take(&mut batch.power_supply_events),
                fold,
            ),
            Self::DirectoryUsage => on_demand::apply_directory_usage(
                store,
                std::mem::take(&mut batch.directory_usage_events),
                fold,
            ),
            Self::GpuEngineRows => on_demand::apply_gpu_engine_rows(
                store,
                std::mem::take(&mut batch.gpu_engine_rows_events),
                fold,
            ),
            Self::Npu => {
                inventory::apply_npu(store, std::mem::take(&mut batch.npu_inventory_events), fold)
            }
            Self::DesktopAppearance => frontend_facts::apply_desktop_appearance(
                std::mem::take(&mut batch.desktop_appearance_events),
                fold,
            ),
            Self::StorageHealth => frontend_facts::apply_storage_health(
                store,
                std::mem::take(&mut batch.storage_health_events),
                fold,
            ),
            Self::Smart => {
                frontend_facts::apply_smart(store, std::mem::take(&mut batch.smart_events), fold)
            }
        }
    }
}

impl BatchFoldMachine<DomainsPending> {
    fn apply_domains(
        self,
        store: &mut SystemProjectionStore,
    ) -> BatchFoldMachine<RevisionsPending> {
        self.apply_domains_in_order(store, IndependentDomainSystem::ALL)
    }

    fn apply_domains_in_order(
        mut self,
        store: &mut SystemProjectionStore,
        systems: impl IntoIterator<Item = IndependentDomainSystem>,
    ) -> BatchFoldMachine<RevisionsPending> {
        for system in systems {
            system.apply(store, &mut self.batch, &mut self.fold);
        }
        self.transition()
    }
}

impl BatchFoldMachine<RevisionsPending> {
    fn advance_revisions(
        mut self,
        store: &mut SystemProjectionStore,
    ) -> BatchFoldMachine<AlertEvaluationPending> {
        store.advance_batch_revisions(&mut self.fold);
        self.transition()
    }
}

impl BatchFoldMachine<AlertEvaluationPending> {
    fn evaluate_alerts(
        mut self,
        store: &mut SystemProjectionStore,
    ) -> BatchFoldMachine<FailureFeedbackPending> {
        telemetry::evaluate_new_snapshot(store, &mut self.fold);
        self.transition()
    }
}

impl BatchFoldMachine<FailureFeedbackPending> {
    fn apply_failure_feedback(
        mut self,
        store: &mut SystemProjectionStore,
    ) -> BatchFoldMachine<FoldComplete> {
        store.apply_batch_failures(&self.batch.failures, &mut self.fold);
        self.transition()
    }
}

impl BatchFoldMachine<FoldComplete> {
    fn finish(mut self) -> BatchFoldOutput {
        self.fold.output.system_telemetry_outcomes = self.batch.system_telemetry_outcomes;
        self.fold.output.shell_events = self.batch.shell_events;
        self.fold.output.setup_script_events = self.batch.setup_script_events;
        self.fold.output
    }
}

impl SystemProjectionStore {
    #[must_use]
    pub fn sensor_projection(
        &self,
    ) -> Option<(
        &SensorCenterSnapshot,
        &[SourceStatus],
        DynamicDeviceProjectionStamp,
    )> {
        Some((
            self.sensors.as_ref()?,
            self.sensor_source.as_deref().unwrap_or_default(),
            self.sensor_stamp?,
        ))
    }

    #[must_use]
    pub fn power_supply_projection(
        &self,
    ) -> Option<(
        &PowerSupplySnapshot,
        &[SourceStatus],
        DynamicDeviceProjectionStamp,
    )> {
        Some((
            self.power_supplies.as_ref()?,
            self.power_supply_source.as_deref().unwrap_or_default(),
            self.power_supply_stamp?,
        ))
    }

    #[must_use]
    pub fn storage_health_projection(
        &self,
    ) -> Option<(
        &taskmanager_application::FilesystemHealthSnapshot,
        &[SourceStatus],
    )> {
        Some((
            self.storage_health.as_ref()?,
            self.storage_health_source.as_deref().unwrap_or_default(),
        ))
    }

    #[must_use]
    pub const fn smart_projection(
        &self,
    ) -> (
        &SmartObservationProjection,
        Option<&taskmanager_application::StorageDeviceTarget>,
    ) {
        (&self.smart_observations, self.smart_subject.as_ref())
    }

    /// Return the lifecycle of the visible telemetry frame. The pending
    /// projection is deliberately not exposed as a render frame; frontends
    /// keep rendering the last committed snapshot until this becomes `Ready`.
    #[must_use]
    pub const fn telemetry_frame_state(&self) -> TelemetryFrameState {
        if self.snapshot.is_some() {
            TelemetryFrameState::Ready
        } else {
            TelemetryFrameState::Collecting
        }
    }

    /// Fold one platform batch in a fixed system order.
    ///
    /// Source failures are seeded before inventory snapshots so a successful
    /// snapshot in the same batch wins. Alert evaluation runs after all data
    /// systems and revision advancement, once per new committed timestamp.
    #[must_use]
    pub fn apply_platform_batch(&mut self, batch: PlatformEventBatch) -> BatchFoldOutput {
        BatchFoldMachine::new(batch)
            .seed_inventory_failures(self)
            .apply_domains(self)
            .advance_revisions(self)
            .evaluate_alerts(self)
            .apply_failure_feedback(self)
            .finish()
    }

    fn advance_batch_revisions(&mut self, fold: &mut FoldState) {
        let changes = &mut fold.output.changes;
        if changes.processes {
            self.process_revision = self.process_revision.saturating_add(1);
        }
        if changes.services {
            self.services_revision = self.services_revision.saturating_add(1);
        }
        if changes.startup {
            self.startup_revision = self.startup_revision.saturating_add(1);
        }
        if changes.sessions {
            self.sessions_revision = self.sessions_revision.saturating_add(1);
        }
        if changes.hardware || changes.telemetry || changes.containers || changes.dynamic_devices {
            self.system_revision = self.system_revision.saturating_add(1);
            changes.system = true;
        }
        if fold.is_updated() {
            self.refresh_count = self.refresh_count.saturating_add(1);
            fold.output.activity = Some(format!("Live · {} updates", self.refresh_count));
        }
    }

    fn apply_batch_failures(
        &mut self,
        failures: &[taskmanager_application::OperationFailure],
        fold: &mut FoldState,
    ) {
        for failure in failures {
            if let Some(feedback) = self.apply_process_control_failure(failure) {
                fold.output.process_feedback = Some(feedback);
            }
        }
    }

    /// Take the process-list refresh a control completion requested. The
    /// typed request is one-shot and only accepted correlated completions can
    /// arm it.
    #[must_use]
    pub fn take_process_refresh_request(&mut self) -> Option<PlatformEffect> {
        self.process_refresh_request
            .take()
            .map(PlatformEffect::Refresh)
    }

    /// Drain queued desktop notifications. Evaluation lives in the shared
    /// alert center; frontends only route these requests.
    #[must_use]
    pub fn drain_alert_notifications(&mut self) -> Vec<DesktopNotificationRequest> {
        self.pending_notifications.drain(..).collect()
    }
}

#[cfg(test)]
#[path = "../../tests/headless/app/batch_fold_order.rs"]
mod order_tests;

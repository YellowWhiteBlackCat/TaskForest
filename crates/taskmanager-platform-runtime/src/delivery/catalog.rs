//! Runtime capability status catalog updated from provider health.
//!
//! `RuntimeCapabilityCatalog` seeds `CapabilityDescriptor`s from configured
//! routes, folds each `CapabilityHealth` observation into status and
//! last-success timestamps, and serves read snapshots.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use taskmanager_application::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilityRecoveryOutcome,
    CapabilityRecoveryTrigger, CapabilitySnapshot, CapabilityStatus, EventQueueSchedulingSnapshot,
    FailureKind, MAX_PROVIDER_PANIC_MESSAGE_CHARS, MAX_PROVIDER_PANIC_NOTES, ProviderFailure,
    ProviderPanicNote, RuntimeSchedulingSnapshot,
};

use taskmanager_application::CapabilityScheduler;

use crate::config::{CapabilityRoute, RuntimeBudgets};
use crate::delivery::event_queue::EventQueueState;
use crate::ecs::{CompletionRejection, CompletionVerdict, StalledSubject};
use crate::health::CapabilityHealth;

/// Lane and request context for one isolated provider call, consumed by
/// [`ProviderPanicLedger::record`] only when that call panics.
pub(crate) struct ProviderPanicContext {
    pub(crate) lane: String,
    pub(crate) capability: CapabilityId,
    pub(crate) request_id: taskmanager_application::RequestId,
}

/// Bounded memo of provider panics caught by the worker isolation seam.
///
/// A panic already degrades to one typed `ProviderFault` publication; the
/// ledger additionally keeps the downcast payload text with its lane/request
/// context so an operator can see what the typed failure swallowed. It holds
/// at most [`MAX_PROVIDER_PANIC_NOTES`] notes plus one saturating monotone
/// counter, both surfaced through the scheduling snapshot.
pub(crate) struct ProviderPanicLedger {
    notes: Mutex<VecDeque<ProviderPanicNote>>,
    total: AtomicU64,
}

impl ProviderPanicLedger {
    pub(crate) fn new() -> Self {
        Self {
            notes: Mutex::new(VecDeque::new()),
            total: AtomicU64::new(0),
        }
    }

    /// Retain one panic. The monotone counter advances even when the note
    /// lock is poisoned, so the visible count never under-reports panics.
    pub(crate) fn record(&self, context: ProviderPanicContext, message: String) {
        // `fetch_update` yields the value *before* the increment (and the
        // un-incremented current value when the saturating bound is hit), so
        // the 1-based sequence is derived from it, saturating at the bound.
        let sequence = self
            .total
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                (value < u64::MAX).then_some(value + 1)
            })
            .map_or(u64::MAX, |previous| previous.saturating_add(1));
        let mut notes = self
            .notes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        notes.push_back(ProviderPanicNote {
            lane: context.lane,
            capability: context.capability,
            request_id: context.request_id,
            message: bounded_panic_message(message),
            sequence,
        });
        while notes.len() > MAX_PROVIDER_PANIC_NOTES {
            notes.pop_front();
        }
    }

    fn total(&self) -> u64 {
        self.total.load(Ordering::Acquire)
    }

    /// Current ring, oldest first. Snapshot readers clone bounded data only.
    fn recent(&self) -> Vec<ProviderPanicNote> {
        self.notes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

impl Default for ProviderPanicLedger {
    fn default() -> Self {
        Self::new()
    }
}

/// Panic payloads are unbounded text; keep only a bounded prefix so one
/// pathological provider cannot grow the diagnostic tail past its bound.
fn bounded_panic_message(message: String) -> String {
    if message.chars().count() <= MAX_PROVIDER_PANIC_MESSAGE_CHARS {
        return message;
    }
    let mut bounded: String = message
        .chars()
        .take(MAX_PROVIDER_PANIC_MESSAGE_CHARS)
        .collect();
    bounded.push('…');
    bounded
}

pub(crate) struct RuntimeCapabilityCatalog {
    descriptors: RwLock<Vec<CapabilityDescriptor>>,
    terminal_publication: Mutex<()>,
    scheduler: crate::ecs::RuntimeEcsSchedulerHandle,
    event_queues: Arc<EventQueueState>,
    /// Terminal publications tolerated as stale (rejected claim, or health
    /// commit after a delivered terminal). Monotone diagnostic, never a
    /// failure: retiring owners must not stop the publishing lane.
    stale_publications: AtomicU64,
    /// Cumulative provider-lane thread exits observed since construction.
    /// Lanes also exit during runtime teardown; during normal operation any
    /// increment means a lane stopped while the runtime kept running.
    lane_exits: Arc<AtomicU64>,
    /// Bounded provider-panic memo filled by the worker isolation seams.
    panics: Arc<ProviderPanicLedger>,
}

impl RuntimeCapabilityCatalog {
    pub(crate) fn new(routes: &[CapabilityRoute], monotonic_clock_ms: fn() -> u64) -> Self {
        let budgets = RuntimeBudgets::DEFAULT;
        Self::with_resources(
            routes,
            monotonic_clock_ms,
            budgets,
            Arc::new(EventQueueState::new(budgets.pending_delivery_limit)),
        )
    }

    pub(crate) fn with_resources(
        routes: &[CapabilityRoute],
        monotonic_clock_ms: fn() -> u64,
        budgets: RuntimeBudgets,
        event_queues: Arc<EventQueueState>,
    ) -> Self {
        // One capability has one runtime route authority. Preserve the first
        // typed registration deterministically and do not let malformed
        // duplicate construction input inflate the catalog or ECS world.
        let mut by_capability = BTreeMap::new();
        for route in routes {
            by_capability
                .entry(route.capability.clone())
                .or_insert_with(|| CapabilityDescriptor {
                    id: route.capability.clone(),
                    status: CapabilityStatus::TemporarilyUnavailable,
                    providers: vec![route.provider.clone()],
                    observed_at_ms: 0,
                    last_success_at_ms: None,
                });
        }
        let descriptors = by_capability.into_values().collect();
        let scheduler = if budgets == RuntimeBudgets::DEFAULT {
            crate::ecs::RuntimeEcsSchedulerHandle::new(routes, monotonic_clock_ms)
        } else {
            crate::ecs::RuntimeEcsSchedulerHandle::with_budgets(routes, monotonic_clock_ms, budgets)
        };
        Self {
            descriptors: RwLock::new(descriptors),
            terminal_publication: Mutex::new(()),
            scheduler,
            event_queues,
            stale_publications: AtomicU64::new(0),
            lane_exits: Arc::new(AtomicU64::new(0)),
            panics: Arc::new(ProviderPanicLedger::new()),
        }
    }

    pub(crate) fn seed_initial_statuses(
        &self,
        initial_statuses: &BTreeMap<CapabilityId, CapabilityStatus>,
    ) {
        if initial_statuses.is_empty() {
            return;
        }
        let mut descriptors = self
            .descriptors
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for descriptor in descriptors.iter_mut() {
            if let Some(status) = initial_statuses.get(&descriptor.id) {
                descriptor.status = *status;
            }
        }
    }

    pub(crate) fn ecs_scheduler_handle(&self) -> crate::ecs::RuntimeEcsSchedulerHandle {
        self.scheduler.clone()
    }

    pub(crate) fn event_queue_state(&self) -> Arc<EventQueueState> {
        Arc::clone(&self.event_queues)
    }

    /// Count one tolerated stale terminal publication (see the field docs).
    pub(super) fn note_stale_publication(&self) {
        self.stale_publications.fetch_add(1, Ordering::Release);
    }

    /// Shared lane-exit ledger handed to every lane's exit guard.
    pub(super) fn lane_exit_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.lane_exits)
    }

    /// Shared panic-note ledger handed to every lane's isolation seam.
    pub(super) fn provider_panic_ledger(&self) -> Arc<ProviderPanicLedger> {
        Arc::clone(&self.panics)
    }

    /// Serialize terminal visibility with its capability-health commit.
    /// Readers take the same guard, so receiving a terminal event and then
    /// reading the catalog can never observe the pre-terminal descriptor.
    pub(super) fn terminal_publication_guard(&self) -> MutexGuard<'_, ()> {
        self.terminal_publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn renew_target_lease(
        &self,
        capability: &CapabilityId,
        request_id: taskmanager_application::RequestId,
    ) {
        let renewed_at_monotonic_ms = self.scheduler.now_ms();
        if let Ok(mut scheduler) = self.scheduler.lock() {
            let _renewal =
                scheduler.renew_target_lease(capability, request_id, renewed_at_monotonic_ms);
        }
    }

    pub(super) fn record(
        &self,
        capability: &CapabilityId,
        health: CapabilityHealth,
        observed_at_wall_ms: u64,
        request_id: taskmanager_application::RequestId,
    ) -> CompletionVerdict {
        let verdict = match self.scheduler.lock() {
            Ok(mut scheduler) => {
                let monotonic_now_ms = self.scheduler.now_ms();
                scheduler.record_health_for_publication(
                    capability,
                    request_id,
                    health,
                    monotonic_now_ms,
                )
            }
            Err(_) => CompletionVerdict::Rejected(CompletionRejection::InvariantViolation),
        };
        if !verdict.is_accepted() {
            return verdict;
        }

        // Catalog presentation is best-effort after the ECS owner has been
        // validated and retired. A poisoned or otherwise unavailable catalog
        // lock must not resurrect completed lifecycle work.
        if let Ok(mut descriptors) = self.descriptors.write()
            && let Some(descriptor) = descriptors
                .iter_mut()
                .find(|descriptor| descriptor.id == *capability)
        {
            descriptor.observed_at_ms = descriptor.observed_at_ms.max(observed_at_wall_ms);
            match health {
                CapabilityHealth::Available => {
                    descriptor.status = CapabilityStatus::Available;
                    descriptor.last_success_at_ms = Some(
                        descriptor
                            .last_success_at_ms
                            .map_or(observed_at_wall_ms, |old| old.max(observed_at_wall_ms)),
                    );
                }
                CapabilityHealth::Degraded(failure) => {
                    descriptor.status = CapabilityStatus::Degraded(failure);
                    descriptor.last_success_at_ms = Some(
                        descriptor
                            .last_success_at_ms
                            .map_or(observed_at_wall_ms, |old| old.max(observed_at_wall_ms)),
                    );
                }
                CapabilityHealth::Unavailable(error) => {
                    descriptor.status = capability_status(error);
                }
            }
        }
        verdict
    }

    pub(super) fn claim_terminal_delivery(
        &self,
        capability: &CapabilityId,
        request_id: taskmanager_application::RequestId,
    ) -> CompletionVerdict {
        self.scheduler
            .lock()
            .map(|mut scheduler| scheduler.claim_terminal_delivery(capability, request_id))
            .unwrap_or(CompletionVerdict::Rejected(
                CompletionRejection::InvariantViolation,
            ))
    }

    pub(super) fn abort_terminal_delivery(
        &self,
        capability: &CapabilityId,
        request_id: taskmanager_application::RequestId,
    ) {
        if let Ok(mut scheduler) = self.scheduler.lock() {
            let _ = scheduler.abort_terminal_delivery(capability, request_id);
        }
    }

    pub(super) fn acknowledge_terminal_delivery(
        &self,
        capability: &CapabilityId,
        request_id: taskmanager_application::RequestId,
    ) {
        let mut scheduler = self
            .scheduler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = scheduler.acknowledge_terminal_delivery(capability, request_id);
    }
}

#[cfg(test)]
#[path = "../../tests/headless/delivery/catalog.rs"]
mod tests;

impl CapabilityCatalog for RuntimeCapabilityCatalog {
    fn snapshot(&self) -> CapabilitySnapshot {
        let _publication = self.terminal_publication_guard();
        self.descriptors
            .read()
            .map(|descriptors| CapabilitySnapshot::from_descriptors(descriptors.clone()))
            .unwrap_or_default()
    }
}

impl CapabilityScheduler for RuntimeCapabilityCatalog {
    fn poll_due(&self, observed_at_wall_ms: u64) -> Vec<CapabilityId> {
        let Ok(mut scheduler) = self.scheduler.lock() else {
            return Vec::new();
        };
        let monotonic_now_ms = self.scheduler.now_ms();
        let plan = scheduler.tick_plan(monotonic_now_ms);
        drop(scheduler);
        if !plan.stalled.is_empty()
            && let Ok(mut descriptors) = self.descriptors.write()
        {
            for capability in plan.stalled.iter().filter_map(|subject| match subject {
                StalledSubject::Capability { capability, .. } => Some(capability),
                StalledSubject::Target { .. } => None,
            }) {
                if let Some(descriptor) = descriptors
                    .iter_mut()
                    .find(|descriptor| descriptor.id == *capability)
                {
                    descriptor.status = CapabilityStatus::TemporarilyUnavailable;
                    descriptor.observed_at_ms = descriptor.observed_at_ms.max(observed_at_wall_ms);
                }
            }
        }
        plan.items.into_iter().map(|item| item.capability).collect()
    }

    fn mark_submission_failed(&self, capability: &CapabilityId, _failed_at_wall_ms: u64) {
        if let Ok(mut scheduler) = self.scheduler.lock() {
            let failed_at_monotonic_ms = self.scheduler.now_ms();
            let _ = scheduler.requeue_planned_submission(capability, failed_at_monotonic_ms);
        }
    }

    fn set_cadence_ms(&self, capability: &CapabilityId, cadence_ms: Option<u64>) {
        if let Ok(mut scheduler) = self.scheduler.lock() {
            let monotonic_now_ms = self.scheduler.now_ms();
            let _ = scheduler.set_cadence_ms(capability, cadence_ms, monotonic_now_ms);
        }
    }

    fn request_recovery(
        &self,
        capability: &CapabilityId,
        trigger: CapabilityRecoveryTrigger,
    ) -> CapabilityRecoveryOutcome {
        let Ok(mut scheduler) = self.scheduler.lock() else {
            return CapabilityRecoveryOutcome::UnknownCapability;
        };
        let monotonic_now_ms = self.scheduler.now_ms();
        scheduler.request_recovery(capability, trigger, monotonic_now_ms)
    }

    fn scheduling_snapshot(&self) -> RuntimeSchedulingSnapshot {
        let mut snapshot = self
            .scheduler
            .lock()
            .map(|scheduler| scheduler.scheduling_snapshot())
            .unwrap_or_default();
        let pressure = self.event_queues.pressure_snapshot();
        snapshot.event_queues = EventQueueSchedulingSnapshot {
            control_pending: pressure.control_pending as u64,
            control_high_water: pressure.control_high_water as u64,
            observation_pending: pressure.observation_pending as u64,
            observation_high_water: pressure.observation_high_water as u64,
            terminal_mailbox_pending: pressure.terminal_mailbox_pending as u64,
            terminal_mailbox_high_water: pressure.terminal_mailbox_high_water as u64,
        };
        snapshot.stale_terminal_publications = self.stale_publications.load(Ordering::Acquire);
        snapshot.worker_lane_exits = self.lane_exits.load(Ordering::Acquire);
        snapshot.provider_panics = self.panics.total();
        snapshot.recent_provider_panics = self.panics.recent();
        snapshot
    }
}

const fn capability_status(error: ProviderFailure) -> CapabilityStatus {
    match error.kind() {
        FailureKind::Unsupported => CapabilityStatus::Unsupported,
        // RequiresEscalation is an escalatable denial; the capability-status
        // vocabulary has no escalation token, so fold it into PermissionRequired.
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            CapabilityStatus::PermissionRequired
        }
        FailureKind::MissingDependency => CapabilityStatus::MissingDependency,
        FailureKind::TimedOut | FailureKind::TemporarilyUnavailable | FailureKind::Rejected => {
            CapabilityStatus::TemporarilyUnavailable
        }
        FailureKind::IdentityChanged | FailureKind::ProviderFault => CapabilityStatus::Stale,
    }
}

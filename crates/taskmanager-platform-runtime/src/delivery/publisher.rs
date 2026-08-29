//! Correlated event publisher shared by every provider execution lane.
//!
//! `RuntimeEventPublisher` assigns monotone event sequences, records
//! capability health per published outcome, and routes control versus
//! observation events to the matching bounded sender.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Sender, TrySendError};
use taskmanager_application::PlatformEvent;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{
    CapabilityId, EventEnvelope, EventSequence, OperationFailure, ProviderFailure, RequestId,
};

use super::catalog::{ProviderPanicLedger, RuntimeCapabilityCatalog};
use super::event_queue::{EventClass, EventFinality, EventQueueState, QueuedEvent};
use crate::health::CapabilityHealth;

/// What one lane publication means for the publishing lane's own lifetime.
///
/// A rejected or failed publication is a lifecycle fact about one request —
/// stale after a retired or superseded owner — and must never stop the lane
/// that serves every future request. Only a gone event transport (runtime
/// teardown) stops the lane; the defensive enqueue invariant below stays
/// fatal because it is unreachable while the delivery budgets hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaneFlow {
    /// The lane keeps serving requests. The publication either reached a
    /// queue or was tolerated (and counted) as stale.
    #[default]
    Continue,
    /// The runtime's event transport is gone; the lane must stop.
    Stop,
}

impl LaneFlow {
    pub const fn is_stop(self) -> bool {
        matches!(self, Self::Stop)
    }
}

/// Correlated event publisher shared by every provider execution lane.
pub struct RuntimeEventPublisher {
    control_tx: Sender<QueuedEvent>,
    observation_tx: Sender<QueuedEvent>,
    event_queues: Arc<EventQueueState>,
    sequence: SequenceCommitter,
    capabilities: Arc<RuntimeCapabilityCatalog>,
    control_capabilities: Vec<CapabilityId>,
    clock_ms: fn() -> u64,
    lane_exits: Arc<AtomicU64>,
}

impl RuntimeEventPublisher {
    pub(crate) fn new(
        control_tx: Sender<QueuedEvent>,
        observation_tx: Sender<QueuedEvent>,
        capabilities: Arc<RuntimeCapabilityCatalog>,
        control_capabilities: Vec<CapabilityId>,
        clock_ms: fn() -> u64,
    ) -> Self {
        let event_queues = capabilities.event_queue_state();
        Self::with_event_queues(
            control_tx,
            observation_tx,
            event_queues,
            capabilities,
            control_capabilities,
            clock_ms,
        )
    }

    pub(crate) fn with_event_queues(
        control_tx: Sender<QueuedEvent>,
        observation_tx: Sender<QueuedEvent>,
        event_queues: Arc<EventQueueState>,
        capabilities: Arc<RuntimeCapabilityCatalog>,
        control_capabilities: Vec<CapabilityId>,
        clock_ms: fn() -> u64,
    ) -> Self {
        let lane_exits = capabilities.lane_exit_counter();
        Self {
            control_tx,
            observation_tx,
            event_queues,
            sequence: SequenceCommitter::default(),
            capabilities,
            control_capabilities,
            clock_ms,
            lane_exits,
        }
    }

    /// Shared lane-exit ledger for the exit guards installed by every lane
    /// wrapper. The catalog surfaces the same counter in its scheduling
    /// snapshot, so a lane that stopped outside runtime teardown is visible
    /// next to the stalls it leaves behind.
    pub(crate) fn lane_exit_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.lane_exits)
    }

    /// Shared panic-note ledger for the isolation seam around every provider
    /// call this publisher serves. The catalog surfaces the same bounded ring
    /// in its scheduling snapshot next to the lane-exit counter.
    pub(crate) fn panic_ledger(&self) -> Arc<ProviderPanicLedger> {
        self.capabilities.provider_panic_ledger()
    }

    pub(super) fn publish(
        &self,
        request_id: RequestId,
        capability: CapabilityId,
        provider: ProviderId,
        result: Result<PlatformEvent, ProviderFailure>,
    ) -> LaneFlow {
        let observed_at_ms = (self.clock_ms)();
        let result = validate_event_capability(&capability, result);
        let health = CapabilityHealth::from_provider_result(
            result.as_ref().map(|_| ()).map_err(|error| *error),
        );
        self.publish_terminal(
            request_id,
            capability,
            provider,
            result,
            health,
            observed_at_ms,
        )
    }

    pub(super) fn publish_typed_outcome(
        &self,
        request_id: RequestId,
        capability: CapabilityId,
        provider: ProviderId,
        event: PlatformEvent,
        provider_result: Result<(), ProviderFailure>,
    ) -> LaneFlow {
        let observed_at_ms = (self.clock_ms)();
        let (result, health) = if !event.accepts_capability(&capability) {
            let error = ProviderFailure::ProviderFault;
            (Err(error), CapabilityHealth::Unavailable(error))
        } else {
            (
                Ok(event),
                CapabilityHealth::from_provider_result(provider_result),
            )
        };
        self.publish_terminal(
            request_id,
            capability,
            provider,
            result,
            health,
            observed_at_ms,
        )
    }

    pub(crate) fn publish_health(
        &self,
        request_id: RequestId,
        capability: CapabilityId,
        provider: ProviderId,
        event: PlatformEvent,
        health: CapabilityHealth,
    ) -> LaneFlow {
        let observed_at_ms = (self.clock_ms)();
        let (result, health) = if event.accepts_capability(&capability) {
            (Ok(event), health)
        } else {
            let error = ProviderFailure::ProviderFault;
            (Err(error), CapabilityHealth::Unavailable(error))
        };
        self.publish_terminal(
            request_id,
            capability,
            provider,
            result,
            health,
            observed_at_ms,
        )
    }

    /// Publish one intermediate event for an in-flight request (e.g. a scan
    /// progress update) without touching the capability-health record. The
    /// terminal publication of the same request must go through
    /// [`Self::publish_health`] so the catalog reflects the final state.
    pub(crate) fn publish_progress(
        &self,
        request_id: RequestId,
        capability: CapabilityId,
        provider: ProviderId,
        event: PlatformEvent,
    ) -> LaneFlow {
        let observed_at_ms = (self.clock_ms)();
        let result = validate_event_capability(&capability, Ok(event));
        let outcome = self.send(
            request_id,
            capability.clone(),
            provider,
            observed_at_ms,
            result,
        );
        if outcome.is_delivered() {
            self.capabilities
                .renew_target_lease(&capability, request_id);
        }
        if outcome == EnqueueOutcome::Disconnected {
            LaneFlow::Stop
        } else {
            LaneFlow::Continue
        }
    }

    pub(super) fn send(
        &self,
        request_id: RequestId,
        capability: CapabilityId,
        provider: ProviderId,
        observed_at_ms: u64,
        result: Result<PlatformEvent, ProviderFailure>,
    ) -> EnqueueOutcome {
        self.enqueue(
            request_id,
            capability,
            provider,
            observed_at_ms,
            result,
            EventFinality::Progress,
        )
    }

    fn publish_terminal(
        &self,
        request_id: RequestId,
        capability: CapabilityId,
        provider: ProviderId,
        result: Result<PlatformEvent, ProviderFailure>,
        health: CapabilityHealth,
        observed_at_ms: u64,
    ) -> LaneFlow {
        let claim = self
            .capabilities
            .claim_terminal_delivery(&capability, request_id);
        if !claim.is_accepted() {
            // A late publication whose owner was retired (cancelled, or
            // abandoned after its stall deadline) is a counted no-op: the
            // lane keeps serving every other request.
            self.capabilities.note_stale_publication();
            return LaneFlow::Continue;
        }
        let _publication = self.capabilities.terminal_publication_guard();
        let outcome = self.enqueue(
            request_id,
            capability.clone(),
            provider,
            observed_at_ms,
            result,
            EventFinality::Terminal,
        );
        if outcome != EnqueueOutcome::Delivered {
            self.capabilities
                .abort_terminal_delivery(&capability, request_id);
            return LaneFlow::Stop;
        }
        let completion = self
            .capabilities
            .record(&capability, health, observed_at_ms, request_id);
        if !completion.is_accepted() {
            // The terminal event is already queued and will be acknowledged by
            // the consumer; only the health repaint failed. Tolerate and
            // count instead of stopping the lane.
            self.capabilities.note_stale_publication();
        }
        LaneFlow::Continue
    }

    fn enqueue(
        &self,
        request_id: RequestId,
        capability: CapabilityId,
        provider: ProviderId,
        observed_at_ms: u64,
        result: Result<PlatformEvent, ProviderFailure>,
        finality: EventFinality,
    ) -> EnqueueOutcome {
        self.sequence
            .commit(|sequence| {
                let outcome = result.map_err(|error| {
                    operation_failure(
                        request_id,
                        capability.clone(),
                        provider.clone(),
                        sequence,
                        error,
                        observed_at_ms,
                    )
                });
                let class = if self.control_capabilities.contains(&capability) {
                    EventClass::Control
                } else {
                    EventClass::Observation
                };
                let queued = QueuedEvent {
                    envelope: EventEnvelope {
                        request_id,
                        capability,
                        provider: Some(provider),
                        sequence,
                        observed_at_ms,
                        outcome,
                    },
                    finality,
                };
                let sender = match class {
                    EventClass::Control => &self.control_tx,
                    EventClass::Observation => &self.observation_tx,
                };
                self.event_queues.primary_pushed(class);
                match sender.try_send(queued) {
                    Ok(()) => EnqueueOutcome::Delivered,
                    Err(TrySendError::Full(queued)) => {
                        self.event_queues.primary_popped(class);
                        if finality.is_terminal()
                            && self.event_queues.retain_terminal(class, queued)
                        {
                            EnqueueOutcome::Delivered
                        } else if finality.is_terminal() {
                            EnqueueOutcome::InvariantViolation
                        } else {
                            EnqueueOutcome::Coalesced
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        self.event_queues.primary_popped(class);
                        EnqueueOutcome::Disconnected
                    }
                }
            })
            .unwrap_or(EnqueueOutcome::InvariantViolation)
    }
}

#[derive(Default)]
struct SequenceCommitter {
    sequence: Mutex<EventSequence>,
}

impl SequenceCommitter {
    /// Keep sequence allocation and the caller's queue commit in one critical
    /// section, so another publisher cannot make a later sequence visible
    /// before this one reaches primary storage or its retained mailbox.
    fn commit<T>(&self, enqueue: impl FnOnce(EventSequence) -> T) -> Result<T, ()> {
        let Ok(mut sequence) = self.sequence.lock() else {
            return Err(());
        };
        *sequence = sequence.next();
        let result = enqueue(*sequence);
        drop(sequence);
        Ok(result)
    }
}

#[cfg(test)]
#[path = "../../tests/headless/delivery/sequence_commit.rs"]
mod sequence_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EnqueueOutcome {
    Delivered,
    Coalesced,
    Disconnected,
    InvariantViolation,
}

impl EnqueueOutcome {
    pub(super) const fn is_delivered(self) -> bool {
        matches!(self, Self::Delivered)
    }
}

fn validate_event_capability(
    capability: &CapabilityId,
    result: Result<PlatformEvent, ProviderFailure>,
) -> Result<PlatformEvent, ProviderFailure> {
    match result {
        Ok(event) if event.accepts_capability(capability) => Ok(event),
        Ok(_) => Err(ProviderFailure::ProviderFault),
        Err(error) => Err(error),
    }
}

fn operation_failure(
    request_id: RequestId,
    capability: CapabilityId,
    provider: ProviderId,
    sequence: EventSequence,
    error: ProviderFailure,
    observed_at_ms: u64,
) -> OperationFailure {
    OperationFailure {
        request_id,
        capability,
        sequence,
        kind: error.kind(),
        retry: error.retry(),
        provider: Some(provider),
        observed_at_ms,
    }
}

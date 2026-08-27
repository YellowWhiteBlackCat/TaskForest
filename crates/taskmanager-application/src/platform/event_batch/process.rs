use super::super::{ProcessAffinityEvent, ProcessEvent};
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedProcessEvent = CorrelatedEvent<ProcessEvent>;
pub type CorrelatedProcessAffinityEvent = CorrelatedEvent<ProcessAffinityEvent>;

pub(super) fn push_processes(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: ProcessEvent,
) {
    batch
        .process_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_process_affinity(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: ProcessAffinityEvent,
) {
    batch
        .process_affinity_events
        .push(CorrelatedEvent::new(context, event));
}

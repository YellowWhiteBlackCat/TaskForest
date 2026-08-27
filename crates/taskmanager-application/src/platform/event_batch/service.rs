use super::super::ServiceEvent;
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedServiceEvent = CorrelatedEvent<ServiceEvent>;

pub(super) fn push_services(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: ServiceEvent,
) {
    batch
        .service_events
        .push(CorrelatedEvent::new(context, event));
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_event_batch_service_tests.rs"]
mod tests;

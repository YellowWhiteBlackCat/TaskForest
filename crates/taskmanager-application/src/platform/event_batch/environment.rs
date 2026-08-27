//! Correlated startup and session events appended to the bounded `PlatformEventBatch`.

use super::super::{SessionEvent, StartupEvent};
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedStartupEvent = CorrelatedEvent<StartupEvent>;
pub type CorrelatedSessionEvent = CorrelatedEvent<SessionEvent>;

pub(super) fn push_startup(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: StartupEvent,
) {
    batch
        .startup_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_sessions(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: SessionEvent,
) {
    batch
        .session_events
        .push(CorrelatedEvent::new(context, event));
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_event_batch_environment_tests.rs"]
mod tests;

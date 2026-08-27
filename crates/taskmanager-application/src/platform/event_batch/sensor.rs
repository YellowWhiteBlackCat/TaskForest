//! Correlated sensor events appended to the bounded `PlatformEventBatch`.

use super::super::SensorEvent;
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedSensorEvent = CorrelatedEvent<SensorEvent>;

pub(super) fn push_sensors(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: SensorEvent,
) {
    batch
        .sensor_events
        .push(CorrelatedEvent::new(context, event));
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_event_batch_sensor_tests.rs"]
mod tests;

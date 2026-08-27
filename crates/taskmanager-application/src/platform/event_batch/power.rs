//! Correlated power-supply events appended to the bounded `PlatformEventBatch`.

use super::super::PowerSupplyEvent;
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedPowerSupplyEvent = CorrelatedEvent<PowerSupplyEvent>;

pub(super) fn push_power_supplies(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: PowerSupplyEvent,
) {
    batch
        .power_supply_events
        .push(CorrelatedEvent::new(context, event));
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_event_batch_power_tests.rs"]
mod tests;

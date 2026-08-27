//! Correlated system-telemetry, hardware-inventory, and container-rollup event
//! types; hardware-inventory and container events are appended to the bounded
//! `PlatformEventBatch`.

use super::super::{
    ContainerRollupEvent, GpuEngineRowsEvent, HardwareInventoryEvent, NpuInventoryEvent,
    SystemTelemetryDomainOutcome,
};
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedSystemTelemetryOutcome = CorrelatedEvent<SystemTelemetryDomainOutcome>;
pub type CorrelatedHardwareInventoryEvent = CorrelatedEvent<HardwareInventoryEvent>;
pub type CorrelatedGpuEngineRowsEvent = CorrelatedEvent<GpuEngineRowsEvent>;
pub type CorrelatedNpuInventoryEvent = CorrelatedEvent<NpuInventoryEvent>;

pub(super) fn push_hardware_inventory(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: HardwareInventoryEvent,
) {
    batch
        .hardware_inventory_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_containers(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: ContainerRollupEvent,
) {
    batch
        .containers_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_gpu_engine_rows(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: GpuEngineRowsEvent,
) {
    batch
        .gpu_engine_rows_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_npu_inventory(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: NpuInventoryEvent,
) {
    batch
        .npu_inventory_events
        .push(CorrelatedEvent::new(context, event));
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_event_batch_system_tests.rs"]
mod tests;

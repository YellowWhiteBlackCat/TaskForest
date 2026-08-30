//! Correlated system-telemetry, hardware-inventory, and container-rollup event
//! types; hardware-inventory and container events are appended to the bounded
//! `PlatformEventBatch`.

use super::super::{
    ContainerRollupEvent, GpuEngineRowsEvent, HardwareInventoryEvent, MsrReadoutEvent,
    NpuInventoryEvent, RaplPowerEvent, SmbiosMemoryEvent, SystemTelemetryDomainOutcome,
};
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedSystemTelemetryOutcome = CorrelatedEvent<SystemTelemetryDomainOutcome>;
pub type CorrelatedHardwareInventoryEvent = CorrelatedEvent<HardwareInventoryEvent>;
pub type CorrelatedGpuEngineRowsEvent = CorrelatedEvent<GpuEngineRowsEvent>;
pub type CorrelatedNpuInventoryEvent = CorrelatedEvent<NpuInventoryEvent>;
pub type CorrelatedSmbiosMemoryEvent = CorrelatedEvent<SmbiosMemoryEvent>;
pub type CorrelatedRaplPowerEvent = CorrelatedEvent<RaplPowerEvent>;
pub type CorrelatedMsrReadoutEvent = CorrelatedEvent<MsrReadoutEvent>;

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

pub(super) fn push_smbios_memory(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: SmbiosMemoryEvent,
) {
    batch
        .smbios_memory_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_rapl_power(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: RaplPowerEvent,
) {
    batch
        .rapl_power_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_msr_readout(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: MsrReadoutEvent,
) {
    batch
        .msr_readout_events
        .push(CorrelatedEvent::new(context, event));
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_event_batch_system_tests.rs"]
mod tests;

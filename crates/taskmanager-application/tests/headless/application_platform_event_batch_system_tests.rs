use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{CpuMetrics, CpuTelemetryObservation, HardwareInfo, NpuInventorySnapshot};
use taskmanager_platform_contract::{
    CapabilityId, CompositeSourceSnapshot, EventSequence, RequestId,
};

use super::super::super::{
    HardwareInventoryEvent, MsrReadoutEvent, NpuInventoryEvent, PlatformEvent, RaplPowerEvent,
    SmbiosMemoryEvent, SystemTelemetryDomainEvent, SystemTelemetryRevision,
};
use super::super::{PlatformEventBatch, PlatformEventContext, test_support::test_event_context};

#[test]
fn hardware_inventory_event_is_independent_from_telemetry() {
    let mut batch = PlatformEventBatch::default();

    batch.merge(
        test_event_context(
            RequestId::new(2).expect("non-zero fixture request"),
            CapabilityId::HARDWARE_INVENTORY,
        ),
        PlatformEvent::HardwareInventory(HardwareInventoryEvent::Snapshot(Box::new(
            CompositeSourceSnapshot::new(
                HardwareInfo {
                    hostname: Some("fixture-host".into()),
                    ..HardwareInfo::default()
                },
                vec![SourceStatus {
                    provider: ProviderId::borrowed("fixture.hardware.system"),
                    outcome: SourceOutcome::Available,
                    item_count: 1,
                }],
            ),
        ))),
    );

    let event = batch
        .hardware_inventory_events
        .first()
        .expect("hardware event should be retained");
    let HardwareInventoryEvent::Snapshot(snapshot) = &event.event;
    assert_eq!(snapshot.value.hostname.as_deref(), Some("fixture-host"));
    assert!(batch.system_telemetry_outcomes.is_empty());
    assert!(batch.system_telemetry_projections.is_empty());
    assert_eq!(
        snapshot.sources[0].provider.as_str(),
        "fixture.hardware.system"
    );
}

#[test]
fn raw_system_domain_event_is_not_retained_by_the_batch() {
    let request_id = RequestId::new(3).expect("request id");
    let revision = SystemTelemetryRevision::new(9);
    let context = PlatformEventContext {
        request_id,
        capability: CapabilityId::TELEMETRY_CPU,
        provider: Some(ProviderId::borrowed("fixture.cpu")),
        sequence: EventSequence::new(21),
        observed_at_ms: 77,
    };
    let mut batch = PlatformEventBatch::default();

    batch.merge(
        context,
        PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Cpu {
            revision,
            observation: Box::new(CpuTelemetryObservation::current(
                CpuMetrics::default(),
                76,
                Vec::new(),
            )),
        }),
    );

    assert!(batch.is_empty());
}

#[test]
fn npu_only_batch_is_observable_work() {
    let mut batch = PlatformEventBatch::default();
    batch.merge(
        test_event_context(
            RequestId::new(4).expect("non-zero fixture request"),
            CapabilityId::ACCELERATOR_NPU,
        ),
        PlatformEvent::NpuInventory(NpuInventoryEvent::Update(NpuInventorySnapshot::default())),
    );

    assert!(!batch.is_empty());
    assert_eq!(batch.npu_inventory_events.len(), 1);
}

#[test]
fn smbios_memory_update_lands_in_its_own_correlated_lane() {
    let mut batch = PlatformEventBatch::default();
    batch.merge(
        test_event_context(
            RequestId::new(5).expect("non-zero fixture request"),
            CapabilityId::TELEMETRY_MEMORY_SMBIOS,
        ),
        PlatformEvent::SmbiosMemory(SmbiosMemoryEvent::Update(
            taskmanager_core::SmbiosMemorySnapshot::success(4, 2, Vec::new(), None),
        )),
    );

    assert!(!batch.is_empty());
    assert_eq!(batch.smbios_memory_events.len(), 1);
    assert!(batch.rapl_power_events.is_empty());
    assert!(batch.hardware_inventory_events.is_empty());
}

#[test]
fn rapl_power_update_lands_in_its_own_correlated_lane() {
    let mut batch = PlatformEventBatch::default();
    batch.merge(
        test_event_context(
            RequestId::new(6).expect("non-zero fixture request"),
            CapabilityId::TELEMETRY_CPU_PACKAGE_POWER,
        ),
        PlatformEvent::RaplPower(RaplPowerEvent::Update(
            taskmanager_core::RaplPowerSnapshot::success(250, Vec::new()),
        )),
    );

    assert!(!batch.is_empty());
    assert_eq!(batch.rapl_power_events.len(), 1);
    assert!(batch.smbios_memory_events.is_empty());
}

#[test]
fn msr_readout_update_lands_in_its_own_correlated_lane() {
    let mut batch = PlatformEventBatch::default();
    batch.merge(
        test_event_context(
            RequestId::new(7).expect("non-zero fixture request"),
            CapabilityId::TELEMETRY_CPU_MSR,
        ),
        PlatformEvent::MsrReadout(MsrReadoutEvent::Update(
            taskmanager_core::MsrReadoutSnapshot::success(Vec::new()),
        )),
    );

    assert!(!batch.is_empty());
    assert_eq!(batch.msr_readout_events.len(), 1);
    assert!(batch.rapl_power_events.is_empty());
    assert!(batch.smbios_memory_events.is_empty());
}

use taskmanager_core::{
    CpuScalarObservations, CpuTelemetryObservation, DeviceState, DeviceStatus,
    GpuTelemetryObservation, HostRuntimeObservation, MemoryTelemetryObservation,
    NetworkTelemetryObservation, ProcessItem, StorageTelemetryObservation, SystemSnapshot,
    SystemTelemetryDomains, core,
};

#[test]
fn crate_root_and_core_module_keep_the_same_public_types() {
    let root_item = ProcessItem::new(42, "worker");
    let module_item: core::ProcessItem = root_item.clone();

    assert_eq!(module_item, root_item);
    let _: core::SystemSnapshot = SystemSnapshot::default();
    let _: core::CpuScalarObservations = CpuScalarObservations::default();
}

#[test]
fn unavailable_state_never_becomes_a_healthy_zero_value() {
    let healthy = DeviceState::healthy(100);
    let stale = healthy.transition(DeviceStatus::Stale, 200);

    assert_eq!(stale.status, DeviceStatus::Stale);
    assert_eq!(stale.last_success_ms, Some(100));
}

#[test]
fn independent_system_domain_models_are_available_at_the_crate_boundary() {
    let domains = SystemTelemetryDomains {
        host: HostRuntimeObservation::default(),
        cpu: CpuTelemetryObservation::default(),
        memory: MemoryTelemetryObservation::default(),
        storage: StorageTelemetryObservation::default(),
        network: NetworkTelemetryObservation::default(),
        gpu: GpuTelemetryObservation::default(),
    };

    assert!(SystemSnapshot::from_current_domains(&domains).is_none());
}

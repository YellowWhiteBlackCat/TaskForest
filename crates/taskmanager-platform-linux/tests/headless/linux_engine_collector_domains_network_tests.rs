use super::*;
use taskmanager_core::{
    DeviceId, DeviceState, FailureKind, ProviderId, SourceOutcome, SourceStatus,
    SystemObservationState,
};
use taskmanager_platform_contract::{DeviceDiscovery, DeviceSourceSnapshot};

fn partial_network_snapshot(
    _networks: &Networks,
    _state: &mut NetworkCollectionState,
    _now: Instant,
    now_ms: u64,
) -> NetworkDomainSnapshot {
    let device_id = DeviceId::new("network:fixture");
    DeviceSourceSnapshot::from_discovery(
        vec![
            taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                .device_id(device_id.as_str().into())
                .device_state(DeviceState::healthy(now_ms))
                .interface_name("fixture0".into())
                .build(),
        ],
        ProviderId::borrowed("fixture.network.discovery"),
        DeviceDiscovery::Partial {
            discovered_devices: vec![device_id],
            failure: FailureKind::PermissionDenied,
        },
        vec![SourceStatus {
            provider: ProviderId::borrowed("fixture.network.counters"),
            outcome: SourceOutcome::Available,
            item_count: 1,
        }],
    )
}

#[test]
fn observe_preserves_partial_discovery_with_current_devices() {
    let mut collector =
        LinuxNetworkTelemetryCollector::with_domain_collector(partial_network_snapshot);

    let observation = collector.observe(Instant::now(), 77);

    assert_eq!(
        observation.state(),
        SystemObservationState::Partial {
            observed_at_ms: 77,
            failure: FailureKind::PermissionDenied,
        }
    );
    let metrics = observation
        .current_value()
        .expect("partial discovery still carries current discovered devices");
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].device_id.as_ref(), "network:fixture");
    assert_eq!(metrics[0].interface_name.as_ref(), "fixture0");
    assert_eq!(observation.sources().len(), 2);
    assert!(
        observation
            .device_lifecycles()
            .contains_key(&DeviceId::new("network:fixture"))
    );
}

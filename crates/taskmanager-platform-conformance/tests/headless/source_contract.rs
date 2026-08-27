use taskmanager_core::{DeviceId, FailureKind, ProviderId};
use taskmanager_platform_contract::{DeviceDiscovery, DeviceSourceSnapshot};

use super::assert_device_discovery_consistent;

fn provider() -> ProviderId {
    ProviderId::borrowed("fixture.discovery")
}

#[test]
fn constrained_discovery_states_satisfy_the_shared_contract() {
    let available = DeviceSourceSnapshot::from_discovery(
        (),
        provider(),
        DeviceDiscovery::Available(vec![DeviceId::new("b"), DeviceId::new("a")]),
        Vec::new(),
    );
    let empty =
        DeviceSourceSnapshot::from_discovery((), provider(), DeviceDiscovery::Empty, Vec::new());
    let partial = DeviceSourceSnapshot::from_discovery(
        (),
        provider(),
        DeviceDiscovery::Partial {
            discovered_devices: vec![DeviceId::new("a")],
            failure: FailureKind::TemporarilyUnavailable,
        },
        Vec::new(),
    );
    let unavailable = DeviceSourceSnapshot::from_discovery(
        (),
        provider(),
        DeviceDiscovery::Unavailable(FailureKind::MissingDependency),
        Vec::new(),
    );

    for snapshot in [&available, &empty, &partial, &unavailable] {
        assert!(assert_device_discovery_consistent(snapshot).is_ok());
    }
}

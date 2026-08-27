use super::*;
use taskmanager_core::{FailureKind, ProviderId};

#[test]
fn partial_source_failure_does_not_discard_successful_items() {
    let snapshot = PartialSourceSnapshot::new(
        vec!["xdg-entry"],
        vec![
            SourceStatus {
                provider: ProviderId::borrowed("fixture.xdg"),
                outcome: SourceOutcome::Available,
                item_count: 1,
            },
            SourceStatus {
                provider: ProviderId::borrowed("fixture.systemd"),
                outcome: SourceOutcome::Unavailable(FailureKind::TimedOut),
                item_count: 0,
            },
        ],
    );

    assert_eq!(snapshot.items, ["xdg-entry"]);
    assert!(!snapshot.is_authoritative());
}

#[test]
fn composite_source_failure_does_not_discard_successful_fields() {
    let snapshot = CompositeSourceSnapshot::new(
        "hardware",
        vec![
            SourceStatus {
                provider: ProviderId::borrowed("fixture.firmware"),
                outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
                item_count: 0,
            },
            SourceStatus {
                provider: ProviderId::borrowed("fixture.system"),
                outcome: SourceOutcome::Available,
                item_count: 4,
            },
        ],
    );

    assert_eq!(snapshot.value, "hardware");
    assert_eq!(snapshot.sources[0].provider.as_str(), "fixture.firmware");
    assert!(!snapshot.is_authoritative());
}

#[test]
fn device_discovery_authority_is_independent_of_enrichment_failure() {
    let snapshot = DeviceSourceSnapshot::from_source_status(
        vec!["disk0"],
        vec![DeviceId::new("disk0")],
        SourceStatus {
            provider: ProviderId::borrowed("fixture.block.inventory"),
            outcome: SourceOutcome::Available,
            item_count: 1,
        },
        vec![SourceStatus {
            provider: ProviderId::borrowed("fixture.smart"),
            outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            item_count: 0,
        }],
    );

    assert!(snapshot.discovery_is_authoritative());
    let (devices, discovered, sources) = snapshot.into_value_and_sources();
    assert_eq!(devices, ["disk0"]);
    assert_eq!(discovered, [DeviceId::new("disk0")]);
    assert_eq!(sources.len(), 2);
}

#[test]
fn partial_device_discovery_cannot_confirm_absence() {
    let snapshot = DeviceSourceSnapshot::from_source_status(
        vec!["nic0"],
        vec![DeviceId::new("nic0")],
        SourceStatus {
            provider: ProviderId::borrowed("fixture.net.inventory"),
            outcome: SourceOutcome::Partial(FailureKind::PermissionDenied),
            item_count: 1,
        },
        Vec::new(),
    );

    assert!(!snapshot.discovery_is_authoritative());
}

#[test]
fn constrained_device_discovery_derives_ids_outcome_and_count_together() {
    let snapshot = DeviceSourceSnapshot::from_discovery(
        vec!["sensor-a"],
        ProviderId::borrowed("fixture.sensor.inventory"),
        DeviceDiscovery::Partial {
            discovered_devices: vec![DeviceId::new("sensor-a"), DeviceId::new("sensor-a")],
            failure: FailureKind::PermissionDenied,
        },
        Vec::new(),
    );

    assert_eq!(snapshot.discovered_devices(), [DeviceId::new("sensor-a")]);
    assert_eq!(
        snapshot.discovery().outcome,
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(snapshot.discovery().item_count, 1);

    let unavailable = DeviceSourceSnapshot::from_discovery(
        Vec::<String>::new(),
        ProviderId::borrowed("fixture.sensor.inventory"),
        DeviceDiscovery::Unavailable(FailureKind::TimedOut),
        Vec::new(),
    );
    assert!(unavailable.discovered_devices().is_empty());
    assert_eq!(unavailable.discovery().item_count, 0);
}

#[test]
fn empty_available_and_partial_inputs_are_canonicalized() {
    let available = DeviceSourceSnapshot::from_discovery(
        (),
        ProviderId::borrowed("fixture.sensor.inventory"),
        DeviceDiscovery::Available(Vec::new()),
        Vec::new(),
    );
    assert_eq!(available.discovery().outcome, SourceOutcome::Empty);

    let partial = DeviceSourceSnapshot::from_discovery(
        (),
        ProviderId::borrowed("fixture.sensor.inventory"),
        DeviceDiscovery::Partial {
            discovered_devices: Vec::new(),
            failure: FailureKind::PermissionDenied,
        },
        Vec::new(),
    );
    assert_eq!(
        partial.discovery().outcome,
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(partial.discovery().item_count, 0);
}

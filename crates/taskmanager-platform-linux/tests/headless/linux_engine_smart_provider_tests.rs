use super::*;
use crate::engine::hardware::classify_storage_connection;
use crate::engine::smart::transport::{SmartctlDeviceType, smartctl_strategy_for_connection};

#[test]
fn standard_registry_routes_by_protocol_without_vendor_selection() {
    let registry = SmartProviderRegistry::standard();
    let cases = [
        (
            "nvme0n1",
            StorageProtocol::Nvme,
            StorageInterconnect::Pcie,
            "linux.smart.nvme",
        ),
        (
            "sda",
            StorageProtocol::Ata,
            StorageInterconnect::Sata,
            "linux.smart.ata",
        ),
        (
            "sdb",
            StorageProtocol::Scsi,
            StorageInterconnect::Sas,
            "linux.smart.scsi",
        ),
        (
            "sdc",
            StorageProtocol::Scsi,
            StorageInterconnect::Unknown,
            "linux.smart.scsi",
        ),
        (
            "sdd",
            StorageProtocol::Unknown,
            StorageInterconnect::Usb,
            "linux.smart.usb-bridge",
        ),
        (
            "hda",
            StorageProtocol::Ata,
            StorageInterconnect::Ide,
            "linux.smart.ata",
        ),
    ];

    for (name, protocol, interconnect, expected) in cases {
        let request = SmartDeviceRequest {
            name,
            connection: StorageConnection::new(protocol, interconnect, StorageDeviceKind::Physical),
        };
        let selected = registry
            .providers
            .iter()
            .find(|provider| provider.supports(&request))
            .map(|provider| provider.id());
        assert_eq!(selected.as_ref().map(ProviderId::as_str), Some(expected));
    }

    let request = SmartDeviceRequest {
        name: "future0",
        connection: StorageConnection::new(
            StorageProtocol::Unknown,
            StorageInterconnect::FibreChannel,
            StorageDeviceKind::Physical,
        ),
    };
    let selected = registry
        .providers
        .iter()
        .find(|provider| provider.supports(&request))
        .map(|provider| provider.id());
    assert_eq!(
        selected.as_ref().map(ProviderId::as_str),
        Some("linux.smart.auto-detect")
    );
}

#[test]
fn unsupported_connection_has_typed_protocol_failure_and_no_fake_provider() {
    let registry = SmartProviderRegistry::standard();

    let observation = registry.observe(
        "vda",
        StorageConnection::new(
            StorageProtocol::Unknown,
            StorageInterconnect::Virtio,
            StorageDeviceKind::Virtual,
        ),
    );

    assert_eq!(
        observation.value.failure,
        Some(SmartProviderFailureKind::UnsupportedProtocol)
    );
    assert!(observation.value.provider.is_none());
    assert_eq!(
        observation.source.outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(observation.source.item_count, 0);
}

#[test]
fn usb_bridge_routing_preserves_tunneled_protocol_without_vendor_selection() {
    let registry = SmartProviderRegistry::standard();
    for (name, protocol) in [
        ("future0", StorageProtocol::Ata),
        ("future0", StorageProtocol::Scsi),
        ("nvme8n1", StorageProtocol::Nvme),
        ("future0", StorageProtocol::Unknown),
    ] {
        let request = SmartDeviceRequest {
            name,
            connection: StorageConnection::new(
                protocol,
                StorageInterconnect::Usb,
                taskmanager_core::StorageDeviceKind::Physical,
            ),
        };
        let selected = registry
            .providers
            .iter()
            .find(|provider| provider.supports(&request))
            .map(|provider| provider.id());
        assert_eq!(
            selected.as_ref().map(ProviderId::as_str),
            Some("linux.smart.usb-bridge")
        );
    }
}

#[test]
fn mmc_and_ufs_remain_typed_unsupported_without_fake_health() {
    let registry = SmartProviderRegistry::standard();
    for (protocol, interconnect) in [
        (StorageProtocol::Mmc, StorageInterconnect::Mmc),
        (StorageProtocol::Ufs, StorageInterconnect::Ufs),
    ] {
        let observation = registry.observe(
            "future0",
            StorageConnection::new(protocol, interconnect, StorageDeviceKind::Physical),
        );
        assert_eq!(
            observation.value.failure,
            Some(SmartProviderFailureKind::UnsupportedProtocol)
        );
        assert!(observation.value.provider.is_none());
    }
}

#[test]
fn auto_detect_never_claims_virtual_aggregate_or_network_storage() {
    let registry = SmartProviderRegistry::standard();
    for (interconnect, device_kind) in [
        (StorageInterconnect::Virtio, StorageDeviceKind::Virtual),
        (StorageInterconnect::Platform, StorageDeviceKind::Aggregate),
        (StorageInterconnect::Network, StorageDeviceKind::Virtual),
    ] {
        let request = SmartDeviceRequest {
            name: "future0",
            connection: StorageConnection::new(StorageProtocol::Unknown, interconnect, device_kind),
        };
        assert!(
            registry
                .providers
                .iter()
                .all(|provider| !provider.supports(&request)),
            "{interconnect:?}/{device_kind:?} must not enter auto-detect"
        );
    }
}

#[test]
fn discovery_connection_drives_registry_and_plan_without_name_guessing() {
    let registry = SmartProviderRegistry::standard();
    let cases = [
        (
            "sda",
            classify_storage_connection(
                "sda",
                None,
                None,
                Some("scsi"),
                Some("/devices/pci0000:00/0000:00:17.0/ata1/host0/target0:0:0/block/sda"),
            ),
            "linux.smart.ata",
            vec![SmartctlDeviceType::Auto, SmartctlDeviceType::Sat],
        ),
        (
            "future-usb",
            classify_storage_connection(
                "future-usb",
                Some("usb"),
                Some("ata"),
                Some("scsi"),
                Some("/devices/pci/usb/usb2/2-1/host0/target0/block/future-usb"),
            ),
            "linux.smart.usb-bridge",
            vec![
                SmartctlDeviceType::Auto,
                SmartctlDeviceType::Sat,
                SmartctlDeviceType::Scsi,
            ],
        ),
    ];

    for (name, connection, expected_provider, expected_strategy) in cases {
        let request = SmartDeviceRequest { name, connection };
        let selected = registry
            .providers
            .iter()
            .find(|provider| provider.supports(&request))
            .map(|provider| provider.id());
        assert_eq!(
            selected.as_ref().map(ProviderId::as_str),
            Some(expected_provider)
        );
        assert_eq!(
            smartctl_strategy_for_connection(connection),
            expected_strategy
        );
    }

    let contradictory = classify_storage_connection(
        "nvme7n1",
        Some("pcie"),
        Some("ata"),
        Some("nvme"),
        Some("/devices/pci/nvme/nvme7/nvme7n1"),
    );
    let request = SmartDeviceRequest {
        name: "nvme7n1",
        connection: contradictory,
    };
    assert!(
        registry
            .providers
            .iter()
            .all(|provider| !provider.supports(&request)),
        "contradictory discovery evidence must not reach a native command plan"
    );
    assert!(smartctl_strategy_for_connection(contradictory).is_empty());
}

#[test]
fn smart_failure_reasons_map_to_typed_enrichment_status() {
    let cases = [
        (
            SmartProviderFailureKind::MissingTool,
            FailureKind::MissingDependency,
        ),
        (
            SmartProviderFailureKind::PermissionDenied,
            FailureKind::PermissionDenied,
        ),
        (SmartProviderFailureKind::TimedOut, FailureKind::TimedOut),
        (
            SmartProviderFailureKind::MalformedResponse,
            FailureKind::ProviderFault,
        ),
        (
            SmartProviderFailureKind::TemporarilyUnavailable,
            FailureKind::TemporarilyUnavailable,
        ),
        (
            SmartProviderFailureKind::DeviceUnavailable,
            FailureKind::TemporarilyUnavailable,
        ),
        (
            SmartProviderFailureKind::CommandFailed,
            FailureKind::ProviderFault,
        ),
        (
            SmartProviderFailureKind::BridgeLimitation,
            FailureKind::Unsupported,
        ),
    ];
    for (smart_failure, source_failure) in cases {
        let observation = DiskSmart::with_failure(smart_failure);
        assert_eq!(
            smart_source_status(ProviderId::borrowed("fixture.smart"), &observation).outcome,
            SourceOutcome::Unavailable(source_failure)
        );
    }
}

#[test]
fn available_smart_fields_with_optional_failure_are_partial_not_unavailable() {
    let observation = DiskSmart {
        availability: SmartAvailability::Available,
        temperature_c: Some(37.0),
        failure: Some(SmartProviderFailureKind::MalformedResponse),
        ..Default::default()
    };

    let status = smart_source_status(ProviderId::borrowed("fixture.smart"), &observation);

    assert_eq!(
        status.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(status.item_count, 1);
}

#[test]
fn mixed_protocol_results_aggregate_without_becoming_discovery_truth() {
    let sources = aggregate_smart_sources([
        SourceStatus {
            provider: ProviderId::borrowed("linux.smart.ata"),
            outcome: SourceOutcome::Available,
            item_count: 1,
        },
        SourceStatus {
            provider: ProviderId::borrowed("linux.smart.ata"),
            outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            item_count: 0,
        },
        SourceStatus {
            provider: ProviderId::borrowed("linux.smart.scsi"),
            outcome: SourceOutcome::Unavailable(FailureKind::MissingDependency),
            item_count: 0,
        },
    ]);

    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].provider.as_str(), "linux.smart.ata");
    assert_eq!(
        sources[0].outcome,
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(sources[0].item_count, 1);
    assert_eq!(sources[1].provider.as_str(), "linux.smart.scsi");
    assert_eq!(
        sources[1].outcome,
        SourceOutcome::Unavailable(FailureKind::MissingDependency)
    );
}

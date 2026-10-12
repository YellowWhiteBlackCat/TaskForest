use super::*;

fn request_args(kind: &str) -> Vec<String> {
    [
        "--start-self-test",
        "disk:wwid:controller-serial",
        "2",
        "nvme0n1",
        "nvme",
        "pcie",
        kind,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn parse_fixed_request(kind: &str) -> Result<Request, Failure> {
    parse_request(request_args(kind).into_iter().map(std::ffi::OsString::from))
}

#[test]
fn fixed_request_parses_only_the_three_supported_operation_kinds() {
    let short = parse_fixed_request("short").expect("fixed short request");
    assert_eq!(short.kind, SmartSelfTestKind::Short);
    assert_eq!(
        short.target.device_id.as_str(),
        "disk:wwid:controller-serial"
    );
    assert_eq!(short.target.device_generation.get(), 2);
    assert_eq!(short.connection.protocol, StorageProtocol::Nvme);

    let extended = parse_fixed_request("extended").expect("fixed extended request");
    assert_eq!(extended.kind, SmartSelfTestKind::Extended);
    let conveyance = parse_fixed_request("conveyance").expect("fixed conveyance request");
    assert_eq!(conveyance.kind, SmartSelfTestKind::Conveyance);
    assert!(parse_fixed_request("vendor,0x99").is_err());
}

#[test]
fn target_validation_rejects_attachment_identity_and_untrusted_names() {
    assert!(is_persistent_disk_id("disk:wwid:fixture"));
    assert!(is_persistent_disk_id("disk:serial:fixture"));
    assert!(!is_persistent_disk_id("disk:path:nvme0n1"));
    assert_eq!(kernel_name("/dev/nvme0n1"), Some("nvme0n1"));
    for name in ["", "../sda", "--all", "sda/1"] {
        assert_eq!(kernel_name(name), None, "{name}");
    }
}

#[test]
fn mutation_strategy_is_narrower_than_read_only_usb_strategy() {
    let usb = StorageConnection::new(
        StorageProtocol::Nvme,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    );
    assert_eq!(smartctl_strategy(usb).len(), 4);

    let remote = StorageConnection::new(
        StorageProtocol::Scsi,
        StorageInterconnect::Iscsi,
        StorageDeviceKind::Physical,
    );
    assert!(smartctl_strategy(remote).is_empty());

    let virtual_disk = StorageConnection::new(
        StorageProtocol::Ata,
        StorageInterconnect::Sata,
        StorageDeviceKind::Virtual,
    );
    assert!(smartctl_strategy(virtual_disk).is_empty());
}

#[test]
fn capability_probe_requires_exact_device_evidence_for_the_requested_kind() {
    let nvme = serde_json::json!({
        "nvme_optional_admin_commands": {"self_test": true}
    });
    assert_eq!(
        self_test_support(&nvme, SmartSelfTestKind::Short),
        DeviceSupport::Supported
    );
    assert_eq!(
        self_test_support(&nvme, SmartSelfTestKind::Extended),
        DeviceSupport::Supported
    );
    assert_eq!(
        self_test_support(&nvme, SmartSelfTestKind::Conveyance),
        DeviceSupport::Unsupported
    );

    let ata = serde_json::json!({
        "ata_smart_data": {
            "capabilities": {
                "self_tests_supported": true,
                "conveyance_self_test_supported": false
            }
        }
    });
    assert_eq!(
        self_test_support(&ata, SmartSelfTestKind::Short),
        DeviceSupport::Supported
    );
    assert_eq!(
        self_test_support(&ata, SmartSelfTestKind::Conveyance),
        DeviceSupport::Unsupported
    );

    let health_only = serde_json::json!({"smart_status": {"passed": true}});
    assert_eq!(
        self_test_support(&health_only, SmartSelfTestKind::Short),
        DeviceSupport::Unknown
    );
}

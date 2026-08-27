//! Smartctl transport parsing tests (line split).

use super::*;

mod tests_inner {
    use std::cell::{Cell, RefCell};

    use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
    use taskmanager_core::core::smart::refresh_state;

    use super::*;

    const ATA_HEALTHY_FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/smartctl_ata_healthy.json"
    ));
    const ATA_FAILED_ATTRIBUTES_FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/smartctl_ata_failed_attributes_only.json"
    ));
    const ATA_FAILURE_ATTRIBUTES_FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/smartctl_ata_failure_attributes.json"
    ));

    #[test]
    fn device_path_accepts_future_whole_disks_and_rejects_untrusted_shapes() {
        assert_eq!(smartctl_device_path("sda").as_deref(), Some("/dev/sda"));
        assert_eq!(
            smartctl_device_path("/dev/sdaa").as_deref(),
            Some("/dev/sdaa")
        );
        assert_eq!(
            smartctl_device_path("future0").as_deref(),
            Some("/dev/future0")
        );
        assert_eq!(
            smartctl_device_path("future0").as_deref(),
            Some("/dev/future0"),
            "unknown transport must not become a kernel-name vendor allowlist"
        );
        for name in ["", "../sda", "--scan", "device/name"] {
            assert_eq!(smartctl_device_path(name), None, "{name}");
        }
    }

    #[test]
    fn fixtures_map_shared_health_fields_and_attribute_fallbacks() {
        let healthy = parse_smartctl_json(ATA_HEALTHY_FIXTURE).expect("ATA fields parsed");
        assert_eq!(healthy.availability, SmartAvailability::Available);
        assert_eq!(healthy.temperature_c, Some(31.0));
        assert_eq!(healthy.critical_warning, Some(false));
        assert_eq!(healthy.power_on_hours, Some(12_876));

        let failed =
            parse_smartctl_json(ATA_FAILED_ATTRIBUTES_FIXTURE).expect("ATA fallback fields");
        assert_eq!(failed.temperature_c, Some(47.0));
        assert_eq!(failed.critical_warning, Some(true));
        assert_eq!(failed.power_on_hours, Some(54_321));
    }

    #[test]
    fn ata_failure_attributes_table_is_typed_and_preserves_signals() {
        use taskmanager_core::core::smart::AtaSmartAttribute;

        let smart = parse_smartctl_json(ATA_FAILURE_ATTRIBUTES_FIXTURE)
            .expect("ATA failure fixture parsed");
        let table = smart
            .ata_attributes
            .as_ref()
            .expect("ATA attribute table is populated");
        // The parser keeps every well-formed row, not just the failure ids, so
        // the storage view can later surface the full attribute set.
        assert_eq!(table.len(), 6);

        let find = |id: u16| -> &AtaSmartAttribute {
            table.iter().find(|attr| attr.id == id).unwrap_or_else(|| {
                panic!("expected ATA attribute id {id} to be present in parsed table")
            })
        };

        // Actionable failure precedursors: reallocated (5), current pending
        // (197), offline uncorrectable (198), command timeout / CRC (199).
        let reallocated = find(5);
        assert_eq!(reallocated.raw_value, 10);
        assert!(
            reallocated.failing_now,
            "id 5 with `failed: true` must parse as failing now"
        );

        let pending = find(197);
        assert_eq!(pending.raw_value, 4);
        assert!(
            !pending.failing_now,
            "id 197 `failing_now: false` must hold"
        );

        let offline = find(198);
        assert_eq!(offline.raw_value, 7);
        assert!(
            offline.failing_now,
            "id 198 with `failing_now: true` must parse as failing now"
        );

        let crc = find(199);
        assert_eq!(crc.raw_value, 2);
        // No boolean flag is present for id 199; the canonical `when_failed:
        // "now"` string must still mark the attribute as failing, matching
        // real smartctl output across schema versions.
        assert!(
            crc.failing_now,
            "id 199 with `when_failed: \"now\"` must fall back to failing now"
        );

        // Standard fields continue to parse alongside the new attribute table.
        assert_eq!(smart.temperature_c, Some(45.0));
        assert_eq!(smart.critical_warning, Some(true));
        assert_eq!(smart.power_on_hours, Some(30_015));
    }

    #[test]
    fn ata_attributes_absent_when_device_has_no_attribute_table() {
        let nvme = parse_smartctl_json(
            r#"{
                "smart_status": {"passed": true},
                "temperature": {"current": 40}
            }"#,
        )
        .expect("NVMe-style fields parsed");
        assert!(
            nvme.ata_attributes.is_none(),
            "a device without an ATA attribute table must not synthesize one"
        );
    }

    #[test]
    fn parser_rejects_malformed_or_unrecognised_json() {
        assert!(parse_smartctl_json("").is_none());
        assert!(parse_smartctl_json("not json").is_none());
        assert!(parse_smartctl_json(r#"{"smartctl":{"exit_status":0}}"#).is_none());
        assert!(parse_smartctl_json(r#"{"temperature":{"current":"hot"}}"#).is_none());
    }

    #[test]
    fn command_failures_and_invalid_names_degrade_without_execution() {
        let requested_device = Cell::new(None);
        let out = read_smartctl_with("sda", unknown_connection(), |device, device_type| {
            requested_device.set(Some(device.to_owned()));
            assert_eq!(device_type, SmartctlDeviceType::Auto);
            SmartCommandResult::Unavailable
        });
        assert_eq!(requested_device.take().as_deref(), Some("/dev/sda"));
        assert_eq!(out.availability, SmartAvailability::Unavailable);

        let called = Cell::new(false);
        let invalid = read_smartctl_with("../sda", unknown_connection(), |_, _| {
            called.set(true);
            SmartCommandResult::Output(ATA_HEALTHY_FIXTURE.to_owned())
        });
        assert!(!called.get());
        assert_eq!(invalid.availability, SmartAvailability::Unsupported);
    }

    #[test]
    fn provider_failures_are_distinct_and_state_recovers() {
        let previous = DeviceState::healthy(100);
        let mut missing = read_smartctl_with("sda", unknown_connection(), |_, _| {
            SmartCommandResult::MissingTool
        });
        refresh_state(previous, &mut missing, 200);
        assert_eq!(missing.state.status, DeviceStatus::MissingTool);

        let mut denied = read_smartctl_with("sda", unknown_connection(), |_, _| {
            SmartCommandResult::PermissionDenied
        });
        refresh_state(missing.state, &mut denied, 300);
        assert_eq!(denied.availability, SmartAvailability::PermissionDenied);
        assert_eq!(denied.state.last_success_ms, Some(100));

        let mut recovered = read_smartctl_with("sda", unknown_connection(), |_, _| {
            SmartCommandResult::Output(ATA_HEALTHY_FIXTURE.into())
        });
        refresh_state(denied.state, &mut recovered, 400);
        assert_eq!(recovered.state, DeviceState::healthy(400));
    }

    #[test]
    fn strategy_is_protocol_based_and_vendor_agnostic() {
        assert_eq!(
            smartctl_strategy(usb_connection()),
            &[
                SmartctlDeviceType::Auto,
                SmartctlDeviceType::Sat,
                SmartctlDeviceType::Scsi,
                SmartctlDeviceType::SntAsmedia,
                SmartctlDeviceType::SntJmicron,
                SmartctlDeviceType::SntRealtek,
            ]
        );
        assert_eq!(
            smartctl_strategy(sas_connection()),
            &[SmartctlDeviceType::Auto, SmartctlDeviceType::Scsi]
        );
        assert_eq!(
            smartctl_strategy(sata_connection()),
            &[SmartctlDeviceType::Auto, SmartctlDeviceType::Sat]
        );
        assert_eq!(
            smartctl_strategy(nvme_connection()),
            &[SmartctlDeviceType::Auto]
        );
        assert!(smartctl_strategy(virtio_connection()).is_empty());

        assert_eq!(
            smartctl_strategy_for_connection(StorageConnection::new(
                StorageProtocol::Ata,
                StorageInterconnect::Usb,
                taskmanager_core::StorageDeviceKind::Physical,
            )),
            &[
                SmartctlDeviceType::Auto,
                SmartctlDeviceType::Sat,
                SmartctlDeviceType::Scsi,
            ]
        );
        assert_eq!(
            smartctl_strategy_for_connection(StorageConnection::new(
                StorageProtocol::Scsi,
                StorageInterconnect::Usb,
                taskmanager_core::StorageDeviceKind::Physical,
            )),
            &[
                SmartctlDeviceType::Auto,
                SmartctlDeviceType::Scsi,
                SmartctlDeviceType::Sat,
            ]
        );
        assert_eq!(
            smartctl_strategy_for_connection(StorageConnection::new(
                StorageProtocol::Nvme,
                StorageInterconnect::Usb,
                taskmanager_core::StorageDeviceKind::Physical,
            )),
            &[
                SmartctlDeviceType::Auto,
                SmartctlDeviceType::SntAsmedia,
                SmartctlDeviceType::SntJmicron,
                SmartctlDeviceType::SntRealtek,
            ]
        );
    }

    #[test]
    fn usb_bridge_retries_only_type_mismatch_and_stops_on_terminal_failures() {
        let requested = RefCell::new(Vec::new());
        let out = read_smartctl_with("sda", usb_connection(), |_, device_type| {
            requested.borrow_mut().push(device_type);
            if device_type == SmartctlDeviceType::Auto {
                SmartCommandResult::RetryableDeviceType
            } else {
                SmartCommandResult::Output(ATA_HEALTHY_FIXTURE.into())
            }
        });
        assert_eq!(out.availability, SmartAvailability::Available);
        assert_eq!(
            requested.into_inner(),
            vec![SmartctlDeviceType::Auto, SmartctlDeviceType::Sat]
        );

        let calls = Cell::new(0);
        let denied = read_smartctl_with("sda", usb_connection(), |_, _| {
            calls.set(calls.get() + 1);
            SmartCommandResult::PermissionDenied
        });
        assert_eq!(calls.get(), 1);
        assert_eq!(denied.availability, SmartAvailability::PermissionDenied);
    }

    #[test]
    fn exhausted_usb_device_types_report_bridge_limitation_not_generic_absence() {
        let out = read_smartctl_with("sda", usb_connection(), |_, _| {
            SmartCommandResult::RetryableDeviceType
        });

        assert_eq!(out.availability, SmartAvailability::Unsupported);
        assert_eq!(
            out.failure,
            Some(SmartProviderFailureKind::BridgeLimitation)
        );
    }

    #[test]
    fn exhausted_usb_nvme_translation_types_report_bridge_limitation() {
        let connection = StorageConnection::new(
            StorageProtocol::Nvme,
            StorageInterconnect::Usb,
            taskmanager_core::StorageDeviceKind::Physical,
        );
        let requested = RefCell::new(Vec::new());
        let out = read_smartctl_with_connection("future0", connection, |_, device_type| {
            requested.borrow_mut().push(device_type);
            SmartCommandResult::RetryableDeviceType
        });

        assert_eq!(
            requested.into_inner(),
            vec![
                SmartctlDeviceType::Auto,
                SmartctlDeviceType::SntAsmedia,
                SmartctlDeviceType::SntJmicron,
                SmartctlDeviceType::SntRealtek,
            ]
        );
        assert_eq!(
            out.failure,
            Some(SmartProviderFailureKind::BridgeLimitation)
        );
    }

    #[test]
    fn scsi_endurance_indicator_maps_without_vendor_attributes() {
        let out = parse_smartctl_json(
            r#"{
                "smart_status": {"passed": true},
                "temperature": {"current": 35},
                "scsi_percentage_used_endurance_indicator": 7
            }"#,
        )
        .expect("standard SCSI SMART fields");
        assert_eq!(out.percent_used, Some(7.0));
        assert_eq!(out.critical_warning, Some(false));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn exit_bits_keep_health_results_but_reject_command_failures() {
        assert!(smartctl_exit_allows_data(0));
        assert!(smartctl_exit_allows_data(1 << 3));
        assert!(smartctl_exit_allows_data((1 << 3) | (1 << 4)));
        assert!(!smartctl_exit_allows_data(1));
        assert!(!smartctl_exit_allows_data(2));
        assert!(!smartctl_exit_allows_data(4));
        assert!(!smartctl_exit_allows_data(7));
    }

    #[test]
    fn stderr_recognises_device_type_requests() {
        assert!(stderr_requests_device_type(
            b"Unknown USB bridge [0x1234:0x5678], please specify device type"
        ));
        assert!(!stderr_requests_device_type(b"device not found"));
    }

    #[test]
    fn json_messages_drive_bridge_retry_but_arbitrary_stdout_never_does() {
        let bridge = br#"{
            "smartctl": {
                "messages": [{
                    "string": "Unknown USB bridge, please specify device type with the -d option",
                    "severity": "error"
                }]
            }
        }"#;
        assert_eq!(
            classify_smartctl_command_diagnostic(bridge, &[]),
            Some(SmartctlCommandDiagnostic::DeviceTypeRequired)
        );
        assert_eq!(
            classify_smartctl_command_diagnostic(
                b"Unknown USB bridge, please specify device type",
                &[]
            ),
            None,
            "unstructured stdout must not authorize another device-type attempt"
        );
    }

    #[test]
    fn terminal_json_diagnostics_take_priority_over_type_retry() {
        let denied = br#"{
            "smartctl": {
                "messages": [
                    {"string": "please specify device type", "severity": "error"},
                    {"string": "open failed: Permission denied", "severity": "error"}
                ]
            }
        }"#;
        assert_eq!(
            classify_smartctl_command_diagnostic(denied, &[]),
            Some(SmartctlCommandDiagnostic::PermissionDenied)
        );

        let gone = br#"{
            "smartctl": {
                "messages": [
                    {"string": "unknown USB bridge", "severity": "error"},
                    {"string": "open failed: No such device", "severity": "error"}
                ]
            }
        }"#;
        assert_eq!(
            classify_smartctl_command_diagnostic(gone, &[]),
            Some(SmartctlCommandDiagnostic::DeviceUnavailable)
        );
        assert_eq!(
            smartctl_diagnostic_result(SmartctlCommandDiagnostic::DeviceUnavailable),
            SmartCommandResult::DeviceUnavailable
        );
    }

    #[test]
    fn unknown_structured_error_is_terminal_command_failure() {
        let output = br#"{
            "smartctl": {
                "messages": [{
                    "string": "provider-specific command failure",
                    "severity": "error"
                }]
            }
        }"#;
        assert_eq!(
            classify_smartctl_command_diagnostic(output, &[]),
            Some(SmartctlCommandDiagnostic::CommandFailure)
        );
        assert_eq!(
            smartctl_diagnostic_result(SmartctlCommandDiagnostic::CommandFailure),
            SmartCommandResult::CommandFailed
        );
    }

    #[test]
    fn explicit_no_smart_support_is_typed_unsupported_not_malformed() {
        let connection = StorageConnection::new(
            StorageProtocol::Ata,
            StorageInterconnect::Sata,
            StorageDeviceKind::Physical,
        );
        let out = read_smartctl_with_connection("sda", connection, |_, _| {
            SmartCommandResult::Output(
                r#"{
                    "smart_support":{"available":false},
                    "temperature":{"current":31},
                    "power_on_time":{"hours":123}
                }"#
                .to_owned(),
            )
        });
        assert_eq!(
            out.failure,
            Some(SmartProviderFailureKind::UnsupportedProtocol)
        );
        assert_eq!(out.availability, SmartAvailability::Unsupported);
    }
}

fn smartctl_strategy(connection: StorageConnection) -> &'static [SmartctlDeviceType] {
    super::smartctl_strategy_for_connection(connection)
}

fn read_smartctl_with(
    name: &str,
    connection: StorageConnection,
    fetch: impl FnMut(&str, SmartctlDeviceType) -> SmartCommandResult,
) -> DiskSmart {
    super::read_smartctl_with_connection(name, connection, fetch)
}

const fn unknown_connection() -> StorageConnection {
    StorageConnection::new(
        StorageProtocol::Unknown,
        StorageInterconnect::Unknown,
        StorageDeviceKind::Physical,
    )
}

const fn usb_connection() -> StorageConnection {
    StorageConnection::new(
        StorageProtocol::Unknown,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    )
}

const fn sas_connection() -> StorageConnection {
    StorageConnection::new(
        StorageProtocol::Scsi,
        StorageInterconnect::Sas,
        StorageDeviceKind::Physical,
    )
}

const fn sata_connection() -> StorageConnection {
    StorageConnection::new(
        StorageProtocol::Ata,
        StorageInterconnect::Sata,
        StorageDeviceKind::Physical,
    )
}

const fn nvme_connection() -> StorageConnection {
    StorageConnection::new(
        StorageProtocol::Nvme,
        StorageInterconnect::Pcie,
        StorageDeviceKind::Physical,
    )
}

const fn virtio_connection() -> StorageConnection {
    StorageConnection::new(
        StorageProtocol::Unknown,
        StorageInterconnect::Virtio,
        StorageDeviceKind::Virtual,
    )
}

fn stderr_requests_device_type(stderr: &[u8]) -> bool {
    super::diagnostic::command_output_requests_device_type(&[], stderr)
}

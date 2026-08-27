use std::cell::{Cell, RefCell};

use taskmanager_core::{StorageDeviceKind, StorageInterconnect, StorageProtocol};

use crate::engine::smart::transport::smartctl_strategy_for_connection;

use super::*;

const ATA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/smartctl_selftest_ata.json"
));
const NVME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/smartctl_selftest_nvme.json"
));

#[test]
fn provider_escalation_remains_an_actionable_smart_failure() {
    assert_eq!(
        provider_execution_failure(ProviderFailure::RequiresEscalation),
        SmartSelfTestFailure::RequiresEscalation
    );
}

#[test]
fn plans_preserve_validated_intent_and_reject_untrusted_names() {
    let plan = smart_self_test_plan("sda", SmartSelfTestKind::Short).unwrap();
    assert_eq!(plan.disk_name(), "sda");
    assert_eq!(plan.kind(), SmartSelfTestKind::Short);
    assert!(smart_self_test_plan("sda1", SmartSelfTestKind::Short).is_err());
    assert!(smart_self_test_plan("../sda", SmartSelfTestKind::Short).is_err());
    assert!(smart_self_test_plan("--scan", SmartSelfTestKind::Short).is_err());
    assert!(smart_self_test_plan("nvme0n1", SmartSelfTestKind::Extended).is_ok());
    assert!(smart_self_test_plan("nvme0pp", SmartSelfTestKind::Short).is_err());
}

#[test]
fn connection_strategy_retries_only_type_mismatch_and_is_bounded() {
    let connection = StorageConnection::new(
        StorageProtocol::Nvme,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    );
    let attempts = RefCell::new(Vec::new());
    let revalidations = Cell::new(0);
    let resolved = execute_for_connection(
        "future0",
        connection,
        || {
            revalidations.set(revalidations.get() + 1);
            Ok(())
        },
        |device, device_type| {
            assert_eq!(device, "/dev/future0");
            attempts.borrow_mut().push(device_type);
            if device_type == SmartctlDeviceType::SntJmicron {
                StrategyAttempt::Success("resolved")
            } else {
                StrategyAttempt::RetryableDeviceType
            }
        },
    );
    assert_eq!(resolved, Ok("resolved"));
    assert_eq!(revalidations.get(), 3);
    assert_eq!(
        attempts.into_inner(),
        vec![
            SmartctlDeviceType::Auto,
            SmartctlDeviceType::SntAsmedia,
            SmartctlDeviceType::SntJmicron,
        ]
    );

    let terminal_attempts = Cell::new(0);
    let denied = execute_for_connection(
        "future0",
        connection,
        || Ok(()),
        |_, _| {
            terminal_attempts.set(terminal_attempts.get() + 1);
            StrategyAttempt::<()>::Failed(SmartSelfTestFailure::PermissionDenied)
        },
    );
    assert_eq!(
        denied,
        Err(StrategyFailure::Report(
            SmartSelfTestFailure::PermissionDenied
        ))
    );
    assert_eq!(terminal_attempts.get(), 1);
}

#[test]
fn start_and_poll_revalidate_immediately_before_every_native_attempt() {
    let connection = StorageConnection::new(
        StorageProtocol::Ata,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    );
    let start_revalidations = Cell::new(0);
    let start_attempts = Cell::new(0);
    let start = execute_start_strategy(
        "sda",
        connection,
        || {
            start_revalidations.set(start_revalidations.get() + 1);
            Ok(())
        },
        |_, _| {
            assert_eq!(start_revalidations.get(), start_attempts.get() + 1);
            start_attempts.set(start_attempts.get() + 1);
            StrategyAttempt::<()>::RetryableDeviceType
        },
    );
    assert_eq!(
        start,
        Err(StrategyFailure::Provider(ProviderFailure::Unsupported))
    );
    assert_eq!(start_revalidations.get(), start_attempts.get());
    assert_eq!(start_attempts.get(), 3);

    let poll_revalidations = Cell::new(0);
    let poll_attempts = Cell::new(0);
    let poll = execute_poll_strategy(
        "sda",
        connection,
        || {
            poll_revalidations.set(poll_revalidations.get() + 1);
            Ok(())
        },
        |_, _| {
            assert_eq!(poll_revalidations.get(), poll_attempts.get() + 1);
            poll_attempts.set(poll_attempts.get() + 1);
            StrategyAttempt::<()>::RetryableDeviceType
        },
    );
    assert_eq!(
        poll,
        Err(StrategyFailure::Provider(ProviderFailure::Unsupported))
    );
    assert_eq!(poll_revalidations.get(), poll_attempts.get());
    assert_eq!(poll_attempts.get(), 3);
}

#[test]
fn unsupported_protocol_and_exhausted_usb_bridge_are_provider_failures() {
    let mmc = StorageConnection::new(
        StorageProtocol::Mmc,
        StorageInterconnect::Mmc,
        StorageDeviceKind::Physical,
    );
    let attempts = Cell::new(0);
    let unsupported = execute_for_connection(
        "mmcblk0",
        mmc,
        || Ok(()),
        |_, _| {
            attempts.set(attempts.get() + 1);
            StrategyAttempt::Success(())
        },
    );
    assert_eq!(
        unsupported,
        Err(StrategyFailure::Provider(ProviderFailure::Unsupported))
    );
    assert_eq!(attempts.get(), 0);

    let usb = StorageConnection::new(
        StorageProtocol::Nvme,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    );
    let exhausted = execute_for_connection(
        "future0",
        usb,
        || Ok(()),
        |_, _| StrategyAttempt::<()>::RetryableDeviceType,
    );
    assert_eq!(
        exhausted,
        Err(StrategyFailure::Provider(ProviderFailure::Unsupported))
    );
}

#[test]
fn read_only_remote_smart_plan_does_not_authorize_a_self_test_mutation() {
    for interconnect in [
        StorageInterconnect::FibreChannel,
        StorageInterconnect::Iscsi,
        StorageInterconnect::Network,
        StorageInterconnect::Other,
        StorageInterconnect::Unknown,
    ] {
        let connection = StorageConnection::new(
            StorageProtocol::Unknown,
            interconnect,
            StorageDeviceKind::Physical,
        );
        if !matches!(interconnect, StorageInterconnect::Network) {
            assert!(
                !smartctl_strategy_for_connection(connection).is_empty(),
                "{interconnect:?} may retain a typed read-only legacy observation attempt"
            );
        }
        assert!(
            smartctl_self_test_strategy_for_connection(connection).is_empty(),
            "{interconnect:?} must require a dedicated mutation-capable provider"
        );
    }
}

#[test]
fn command_exit_health_bits_are_accepted_but_invocation_bits_are_not() {
    for accepted in [0, 1 << 3, (1 << 3) | (1 << 4)] {
        assert!(smartctl_exit_allows_command(Some(accepted)));
    }
    for rejected in [1, 2, 4, 7] {
        assert!(!smartctl_exit_allows_command(Some(rejected)));
    }
    assert!(!smartctl_exit_allows_command(None));
}

#[test]
fn poll_strategy_continues_when_one_mechanism_has_no_self_test_fields() {
    let connection = StorageConnection::new(
        StorageProtocol::Ata,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    );
    let attempts = Cell::new(0);
    let report = execute_for_connection(
        "sda",
        connection,
        || Ok(()),
        |_, _| {
            attempts.set(attempts.get() + 1);
            let payload = if attempts.get() == 1 { "{}" } else { ATA };
            parse_smart_self_test_json(payload).map_or(
                StrategyAttempt::RetryableDeviceType,
                StrategyAttempt::Success,
            )
        },
    )
    .expect("second mechanism should expose self-test fields");

    assert_eq!(attempts.get(), 2);
    assert_eq!(report.phase, SmartSelfTestPhase::Running);
}

#[test]
fn ata_fixture_reports_running_progress_and_last_result() {
    let report = parse_smart_self_test_json(ATA).unwrap();
    assert_eq!(report.phase, SmartSelfTestPhase::Running);
    assert_eq!(report.progress_pct, Some(10.0));
    assert_eq!(report.kind, Some(SmartSelfTestKind::Short));
    assert_eq!(report.lifetime_hours, Some(12_876));
}

#[test]
fn nvme_fixture_reports_completed_extended_test() {
    let report = parse_smart_self_test_json(NVME).unwrap();
    assert_eq!(report.phase, SmartSelfTestPhase::Completed);
    assert_eq!(report.kind, Some(SmartSelfTestKind::Extended));
    assert_eq!(report.progress_pct, Some(100.0));
}

#[test]
fn provider_failure_preserves_success_and_parsed_recovery_advances_it() {
    let previous = DeviceState::healthy(100);
    let mut failed = failed_report(SmartSelfTestFailure::PermissionDenied, 200);
    failed.state = previous.transition(failed.state.status, 200);
    assert_eq!(failed.state.last_success_ms, Some(100));

    let mut recovered = parse_smart_self_test_json(NVME).unwrap();
    recovered.state = failed.state.transition(recovered.state.status, 300);
    assert_eq!(recovered.state, DeviceState::healthy(300));
}

#[test]
fn failed_self_test_reports_preserve_failure_and_device_status() {
    for (failure, expected_status) in [
        (SmartSelfTestFailure::MissingTool, DeviceStatus::MissingTool),
        (
            SmartSelfTestFailure::RequiresEscalation,
            DeviceStatus::PermissionDenied,
        ),
        (
            SmartSelfTestFailure::PermissionDenied,
            DeviceStatus::PermissionDenied,
        ),
        (SmartSelfTestFailure::TimedOut, DeviceStatus::Stale),
        (SmartSelfTestFailure::InvalidDevice, DeviceStatus::Stale),
        (
            SmartSelfTestFailure::ProviderUnavailable,
            DeviceStatus::Stale,
        ),
        (SmartSelfTestFailure::Rejected, DeviceStatus::Stale),
    ] {
        let report = failed_report(failure, 321);

        assert_eq!(report.failure, Some(failure));
        assert_eq!(report.state.status, expected_status);
        assert_eq!(report.state.last_success_ms, None);
    }
}

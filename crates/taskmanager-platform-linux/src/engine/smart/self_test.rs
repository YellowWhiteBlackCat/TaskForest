//! Typed SMART self-test plans, execution outcomes and status parsing.

#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::time::Duration;

use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::smart::self_test::{
    SmartSelfTestFailure, SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport,
};
use taskmanager_core::{StorageConnection, StorageInterconnect};
#[cfg(feature = "test-support")]
use taskmanager_core::{StorageDeviceKind, StorageProtocol};
use taskmanager_platform_contract::ProviderFailure;

use super::transport::{
    SmartctlDeviceType, command_output_is_permission_denied, command_output_requests_device_type,
    smartctl_device_path, smartctl_self_test_strategy_for_connection,
};
#[cfg(target_os = "linux")]
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

#[cfg(target_os = "linux")]
const SMART_SELF_TEST_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartSelfTestPlan {
    disk_name: String,
    kind: SmartSelfTestKind,
}

#[cfg(any(test, feature = "test-support"))]
impl SmartSelfTestPlan {
    /// Validated Linux whole-device name targeted by this intent.
    ///
    /// Command program, arguments, bridge strategy, and native path stay
    /// private to the provider execution layer.
    #[must_use]
    pub fn disk_name(&self) -> &str {
        &self.disk_name
    }

    /// Requested provider-neutral SMART self-test kind.
    #[must_use]
    pub const fn kind(&self) -> SmartSelfTestKind {
        self.kind
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn smart_self_test_plan(
    disk_name: &str,
    kind: SmartSelfTestKind,
) -> Result<SmartSelfTestPlan, SmartSelfTestFailure> {
    let device = safe_device_path(disk_name).ok_or(SmartSelfTestFailure::InvalidDevice)?;
    let disk_name = device
        .strip_prefix("/dev/")
        .unwrap_or(device.as_str())
        .to_owned();
    Ok(SmartSelfTestPlan { disk_name, kind })
}

fn smartctl_token(kind: SmartSelfTestKind) -> &'static str {
    match kind {
        SmartSelfTestKind::Short => "short",
        SmartSelfTestKind::Extended => "long",
        SmartSelfTestKind::Conveyance => "conveyance",
    }
}

enum StrategyAttempt<T> {
    Success(T),
    RetryableDeviceType,
    Failed(SmartSelfTestFailure),
}

#[derive(Debug, PartialEq, Eq)]
enum StrategyFailure {
    Provider(ProviderFailure),
    Report(SmartSelfTestFailure),
}

fn execute_for_connection<T>(
    disk_name: &str,
    connection: StorageConnection,
    mut revalidate: impl FnMut() -> Result<(), ProviderFailure>,
    mut attempt: impl FnMut(&str, SmartctlDeviceType) -> StrategyAttempt<T>,
) -> Result<T, StrategyFailure> {
    let device = smartctl_device_path(disk_name)
        .ok_or(StrategyFailure::Report(SmartSelfTestFailure::InvalidDevice))?;
    let strategy = smartctl_self_test_strategy_for_connection(connection);
    if strategy.is_empty() {
        return Err(StrategyFailure::Provider(ProviderFailure::Unsupported));
    }
    for device_type in strategy {
        revalidate().map_err(StrategyFailure::Provider)?;
        match attempt(&device, *device_type) {
            StrategyAttempt::Success(value) => return Ok(value),
            StrategyAttempt::RetryableDeviceType => {}
            StrategyAttempt::Failed(failure) => {
                return Err(StrategyFailure::Report(failure));
            }
        }
    }
    if connection.interconnect == StorageInterconnect::Usb {
        Err(StrategyFailure::Provider(ProviderFailure::Unsupported))
    } else {
        Err(StrategyFailure::Provider(
            ProviderFailure::TemporarilyUnavailable,
        ))
    }
}

fn execute_start_strategy<T>(
    disk_name: &str,
    connection: StorageConnection,
    revalidate: impl FnMut() -> Result<(), ProviderFailure>,
    attempt: impl FnMut(&str, SmartctlDeviceType) -> StrategyAttempt<T>,
) -> Result<T, StrategyFailure> {
    execute_for_connection(disk_name, connection, revalidate, attempt)
}

fn execute_poll_strategy<T>(
    disk_name: &str,
    connection: StorageConnection,
    revalidate: impl FnMut() -> Result<(), ProviderFailure>,
    attempt: impl FnMut(&str, SmartctlDeviceType) -> StrategyAttempt<T>,
) -> Result<T, StrategyFailure> {
    execute_for_connection(disk_name, connection, revalidate, attempt)
}

/// Start a self-test synchronously. This is a blocking provider API and must be
/// invoked by a worker. It never shells out and accepts only validated device
/// names.
#[cfg(all(feature = "test-support", target_os = "linux"))]
pub fn start_smart_self_test(
    disk_name: &str,
    kind: SmartSelfTestKind,
    now_ms: u64,
) -> SmartSelfTestReport {
    if safe_device_path(disk_name).is_none() {
        return failed_report(SmartSelfTestFailure::InvalidDevice, now_ms);
    }
    match start_smart_self_test_for_connection(
        disk_name,
        auto_detect_connection(disk_name),
        kind,
        now_ms,
        || Ok(()),
    ) {
        Ok(report) => report,
        Err(failure) => failed_report(provider_execution_failure(failure), now_ms),
    }
}

/// Start a SMART self-test using the protocol/interconnect strategy resolved
/// from the same authoritative storage discovery as the mutation target.
#[cfg(target_os = "linux")]
pub(crate) fn start_smart_self_test_for_connection(
    disk_name: &str,
    connection: StorageConnection,
    kind: SmartSelfTestKind,
    now_ms: u64,
    revalidate: impl FnMut() -> Result<(), ProviderFailure>,
) -> Result<SmartSelfTestReport, ProviderFailure> {
    match execute_start_strategy(disk_name, connection, revalidate, |device, device_type| {
        start_attempt(device, device_type, kind)
    }) {
        Ok(()) => Ok(SmartSelfTestReport {
            state: DeviceState::healthy(now_ms),
            phase: SmartSelfTestPhase::Running,
            kind: Some(kind),
            progress_pct: Some(0.0),
            ..Default::default()
        }),
        Err(StrategyFailure::Report(failure)) => Ok(failed_report(failure, now_ms)),
        Err(StrategyFailure::Provider(failure)) => Err(failure),
    }
}

#[cfg(target_os = "linux")]
fn start_attempt(
    device: &str,
    device_type: SmartctlDeviceType,
    kind: SmartSelfTestKind,
) -> StrategyAttempt<()> {
    let mut command = Command::new("smartctl");
    command.args(["-t", smartctl_token(kind)]);
    if let Some(device_type) = device_type.argument() {
        command.args(["-d", device_type]);
    }
    command.arg(device);
    match run_with_timeout(&mut command, SMART_SELF_TEST_TIMEOUT) {
        Ok(output) if smartctl_exit_allows_command(output.status.code()) => {
            StrategyAttempt::Success(())
        }
        Ok(output) if command_output_is_permission_denied(&output.stdout, &output.stderr) => {
            StrategyAttempt::Failed(SmartSelfTestFailure::PermissionDenied)
        }
        Ok(output) if command_output_requests_device_type(&output.stdout, &output.stderr) => {
            StrategyAttempt::RetryableDeviceType
        }
        Ok(_) => StrategyAttempt::Failed(SmartSelfTestFailure::Rejected),
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            StrategyAttempt::Failed(SmartSelfTestFailure::MissingTool)
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            StrategyAttempt::Failed(SmartSelfTestFailure::PermissionDenied)
        }
        Err(BoundedCommandError::TimedOut) => {
            StrategyAttempt::Failed(SmartSelfTestFailure::TimedOut)
        }
        Err(_) => StrategyAttempt::Failed(SmartSelfTestFailure::ProviderUnavailable),
    }
}

/// Query current/last SMART self-test state. Like `start_smart_self_test`, this
/// is blocking and intended for a worker. Provider failures preserve the last
/// successful timestamp supplied by the caller.
#[cfg(all(feature = "test-support", target_os = "linux"))]
pub fn read_smart_self_test_status(
    disk_name: &str,
    previous: DeviceState,
    now_ms: u64,
) -> SmartSelfTestReport {
    if safe_device_path(disk_name).is_none() {
        let mut report = failed_report(SmartSelfTestFailure::InvalidDevice, now_ms);
        report.state = previous.transition(report.state.status, now_ms);
        return report;
    }
    match read_smart_self_test_status_for_connection(
        disk_name,
        auto_detect_connection(disk_name),
        previous,
        now_ms,
        || Ok(()),
    ) {
        Ok(report) => report,
        Err(failure) => {
            let mut report = failed_report(provider_execution_failure(failure), now_ms);
            report.state = previous.transition(report.state.status, now_ms);
            report
        }
    }
}

/// Poll a SMART self-test through the same bounded device-type strategy used
/// at start time.
#[cfg(target_os = "linux")]
pub(crate) fn read_smart_self_test_status_for_connection(
    disk_name: &str,
    connection: StorageConnection,
    previous: DeviceState,
    now_ms: u64,
    revalidate: impl FnMut() -> Result<(), ProviderFailure>,
) -> Result<SmartSelfTestReport, ProviderFailure> {
    let result = execute_poll_strategy(disk_name, connection, revalidate, poll_attempt);
    let mut report = match result {
        Ok(report) => report,
        Err(StrategyFailure::Report(failure)) => failed_report(failure, now_ms),
        Err(StrategyFailure::Provider(failure)) => return Err(failure),
    };
    report.state = previous.transition(report.state.status, now_ms);
    Ok(report)
}

#[cfg(target_os = "linux")]
fn poll_attempt(
    device: &str,
    device_type: SmartctlDeviceType,
) -> StrategyAttempt<SmartSelfTestReport> {
    let mut command = Command::new("smartctl");
    command.args(["--json=c", "--all"]);
    if let Some(device_type) = device_type.argument() {
        command.args(["-d", device_type]);
    }
    command.arg(device);
    match run_with_timeout(&mut command, SMART_SELF_TEST_TIMEOUT) {
        Ok(output) if smartctl_exit_allows_command(output.status.code()) => {
            match String::from_utf8(output.stdout)
                .ok()
                .and_then(|text| parse_smart_self_test_json(&text))
            {
                Some(report) => StrategyAttempt::Success(report),
                None => StrategyAttempt::RetryableDeviceType,
            }
        }
        Ok(output) if command_output_is_permission_denied(&output.stdout, &output.stderr) => {
            StrategyAttempt::Failed(SmartSelfTestFailure::PermissionDenied)
        }
        Ok(output) if command_output_requests_device_type(&output.stdout, &output.stderr) => {
            StrategyAttempt::RetryableDeviceType
        }
        Ok(_) => StrategyAttempt::Failed(SmartSelfTestFailure::ProviderUnavailable),
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            StrategyAttempt::Failed(SmartSelfTestFailure::MissingTool)
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            StrategyAttempt::Failed(SmartSelfTestFailure::PermissionDenied)
        }
        Err(BoundedCommandError::TimedOut) => {
            StrategyAttempt::Failed(SmartSelfTestFailure::TimedOut)
        }
        Err(_) => StrategyAttempt::Failed(SmartSelfTestFailure::ProviderUnavailable),
    }
}

#[cfg(all(feature = "test-support", not(target_os = "linux")))]
pub fn read_smart_self_test_status(
    _disk_name: &str,
    previous: DeviceState,
    now_ms: u64,
) -> SmartSelfTestReport {
    SmartSelfTestReport {
        state: previous.transition(DeviceStatus::Unsupported, now_ms),
        failure: Some(SmartSelfTestFailure::ProviderUnavailable),
        ..Default::default()
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn read_smart_self_test_status_for_connection(
    _disk_name: &str,
    _connection: StorageConnection,
    previous: DeviceState,
    now_ms: u64,
    _revalidate: impl FnMut() -> Result<(), ProviderFailure>,
) -> Result<SmartSelfTestReport, ProviderFailure> {
    Ok(SmartSelfTestReport {
        state: previous.transition(DeviceStatus::Unsupported, now_ms),
        failure: Some(SmartSelfTestFailure::ProviderUnavailable),
        ..Default::default()
    })
}

#[cfg(all(feature = "test-support", not(target_os = "linux")))]
pub fn start_smart_self_test(
    _disk_name: &str,
    _kind: SmartSelfTestKind,
    _now_ms: u64,
) -> SmartSelfTestReport {
    SmartSelfTestReport {
        state: DeviceState::default(),
        failure: Some(SmartSelfTestFailure::ProviderUnavailable),
        ..Default::default()
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn start_smart_self_test_for_connection(
    _disk_name: &str,
    _connection: StorageConnection,
    _kind: SmartSelfTestKind,
    _now_ms: u64,
    _revalidate: impl FnMut() -> Result<(), ProviderFailure>,
) -> Result<SmartSelfTestReport, ProviderFailure> {
    Ok(SmartSelfTestReport {
        state: DeviceState::default(),
        failure: Some(SmartSelfTestFailure::ProviderUnavailable),
        ..Default::default()
    })
}

#[cfg(feature = "test-support")]
fn auto_detect_connection(disk_name: &str) -> StorageConnection {
    if disk_name
        .strip_prefix("/dev/")
        .unwrap_or(disk_name)
        .starts_with("nvme")
    {
        StorageConnection::new(
            StorageProtocol::Nvme,
            StorageInterconnect::Pcie,
            StorageDeviceKind::Physical,
        )
    } else {
        StorageConnection::new(
            StorageProtocol::Unknown,
            StorageInterconnect::Unknown,
            StorageDeviceKind::Physical,
        )
    }
}

fn smartctl_exit_allows_command(exit_code: Option<i32>) -> bool {
    exit_code.is_some_and(|exit_code| exit_code & 0b111 == 0)
}

#[cfg(any(test, feature = "test-support"))]
const fn provider_execution_failure(failure: ProviderFailure) -> SmartSelfTestFailure {
    match failure {
        ProviderFailure::RequiresEscalation => SmartSelfTestFailure::RequiresEscalation,
        ProviderFailure::PermissionDenied => SmartSelfTestFailure::PermissionDenied,
        ProviderFailure::MissingDependency => SmartSelfTestFailure::MissingTool,
        ProviderFailure::TimedOut => SmartSelfTestFailure::TimedOut,
        ProviderFailure::Rejected => SmartSelfTestFailure::Rejected,
        ProviderFailure::Unsupported
        | ProviderFailure::IdentityChanged
        | ProviderFailure::TemporarilyUnavailable
        | ProviderFailure::ProviderFault => SmartSelfTestFailure::ProviderUnavailable,
    }
}

pub fn parse_smart_self_test_json(text: &str) -> Option<SmartSelfTestReport> {
    let root: serde_json::Value = serde_json::from_str(text).ok()?;
    let status_text = string_at(
        &root,
        &[
            "/ata_smart_data/self_test/status/string",
            "/nvme_self_test_log/current_self_test_operation/string",
        ],
    );
    let progress_pct = number_at(
        &root,
        &[
            "/ata_smart_data/self_test/status/remaining_percent",
            "/nvme_self_test_log/current_self_test_completion_percent",
        ],
    )
    .map(|value| {
        if status_text
            .as_deref()
            .is_some_and(|status| status.to_ascii_lowercase().contains("remaining"))
        {
            100.0 - value
        } else {
            value
        }
    })
    .filter(|value| (0.0..=100.0).contains(value));
    let phase = status_text
        .as_deref()
        .map(parse_phase)
        .unwrap_or(SmartSelfTestPhase::Idle);
    let table = root
        .pointer("/ata_smart_self_test_log/standard/table")
        .or_else(|| root.pointer("/nvme_self_test_log/table"))
        .and_then(serde_json::Value::as_array);
    let latest = table.and_then(|entries| entries.first());
    let kind_text = latest
        .and_then(|entry| entry.pointer("/type/string"))
        .and_then(serde_json::Value::as_str)
        .or(status_text.as_deref());
    let kind = kind_text.and_then(parse_kind);
    let lifetime_hours = latest
        .and_then(|entry| entry.get("lifetime_hours"))
        .and_then(serde_json::Value::as_u64);
    let first_error_lba = latest
        .and_then(|entry| entry.get("lba_of_first_error"))
        .and_then(serde_json::Value::as_u64);
    if status_text.is_none() && table.is_none() {
        return None;
    }
    Some(SmartSelfTestReport {
        state: DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: None,
        },
        phase,
        kind,
        progress_pct,
        lifetime_hours,
        first_error_lba,
        failure: None,
    })
}

#[cfg(any(test, feature = "test-support"))]
fn safe_device_path(name: &str) -> Option<String> {
    let name = name.strip_prefix("/dev/").unwrap_or(name);
    let ata = name
        .strip_prefix("sd")
        .or_else(|| name.strip_prefix("hd"))
        .is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_lowercase())
        });
    let nvme = name.strip_prefix("nvme").is_some_and(valid_nvme_suffix);
    (ata || nvme).then(|| format!("/dev/{name}"))
}

#[cfg(any(test, feature = "test-support"))]
fn valid_nvme_suffix(suffix: &str) -> bool {
    let controller_digits = suffix.bytes().take_while(u8::is_ascii_digit).count();
    if controller_digits == 0 {
        return false;
    }
    let remainder = &suffix[controller_digits..];
    if remainder.is_empty() {
        return true;
    }
    let Some(namespace) = remainder.strip_prefix('n') else {
        return false;
    };
    let namespace_digits = namespace.bytes().take_while(u8::is_ascii_digit).count();
    if namespace_digits == 0 {
        return false;
    }
    let remainder = &namespace[namespace_digits..];
    if remainder.is_empty() {
        return true;
    }
    remainder.strip_prefix('p').is_some_and(|partition| {
        !partition.is_empty() && partition.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn string_at(root: &serde_json::Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .find_map(|pointer| root.pointer(pointer).and_then(serde_json::Value::as_str))
        .map(ToOwned::to_owned)
}

fn number_at(root: &serde_json::Value, pointers: &[&str]) -> Option<f32> {
    pointers
        .iter()
        .find_map(|pointer| root.pointer(pointer).and_then(serde_json::Value::as_f64))
        .map(|value| value as f32)
}

fn parse_phase(status: &str) -> SmartSelfTestPhase {
    let status = status.to_ascii_lowercase();
    if status.contains("progress") || status.contains("remaining") || status.contains("running") {
        SmartSelfTestPhase::Running
    } else if status.contains("completed without error") || status.contains("success") {
        SmartSelfTestPhase::Completed
    } else if status.contains("abort") || status.contains("interrupt") {
        SmartSelfTestPhase::Aborted
    } else if status.contains("fail") || status.contains("error") {
        SmartSelfTestPhase::Failed
    } else if status.contains("no self-test") || status.contains("idle") {
        SmartSelfTestPhase::Idle
    } else {
        SmartSelfTestPhase::Unknown
    }
}

fn parse_kind(kind: &str) -> Option<SmartSelfTestKind> {
    let kind = kind.to_ascii_lowercase();
    if kind.contains("short") {
        Some(SmartSelfTestKind::Short)
    } else if kind.contains("extended") || kind.contains("long") {
        Some(SmartSelfTestKind::Extended)
    } else if kind.contains("conveyance") {
        Some(SmartSelfTestKind::Conveyance)
    } else {
        None
    }
}

fn failed_report(failure: SmartSelfTestFailure, now_ms: u64) -> SmartSelfTestReport {
    let status = match failure {
        SmartSelfTestFailure::MissingTool => DeviceStatus::MissingTool,
        SmartSelfTestFailure::RequiresEscalation | SmartSelfTestFailure::PermissionDenied => {
            DeviceStatus::PermissionDenied
        }
        SmartSelfTestFailure::TimedOut => DeviceStatus::Stale,
        SmartSelfTestFailure::InvalidDevice
        | SmartSelfTestFailure::ProviderUnavailable
        | SmartSelfTestFailure::Rejected => DeviceStatus::Stale,
    };
    SmartSelfTestReport {
        state: DeviceState::default().transition(status, now_ms),
        failure: Some(failure),
        ..Default::default()
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/engine/smart/self_test.rs"]
mod tests;

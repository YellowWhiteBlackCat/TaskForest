//! Transport-aware smartctl command planning and protocol-neutral JSON parsing.

use taskmanager_core::core::metrics::{
    SmartAvailability, StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol,
};
use taskmanager_core::core::smart::SmartProviderFailureKind;

use super::{DiskSmart, SmartCommandResult, parse_command_result};
#[cfg(target_os = "linux")]
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

mod diagnostic;
#[cfg(test)]
#[path = "../../../tests/headless/engine/smart/transport/proptests.rs"]
mod proptests;
use diagnostic::{
    SmartctlCommandDiagnostic, classify_smartctl_command_diagnostic,
    smartctl_json_reports_unsupported,
};
pub(super) use diagnostic::{
    command_output_is_permission_denied, command_output_requests_device_type,
};

/// Convert a collector physical name to a whole-device path without accepting
/// an option, slash, traversal component, or untrusted character.
///
/// Requests originate from the sysfs whole-device inventory, so an unknown
/// transport may retain a future kernel device name. This boundary validates
/// command arguments; it deliberately does not use an `sd*`/`hd*` hardware
/// allowlist.
pub(super) fn smartctl_device_path(name: &str) -> Option<String> {
    let name = name.strip_prefix("/dev/").unwrap_or(name);
    if name.is_empty()
        || name.starts_with('-')
        || matches!(name, "." | "..")
        || name.len() > 255
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return None;
    }
    Some(format!("/dev/{name}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SmartctlDeviceType {
    Auto,
    Sat,
    Scsi,
    SntAsmedia,
    SntJmicron,
    SntRealtek,
}

impl SmartctlDeviceType {
    pub(super) fn argument(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Sat => Some("sat"),
            Self::Scsi => Some("scsi"),
            Self::SntAsmedia => Some("sntasmedia"),
            Self::SntJmicron => Some("sntjmicron"),
            Self::SntRealtek => Some("sntrealtek"),
        }
    }
}

pub(super) fn smartctl_strategy_for_connection(
    connection: StorageConnection,
) -> &'static [SmartctlDeviceType] {
    use SmartctlDeviceType::{Auto, Sat, Scsi, SntAsmedia, SntJmicron, SntRealtek};

    if connection.device_kind != StorageDeviceKind::Physical {
        return &[];
    }
    match (connection.interconnect, connection.protocol) {
        // Ask smartmontools to use its drive database and native command-set
        // detection first. Linux presents many USB-SAT/UAS devices through the
        // SCSI subsystem, so sysfs protocol alone cannot safely choose between
        // ATA pass-through and native SCSI.
        (StorageInterconnect::Usb, StorageProtocol::Ata) => &[Auto, Sat, Scsi],
        (StorageInterconnect::Usb, StorageProtocol::Scsi) => &[Auto, Scsi, Sat],
        (StorageInterconnect::Usb, StorageProtocol::Nvme) => {
            &[Auto, SntAsmedia, SntJmicron, SntRealtek]
        }
        (StorageInterconnect::Usb, _) => &[Auto, Sat, Scsi, SntAsmedia, SntJmicron, SntRealtek],
        (_, StorageProtocol::Nvme) => &[Auto],
        (_, StorageProtocol::Scsi) => &[Auto, Scsi],
        (_, StorageProtocol::Ata) => &[Auto, Sat],
        (
            StorageInterconnect::Pcie
            | StorageInterconnect::Sata
            | StorageInterconnect::Sas
            | StorageInterconnect::Ide
            | StorageInterconnect::FibreChannel
            | StorageInterconnect::Iscsi
            | StorageInterconnect::PcieTunnel
            | StorageInterconnect::FireWire
            | StorageInterconnect::Other
            | StorageInterconnect::Unknown,
            StorageProtocol::Unknown,
        ) => &[Auto],
        _ => &[],
    }
}

/// Self-tests are mutations, so their topology authority is intentionally
/// narrower than read-only SMART observation. Remote/SAN and unknown fabrics
/// require a future dedicated control provider that can prove target identity.
pub(super) fn smartctl_self_test_strategy_for_connection(
    connection: StorageConnection,
) -> &'static [SmartctlDeviceType] {
    if matches!(
        connection.interconnect,
        StorageInterconnect::FibreChannel
            | StorageInterconnect::Iscsi
            | StorageInterconnect::Network
            | StorageInterconnect::Platform
            | StorageInterconnect::Virtio
            | StorageInterconnect::Other
            | StorageInterconnect::Unknown
    ) {
        &[]
    } else {
        smartctl_strategy_for_connection(connection)
    }
}

pub(super) fn read_smartctl_smart(name: &str, connection: StorageConnection) -> DiskSmart {
    read_smartctl_with_connection(name, connection, smartctl_json_stdout)
}

fn read_smartctl_with_connection(
    name: &str,
    connection: StorageConnection,
    mut fetch: impl FnMut(&str, SmartctlDeviceType) -> SmartCommandResult,
) -> DiskSmart {
    let Some(device) = smartctl_device_path(name) else {
        return DiskSmart::with_failure(SmartProviderFailureKind::UnsupportedProtocol);
    };
    let strategy = smartctl_strategy_for_connection(connection);
    if strategy.is_empty() {
        return DiskSmart::with_failure(SmartProviderFailureKind::UnsupportedProtocol);
    }

    let mut saw_retryable_type = false;
    let mut saw_malformed_output = false;
    let mut saw_unsupported = false;
    for device_type in strategy {
        match fetch(&device, *device_type) {
            SmartCommandResult::Output(stdout) => {
                if smartctl_json_reports_unsupported(&stdout) {
                    saw_unsupported = true;
                } else if let Some(smart) = parse_smartctl_json(&stdout) {
                    return smart;
                } else {
                    saw_malformed_output = true;
                }
            }
            SmartCommandResult::RetryableDeviceType => saw_retryable_type = true,
            SmartCommandResult::Unsupported => saw_unsupported = true,
            terminal => return parse_command_result(terminal, parse_smartctl_json),
        }
    }
    if saw_malformed_output {
        DiskSmart::with_failure(SmartProviderFailureKind::MalformedResponse)
    } else if connection.interconnect == StorageInterconnect::Usb && saw_retryable_type {
        DiskSmart::with_failure(SmartProviderFailureKind::BridgeLimitation)
    } else if saw_unsupported {
        DiskSmart::with_failure(SmartProviderFailureKind::UnsupportedProtocol)
    } else {
        DiskSmart::with_failure(SmartProviderFailureKind::TemporarilyUnavailable)
    }
}

/// Capture compact smartctl JSON without invoking a shell. smartctl uses an
/// exit-status bitmask: bits 0–2 are invocation/device/command failures and
/// make the sample unavailable, while bits 3–7 report actual SMART health
/// findings and must not discard otherwise valid telemetry.
#[cfg(target_os = "linux")]
fn smartctl_json_stdout(device: &str, device_type: SmartctlDeviceType) -> SmartCommandResult {
    let mut command = std::process::Command::new("smartctl");
    command.args(["--json=c", "--all"]);
    if let Some(device_type) = device_type.argument() {
        command.args(["-d", device_type]);
    }
    command.arg(device);
    let output = match run_with_timeout(&mut command, super::SMART_COMMAND_TIMEOUT) {
        Ok(output) => output,
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return SmartCommandResult::MissingTool;
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            return SmartCommandResult::PermissionDenied;
        }
        Err(BoundedCommandError::TimedOut) => return SmartCommandResult::TimedOut,
        Err(_) => return SmartCommandResult::Unavailable,
    };
    let Some(exit_code) = output.status.code() else {
        return SmartCommandResult::Unavailable;
    };
    if !smartctl_exit_allows_data(exit_code) {
        return classify_smartctl_command_diagnostic(&output.stdout, &output.stderr)
            .map_or(SmartCommandResult::Unavailable, smartctl_diagnostic_result);
    }
    match String::from_utf8(output.stdout) {
        Ok(stdout) => SmartCommandResult::Output(stdout),
        Err(_) => SmartCommandResult::Output(String::new()),
    }
}

const fn smartctl_diagnostic_result(diagnostic: SmartctlCommandDiagnostic) -> SmartCommandResult {
    match diagnostic {
        SmartctlCommandDiagnostic::PermissionDenied => SmartCommandResult::PermissionDenied,
        SmartctlCommandDiagnostic::DeviceUnavailable => SmartCommandResult::DeviceUnavailable,
        SmartctlCommandDiagnostic::CommandFailure => SmartCommandResult::CommandFailed,
        SmartctlCommandDiagnostic::DeviceTypeRequired => SmartCommandResult::RetryableDeviceType,
        SmartctlCommandDiagnostic::Unsupported => SmartCommandResult::Unsupported,
    }
}

#[cfg(not(target_os = "linux"))]
fn smartctl_json_stdout(_device: &str, _device_type: SmartctlDeviceType) -> SmartCommandResult {
    SmartCommandResult::Unsupported
}

#[cfg(target_os = "linux")]
fn smartctl_exit_allows_data(exit_code: i32) -> bool {
    exit_code & 0b111 == 0
}

/// Pure parser for smartctl ATA/SATA/SAS/SCSI JSON. Standard top-level fields
/// take precedence; ATA attribute IDs are conservative compatibility fallbacks.
pub fn parse_smartctl_json(stdout: &str) -> Option<DiskSmart> {
    use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};

    let root: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let attributes = root
        .pointer("/ata_smart_attributes/table")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice);

    let temperature_c = root
        .pointer("/temperature/current")
        .and_then(serde_json::Value::as_f64)
        .or_else(|| ata_raw_attribute(attributes, &[194, 190]).and_then(serde_json::Value::as_f64))
        .filter(|value| value.is_finite() && (-273.15..=1000.0).contains(value))
        .map(|value| value as f32);
    let critical_warning = root
        .pointer("/smart_status/passed")
        .and_then(serde_json::Value::as_bool)
        .map(|passed| !passed);
    let power_on_hours = root
        .pointer("/power_on_time/hours")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| ata_raw_attribute(attributes, &[9]).and_then(serde_json::Value::as_u64));
    let percent_used = root
        .get("scsi_percentage_used_endurance_indicator")
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as f32);
    let ata_attributes = parse_ata_attributes(attributes);

    if temperature_c.is_none()
        && critical_warning.is_none()
        && percent_used.is_none()
        && power_on_hours.is_none()
        && ata_attributes.is_none()
    {
        return None;
    }
    Some(DiskSmart {
        availability: SmartAvailability::Available,
        state: DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: None,
        },
        provider: None,
        failure: None,
        temperature_c,
        critical_warning,
        temp_critical_c: None,
        percent_used,
        power_on_hours,
        ata_attributes,
    })
}

fn ata_raw_attribute<'a>(
    attributes: Option<&'a [serde_json::Value]>,
    ids: &[u64],
) -> Option<&'a serde_json::Value> {
    attributes?
        .iter()
        .find(|attribute| {
            attribute
                .get("id")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|id| ids.contains(&id))
        })
        .and_then(|attribute| attribute.pointer("/raw/value"))
}

/// Map the `/ata_smart_attributes/table` array into typed rows. Entries
/// without a valid `id` / `raw.value` are dropped, since they cannot be
/// matched against the failure-precedent IDs (5/197/198/199). A failing-now
/// signal is accepted from any of the shapes smartctl emits across versions:
/// the boolean `failing_now`/`failed` flags, or the canonical `when_failed`
/// string value `"now"`.
fn parse_ata_attributes(
    attributes: Option<&[serde_json::Value]>,
) -> Option<Vec<taskmanager_core::core::smart::AtaSmartAttribute>> {
    use taskmanager_core::core::smart::AtaSmartAttribute;

    let parsed: Vec<AtaSmartAttribute> = attributes?
        .iter()
        .filter_map(|attribute| {
            let id = attribute.get("id").and_then(serde_json::Value::as_u64)?;
            // u16 is the on-wire ATA attribute id range; reject implausible
            // values rather than truncating them into a misleading key.
            if id > u16::MAX as u64 {
                return None;
            }
            let raw_value = attribute
                .pointer("/raw/value")
                .and_then(serde_json::Value::as_u64)?;
            let failing_now = attribute
                .get("failing_now")
                .and_then(serde_json::Value::as_bool)
                .or_else(|| attribute.get("failed").and_then(serde_json::Value::as_bool))
                .unwrap_or_else(|| {
                    attribute
                        .get("when_failed")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|value| value.eq_ignore_ascii_case("now"))
                });
            Some(AtaSmartAttribute {
                id: id as u16,
                raw_value,
                failing_now,
            })
        })
        .collect();
    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/engine/smart/transport.rs"]
mod tests;

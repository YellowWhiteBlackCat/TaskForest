//! Fixed Linux helper for one SMART self-test start operation.
//!
//! This binary has a separate polkit path and policy from the GPU PMU helper.
//! Its only operation is `smartctl -t {short,long,conveyance}` against one
//! generation-bound, persistent disk identity. It accepts no executable or
//! arbitrary arguments from the caller and never invokes a command shell.

#![forbid(unsafe_code)]

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use taskmanager_core::core::device_state::stable_disk_id;
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::storage::{
    StorageConnection, StorageDeviceKind, StorageDeviceTarget, StorageInterconnect, StorageProtocol,
};
use taskmanager_core::core::{DeviceGeneration, DeviceId, StorageDeviceKey};
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const SYSFS_BLOCK_ROOT: &str = "/sys/class/block";
const SMARTCTL_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
enum Failure {
    InvalidTarget,
    IdentityChanged,
    MissingTool,
    PermissionDenied,
    SupportUnknown,
    TimedOut,
    Unsupported,
    Rejected,
    ProviderUnavailable,
}

impl Failure {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidTarget => "invalid_target",
            Self::IdentityChanged => "identity_changed",
            Self::MissingTool => "missing_tool",
            Self::PermissionDenied => "permission_denied",
            Self::SupportUnknown => "support_unknown",
            Self::TimedOut => "timed_out",
            Self::Unsupported => "unsupported",
            Self::Rejected => "rejected",
            Self::ProviderUnavailable => "provider_unavailable",
        }
    }
}

struct Request {
    target: StorageDeviceTarget,
    connection: StorageConnection,
    kind: SmartSelfTestKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceSupport {
    Unknown,
    Supported,
    Unsupported,
}

enum CapabilityAttempt {
    Observed(DeviceSupport),
    RetryableDeviceType,
    Failed(Failure),
}

pub fn run() -> ExitCode {
    match start(parse_request(std::env::args_os().skip(1))) {
        Ok(()) => emit_success(),
        Err(failure) => emit_failure(failure),
    }
}

fn parse_request(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<Request, Failure> {
    let args = args
        .into_iter()
        .map(|value| value.into_string().map_err(|_| Failure::InvalidTarget))
        .collect::<Result<Vec<_>, _>>()?;
    if args.len() != 7 || args[0] != "--start-self-test" {
        return Err(Failure::InvalidTarget);
    }
    let generation = args[2]
        .parse::<u64>()
        .ok()
        .map(DeviceGeneration::new)
        .filter(|generation| generation.is_valid())
        .ok_or(Failure::InvalidTarget)?;
    let protocol = parse_protocol(&args[4]).ok_or(Failure::InvalidTarget)?;
    let interconnect = parse_interconnect(&args[5]).ok_or(Failure::InvalidTarget)?;
    let kind = parse_kind(&args[6]).ok_or(Failure::InvalidTarget)?;
    Ok(Request {
        target: StorageDeviceTarget {
            device_id: DeviceId::new(args[1].as_str()),
            device_generation: generation,
            locator: StorageDeviceKey::new(args[3].as_str()),
        },
        connection: StorageConnection::new(protocol, interconnect, StorageDeviceKind::Physical),
        kind,
    })
}

fn start(request: Result<Request, Failure>) -> Result<(), Failure> {
    let request = request?;
    if !is_persistent_disk_id(request.target.device_id.as_str()) {
        return Err(Failure::InvalidTarget);
    }
    let name = kernel_name(request.target.locator.as_str()).ok_or(Failure::InvalidTarget)?;
    let strategy = smartctl_strategy(request.connection);
    if strategy.is_empty() {
        return Err(Failure::Unsupported);
    }
    let device = format!("/dev/{name}");
    for device_type in strategy {
        verify_live_identity(name, request.target.device_id.as_str())?;
        match inspect_device_support(&device, *device_type, request.kind) {
            CapabilityAttempt::Observed(DeviceSupport::Supported) => {}
            CapabilityAttempt::Observed(DeviceSupport::Unsupported) => {
                return Err(Failure::Unsupported);
            }
            CapabilityAttempt::Observed(DeviceSupport::Unknown) => {
                return Err(Failure::SupportUnknown);
            }
            CapabilityAttempt::RetryableDeviceType => continue,
            CapabilityAttempt::Failed(failure) => return Err(failure),
        }
        verify_live_identity(name, request.target.device_id.as_str())?;
        let mut command = Command::new("smartctl");
        command
            .args(["--json=c", "-t", smartctl_token(request.kind)])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(device_type) = device_type {
            command.args(["-d", device_type]);
        }
        command.arg(&device);
        match run_with_timeout(&mut command, SMARTCTL_TIMEOUT) {
            Ok(output) if smartctl_exit_allows_command(output.status.code()) => return Ok(()),
            Ok(output) => match classify_output(&output.stdout, &output.stderr) {
                Some(Failure::Unsupported) => return Err(Failure::Unsupported),
                Some(Failure::PermissionDenied) => return Err(Failure::PermissionDenied),
                Some(Failure::IdentityChanged) => return Err(Failure::IdentityChanged),
                _ if output_requests_device_type(&output.stdout, &output.stderr) => continue,
                _ => return Err(Failure::Rejected),
            },
            Err(BoundedCommandError::Spawn(error)) if error.kind() == io::ErrorKind::NotFound => {
                return Err(Failure::MissingTool);
            }
            Err(BoundedCommandError::Spawn(error))
                if error.kind() == io::ErrorKind::PermissionDenied =>
            {
                return Err(Failure::PermissionDenied);
            }
            Err(BoundedCommandError::TimedOut) => return Err(Failure::TimedOut),
            Err(_) => return Err(Failure::ProviderUnavailable),
        }
    }
    Err(Failure::Unsupported)
}

fn inspect_device_support(
    device: &str,
    device_type: Option<&str>,
    kind: SmartSelfTestKind,
) -> CapabilityAttempt {
    let mut command = Command::new("smartctl");
    command
        .args(["--json=c", "--capabilities"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(device_type) = device_type {
        command.args(["-d", device_type]);
    }
    command.arg(device);
    match run_with_timeout(&mut command, SMARTCTL_TIMEOUT) {
        Ok(output) if smartctl_exit_allows_command(output.status.code()) => {
            let support = String::from_utf8(output.stdout)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .map_or(DeviceSupport::Unknown, |root| {
                    self_test_support(&root, kind)
                });
            CapabilityAttempt::Observed(support)
        }
        Ok(output) if output_requests_device_type(&output.stdout, &output.stderr) => {
            CapabilityAttempt::RetryableDeviceType
        }
        Ok(output) => CapabilityAttempt::Failed(
            classify_output(&output.stdout, &output.stderr).unwrap_or(Failure::Rejected),
        ),
        Err(BoundedCommandError::Spawn(error)) if error.kind() == io::ErrorKind::NotFound => {
            CapabilityAttempt::Failed(Failure::MissingTool)
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == io::ErrorKind::PermissionDenied =>
        {
            CapabilityAttempt::Failed(Failure::PermissionDenied)
        }
        Err(BoundedCommandError::TimedOut) => CapabilityAttempt::Failed(Failure::TimedOut),
        Err(_) => CapabilityAttempt::Failed(Failure::ProviderUnavailable),
    }
}

fn self_test_support(root: &serde_json::Value, kind: SmartSelfTestKind) -> DeviceSupport {
    if let Some(nvme_commands) = root
        .get("nvme_optional_admin_commands")
        .and_then(serde_json::Value::as_object)
    {
        if kind == SmartSelfTestKind::Conveyance {
            return DeviceSupport::Unsupported;
        }
        return explicit_support(nvme_commands.get("self_test"));
    }
    let Some(ata_capabilities) = root
        .pointer("/ata_smart_data/capabilities")
        .and_then(serde_json::Value::as_object)
    else {
        return DeviceSupport::Unknown;
    };
    let key = match kind {
        SmartSelfTestKind::Short | SmartSelfTestKind::Extended => "self_tests_supported",
        SmartSelfTestKind::Conveyance => "conveyance_self_test_supported",
    };
    explicit_support(ata_capabilities.get(key))
}

fn explicit_support(value: Option<&serde_json::Value>) -> DeviceSupport {
    match value.and_then(serde_json::Value::as_bool) {
        Some(true) => DeviceSupport::Supported,
        Some(false) => DeviceSupport::Unsupported,
        None => DeviceSupport::Unknown,
    }
}

fn verify_live_identity(name: &str, expected_id: &str) -> Result<(), Failure> {
    let device = Path::new(SYSFS_BLOCK_ROOT).join(name);
    fs::metadata(&device).map_err(|_| Failure::IdentityChanged)?;
    match fs::metadata(device.join("partition")) {
        Ok(_) => return Err(Failure::InvalidTarget),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(Failure::ProviderUnavailable),
    }
    let wwid = first_live_text(&[
        device.join("wwid"),
        device.join("device/wwid"),
        device.join("device/cid"),
        device.join("md/uuid"),
        device.join("dm/uuid"),
    ]);
    let serial = read_live_text(&device.join("device/serial"));
    if wwid.is_none() && serial.is_none() {
        return Err(Failure::ProviderUnavailable);
    }
    let observed_id = stable_disk_id(name, wwid.as_deref(), serial.as_deref());
    if observed_id == expected_id {
        Ok(())
    } else {
        Err(Failure::IdentityChanged)
    }
}

fn first_live_text(paths: &[std::path::PathBuf]) -> Option<String> {
    paths.iter().find_map(|path| read_live_text(path))
}

fn read_live_text(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn kernel_name(locator: &str) -> Option<&str> {
    let name = locator.strip_prefix("/dev/").unwrap_or(locator);
    (!name.is_empty()
        && name.len() <= 255
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    .then_some(name)
}

fn is_persistent_disk_id(value: &str) -> bool {
    value.starts_with("disk:wwid:") || value.starts_with("disk:serial:")
}

fn smartctl_strategy(connection: StorageConnection) -> &'static [Option<&'static str>] {
    const AUTO: &[Option<&str>] = &[None];
    const ATA_USB: &[Option<&str>] = &[None, Some("sat"), Some("scsi")];
    const SCSI_USB: &[Option<&str>] = &[None, Some("scsi"), Some("sat")];
    const NVME_USB: &[Option<&str>] = &[
        None,
        Some("sntasmedia"),
        Some("sntjmicron"),
        Some("sntrealtek"),
    ];
    const UNKNOWN_USB: &[Option<&str>] = &[
        None,
        Some("sat"),
        Some("scsi"),
        Some("sntasmedia"),
        Some("sntjmicron"),
        Some("sntrealtek"),
    ];
    const ATA: &[Option<&str>] = &[None, Some("sat")];
    const SCSI: &[Option<&str>] = &[None, Some("scsi")];
    if connection.device_kind != StorageDeviceKind::Physical
        || matches!(
            connection.interconnect,
            StorageInterconnect::FibreChannel
                | StorageInterconnect::Iscsi
                | StorageInterconnect::Network
                | StorageInterconnect::Platform
                | StorageInterconnect::Virtio
                | StorageInterconnect::Other
                | StorageInterconnect::Unknown
        )
    {
        return &[];
    }
    match (connection.interconnect, connection.protocol) {
        (StorageInterconnect::Usb, StorageProtocol::Ata) => ATA_USB,
        (StorageInterconnect::Usb, StorageProtocol::Scsi) => SCSI_USB,
        (StorageInterconnect::Usb, StorageProtocol::Nvme) => NVME_USB,
        (StorageInterconnect::Usb, _) => UNKNOWN_USB,
        (_, StorageProtocol::Nvme) => AUTO,
        (_, StorageProtocol::Scsi) => SCSI,
        (_, StorageProtocol::Ata) => ATA,
        (
            StorageInterconnect::Pcie
            | StorageInterconnect::Sata
            | StorageInterconnect::Sas
            | StorageInterconnect::Ide
            | StorageInterconnect::PcieTunnel
            | StorageInterconnect::FireWire
            | StorageInterconnect::Other
            | StorageInterconnect::Unknown,
            StorageProtocol::Unknown,
        ) => AUTO,
        _ => &[],
    }
}

fn parse_protocol(value: &str) -> Option<StorageProtocol> {
    Some(match value {
        "nvme" => StorageProtocol::Nvme,
        "ata" => StorageProtocol::Ata,
        "scsi" => StorageProtocol::Scsi,
        "unknown" => StorageProtocol::Unknown,
        "mmc" => StorageProtocol::Mmc,
        "sd" => StorageProtocol::Sd,
        "ufs" => StorageProtocol::Ufs,
        "other" => StorageProtocol::Other,
        _ => return None,
    })
}

fn parse_interconnect(value: &str) -> Option<StorageInterconnect> {
    Some(match value {
        "pcie" => StorageInterconnect::Pcie,
        "sata" => StorageInterconnect::Sata,
        "sas" => StorageInterconnect::Sas,
        "usb" => StorageInterconnect::Usb,
        "mmc" => StorageInterconnect::Mmc,
        "sd" => StorageInterconnect::Sd,
        "ufs" => StorageInterconnect::Ufs,
        "ide" => StorageInterconnect::Ide,
        "virtio" => StorageInterconnect::Virtio,
        "fibre_channel" => StorageInterconnect::FibreChannel,
        "iscsi" => StorageInterconnect::Iscsi,
        "network" => StorageInterconnect::Network,
        "pcie_tunnel" => StorageInterconnect::PcieTunnel,
        "fire_wire" => StorageInterconnect::FireWire,
        "platform" => StorageInterconnect::Platform,
        "other" => StorageInterconnect::Other,
        "unknown" => StorageInterconnect::Unknown,
        _ => return None,
    })
}

fn parse_kind(value: &str) -> Option<SmartSelfTestKind> {
    Some(match value {
        "short" => SmartSelfTestKind::Short,
        "extended" => SmartSelfTestKind::Extended,
        "conveyance" => SmartSelfTestKind::Conveyance,
        _ => return None,
    })
}

fn smartctl_token(kind: SmartSelfTestKind) -> &'static str {
    match kind {
        SmartSelfTestKind::Short => "short",
        SmartSelfTestKind::Extended => "long",
        SmartSelfTestKind::Conveyance => "conveyance",
    }
}

fn smartctl_exit_allows_command(exit_code: Option<i32>) -> bool {
    exit_code.is_some_and(|exit_code| exit_code & 0b111 == 0)
}

fn classify_output(stdout: &[u8], stderr: &[u8]) -> Option<Failure> {
    let stdout = String::from_utf8_lossy(stdout).to_ascii_lowercase();
    let stderr = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    let text = format!("{stdout}\n{stderr}");
    if [
        "permission denied",
        "operation not permitted",
        "insufficient privileges",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        Some(Failure::PermissionDenied)
    } else if [
        "no such device",
        "device not found",
        "device has disappeared",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        Some(Failure::IdentityChanged)
    } else if [
        "smart support is: unavailable",
        "device lacks smart capability",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        Some(Failure::Unsupported)
    } else {
        None
    }
}

fn output_requests_device_type(stdout: &[u8], stderr: &[u8]) -> bool {
    let stdout = String::from_utf8_lossy(stdout).to_ascii_lowercase();
    let stderr = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    let text = format!("{stdout}\n{stderr}");
    [
        "please specify device type",
        "requires option '-d",
        "try adding '-d",
        "unknown usb bridge",
        "unsupported usb bridge",
        "device type mismatch",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn emit_success() -> ExitCode {
    emit(r#"{"schema":1,"status":"started"}"#, 0)
}

fn emit_failure(failure: Failure) -> ExitCode {
    let output = serde_json::json!({
        "schema": 1,
        "status": "error",
        "kind": failure.as_str(),
    });
    match serde_json::to_string(&output) {
        Ok(json) => emit(&json, 1),
        Err(_) => emit(
            r#"{"schema":1,"status":"error","kind":"provider_unavailable"}"#,
            1,
        ),
    }
}

fn emit(json: &str, exit_code: u8) -> ExitCode {
    let mut stdout = io::stdout().lock();
    match writeln!(stdout, "{json}").and_then(|()| stdout.flush()) {
        Ok(()) => ExitCode::from(exit_code),
        Err(_) => ExitCode::FAILURE,
    }
}

#[cfg(test)]
#[path = "../tests/headless/smart_self_test_helper.rs"]
mod tests;

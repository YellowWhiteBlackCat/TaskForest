//! Disk SMART / health telemetry with graceful degradation.
//!
//! NVMe uses two complementary sources, both best-effort and graceful:
//!  1. **sysfs hwmon** (primary; verified on this host). The kernel `nvme`
//!     driver registers a hwmon device symlinked at
//!     `/sys/class/nvme/<controller>/hwmon*` whose `name` reads `nvme`. It
//!     exposes the SMART **Composite** temperature (`temp1_input`,
//!     millidegrees °C), the critical-temperature alarm (`temp1_alarm` — set
//!     when the composite crosses `temp1_crit`, the most actionable bit of the
//!     NVMe critical-warning byte the kernel surfaces), and the thresholds
//!     (`temp1_crit` / `temp1_max`). This gives temperature + critical-warning
//!     + critical threshold with NO external binary and NO root.
//!  2. **`nvme smart-log` shell-out** (supplementary). For the endurance/power
//!     fields sysfs does NOT expose (`percentage_used`, `power_on_hours`) we
//!     parse the `nvme-cli` `smart-log` output when the binary is installed.
//!     Requires the controller char device (`/dev/nvmeN`) and may require
//!     elevated device permissions. Command and sysfs failures remain typed
//!     enrichment status; available fields are retained as a partial result.
//!  3. **ATA/SATA `smartctl` JSON** (Linux-only). Whole-disk devices such as
//!     `/dev/sda` are queried with separate, fixed [`std::process::Command`]
//!     arguments (`smartctl --json=c --all [-d TYPE] <device>`; never a shell
//!     string). The runtime block transport selects bounded SATA, SAS/SCSI, and
//!     USB SAT/UAS strategies; no vendor/model allowlist or vendor build is
//!     involved.
//!     The pure JSON parser maps temperature, overall SMART failure, and
//!     power-on hours into the existing shared fields. Missing binaries,
//!     permissions, unsupported devices, and malformed output yield unavailable
//!     fields rather than guessed values.
//!
//! The public model and parsers are reusable by background workers; the metrics
//! collector owns periodic invocation and caching.

#[cfg(not(target_os = "linux"))]
use taskmanager_core::core::device_state::DeviceStatus;
#[cfg(target_os = "linux")]
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::SmartAvailability;
#[cfg(feature = "test-support")]
use taskmanager_core::core::metrics::{
    StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol,
};
pub use taskmanager_core::core::smart::DiskSmart;
use taskmanager_core::core::smart::SmartProviderFailureKind;

#[cfg(target_os = "linux")]
use std::time::Duration;
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

pub(crate) mod provider;
pub mod self_test;
mod transport;

pub(crate) use provider::SmartProviderRegistry;
#[cfg(feature = "test-support")]
pub use transport::parse_smartctl_json;

#[cfg(target_os = "linux")]
const SMART_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

/// Read the best available SMART data when only a physical disk name is known.
#[cfg(feature = "test-support")]
pub fn read_disk_smart(name: &str) -> DiskSmart {
    let connection = if nvme_controller_from_name(name).is_some() {
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
    };
    read_disk_smart_for_connection(name, connection)
}

/// Read SMART data using platform-neutral runtime transport evidence.
///
/// Hardware families remain runtime capabilities inside the standard Linux
/// artifact. Unsupported transports return a typed result and never cause a
/// vendor-specific package split.
#[cfg(feature = "test-support")]
pub fn read_disk_smart_for_connection(name: &str, connection: StorageConnection) -> DiskSmart {
    SmartProviderRegistry::standard()
        .observe(name, connection)
        .value
}

#[derive(Debug, PartialEq, Eq)]
enum SmartCommandResult {
    Output(String),
    RetryableDeviceType,
    MissingTool,
    TimedOut,
    Unavailable,
    DeviceUnavailable,
    CommandFailed,
    PermissionDenied,
    Unsupported,
}

fn parse_command_result(
    result: SmartCommandResult,
    parser: fn(&str) -> Option<DiskSmart>,
) -> DiskSmart {
    match result {
        SmartCommandResult::Output(stdout) => parser(&stdout).unwrap_or_else(|| {
            DiskSmart::with_failure(SmartProviderFailureKind::MalformedResponse)
        }),
        SmartCommandResult::RetryableDeviceType => {
            DiskSmart::with_failure(SmartProviderFailureKind::BridgeLimitation)
        }
        SmartCommandResult::MissingTool => {
            DiskSmart::with_failure(SmartProviderFailureKind::MissingTool)
        }
        SmartCommandResult::TimedOut => DiskSmart::with_failure(SmartProviderFailureKind::TimedOut),
        SmartCommandResult::Unavailable => {
            DiskSmart::with_failure(SmartProviderFailureKind::TemporarilyUnavailable)
        }
        SmartCommandResult::DeviceUnavailable => {
            DiskSmart::with_failure(SmartProviderFailureKind::DeviceUnavailable)
        }
        SmartCommandResult::CommandFailed => {
            DiskSmart::with_failure(SmartProviderFailureKind::CommandFailed)
        }
        SmartCommandResult::PermissionDenied => {
            DiskSmart::with_failure(SmartProviderFailureKind::PermissionDenied)
        }
        SmartCommandResult::Unsupported => {
            DiskSmart::with_failure(SmartProviderFailureKind::UnsupportedProtocol)
        }
    }
}

/// Read NVMe SMART/health for the disk whose phys/namespace name is `name`
/// (e.g. `nvme0n1` — the value `physical_disk_key` yields and that
/// `DiskMetrics::name` carries minus the `/dev/` prefix). The controller is
/// derived by stripping the trailing `n<ns>` (`nvme0n1` → `nvme0`).
///
/// sysfs hwmon is always attempted (cheap file reads); `nvme smart-log` is
/// attempted on top for the fields sysfs lacks. Never panics.
pub fn read_nvme_smart(name: &str) -> DiskSmart {
    let Some(ctrl) = nvme_controller_from_name(name) else {
        return DiskSmart::with_availability(SmartAvailability::Unsupported);
    };

    // The command provider determines MissingTool/Unavailable/Unsupported when
    // sysfs cannot supply any health field.
    let mut out = read_nvme_smart_log(&ctrl);

    // 1. sysfs hwmon — the verified-on-this-host source.
    let sysfs = read_sysfs_hwmon(&ctrl);
    if let Some(sysfs_value) = sysfs.value {
        out.failure = strongest_smart_failure(out.failure, sysfs_value.failure);
        out.temperature_c = sysfs_value.temperature_c.or(out.temperature_c);
        out.critical_warning = sysfs_value.critical_warning.or(out.critical_warning);
        out.temp_critical_c = sysfs_value.temp_critical_c.or(out.temp_critical_c);
        out.availability = SmartAvailability::Available;
        out.state.status = DeviceStatus::Healthy;
    } else if out.availability == SmartAvailability::Available {
        out.failure = strongest_smart_failure(out.failure, sysfs.failure);
    } else if let Some(failure) = strongest_smart_failure(out.failure, sysfs.failure) {
        out = DiskSmart::with_failure(failure);
    }

    out
}

struct SmartSysfsObservation {
    value: Option<DiskSmart>,
    failure: Option<SmartProviderFailureKind>,
}

impl SmartSysfsObservation {
    fn unavailable(failure: SmartProviderFailureKind) -> Self {
        Self {
            value: None,
            failure: Some(failure),
        }
    }
}

fn strongest_smart_failure(
    left: Option<SmartProviderFailureKind>,
    right: Option<SmartProviderFailureKind>,
) -> Option<SmartProviderFailureKind> {
    match (left, right) {
        (Some(left), Some(right))
            if smart_failure_priority(right) > smart_failure_priority(left) =>
        {
            Some(right)
        }
        (Some(left), _) => Some(left),
        (None, right) => right,
    }
}

const fn smart_failure_priority(failure: SmartProviderFailureKind) -> u8 {
    match failure {
        SmartProviderFailureKind::PermissionDenied => 7,
        SmartProviderFailureKind::MissingTool => 6,
        SmartProviderFailureKind::TimedOut => 5,
        SmartProviderFailureKind::MalformedResponse => 4,
        SmartProviderFailureKind::CommandFailed => 4,
        SmartProviderFailureKind::DeviceUnavailable
        | SmartProviderFailureKind::TemporarilyUnavailable => 3,
        SmartProviderFailureKind::BridgeLimitation => 2,
        SmartProviderFailureKind::UnsupportedProtocol => 1,
    }
}

fn stderr_is_permission_denied(stderr: &[u8]) -> bool {
    let message = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    [
        "permission denied",
        "operation not permitted",
        "insufficient privileges",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

/// Collapse an NVMe namespace / partition name to its CONTROLLER name.
/// `nvme0n1` → `nvme0`, `nvme10n2` → `nvme10`, `nvme0` → `nvme0`.
/// Returns `None` when the input isn't an `nvme<digits>...` name.
fn nvme_controller_from_name(name: &str) -> Option<String> {
    let n = name.trim_start_matches("/dev/");
    let after = n.strip_prefix("nvme")?;
    let ctrl_num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if ctrl_num.is_empty() {
        return None;
    }
    Some(format!("nvme{ctrl_num}"))
}

/// Read the NVMe hwmon nodes for a controller (e.g. `nvme0`). Scans
/// `/sys/class/nvme/<ctrl>/hwmon*` (the driver symlinks its hwmon there) and
/// takes the first hwmon with a `temp1_input`. Verified shape on this host:
/// `temp1_input`/`temp1_alarm`/`temp1_crit` are millidegrees / 0|1.
///
/// Linux-only: `/sys/class/nvme` is the kernel `nvme` driver's sysfs root, which
/// doesn't exist on macOS/Windows. Off Linux the [`read_sysfs_hwmon`] stub below
/// returns a typed unsupported observation so absence is never confused with an
/// authoritative empty health sample.
#[cfg(target_os = "linux")]
fn read_sysfs_hwmon(ctrl: &str) -> SmartSysfsObservation {
    read_sysfs_hwmon_in(std::path::Path::new("/sys/class/nvme"), ctrl)
}

#[cfg(target_os = "linux")]
fn read_sysfs_hwmon_in(base: &std::path::Path, ctrl: &str) -> SmartSysfsObservation {
    let entries = match std::fs::read_dir(base.join(ctrl)) {
        Ok(entries) => entries,
        Err(error) => return SmartSysfsObservation::unavailable(smart_io_failure(&error)),
    };
    let mut failure = None;
    for entry in entries {
        let e = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failure = strongest_smart_failure(failure, Some(smart_io_failure(&error)));
                continue;
            }
        };
        let name = e.file_name();
        if !name.to_string_lossy().starts_with("hwmon") {
            continue;
        }
        let hw = e.path();
        let temperature_c = match read_milli(&hw.join("temp1_input")) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(observed) => {
                failure = strongest_smart_failure(failure, Some(observed));
                continue;
            }
        };
        let (critical_warning, alarm_failure) = match read_hwmon_alarm(&hw.join("temp1_alarm")) {
            Ok(value) => (value, None),
            Err(observed) => (None, Some(observed)),
        };
        failure = strongest_smart_failure(failure, alarm_failure);
        let (temp_critical_c, critical_failure) = match read_milli(&hw.join("temp1_crit")) {
            Ok(value) => (value, None),
            Err(observed) => (None, Some(observed)),
        };
        failure = strongest_smart_failure(failure, critical_failure);
        return SmartSysfsObservation {
            value: Some(DiskSmart {
                availability: SmartAvailability::Available,
                state: DeviceState {
                    status: DeviceStatus::Healthy,
                    last_success_ms: None,
                },
                provider: None,
                failure,
                temperature_c: Some(temperature_c),
                // Preserve the provider's three states: a missing alarm node is
                // unknown, while a reported 0/1 is healthy/warning. Read or
                // parse failure is additionally retained in `failure`.
                critical_warning,
                temp_critical_c,
                percent_used: None,
                power_on_hours: None,
                ata_attributes: None,
            }),
            failure,
        };
    }
    SmartSysfsObservation {
        value: None,
        failure: failure.or(Some(SmartProviderFailureKind::UnsupportedProtocol)),
    }
}

#[cfg(target_os = "linux")]
fn read_hwmon_alarm(path: &std::path::Path) -> Result<Option<bool>, SmartProviderFailureKind> {
    match read_u64(path)? {
        Some(0) => Ok(Some(false)),
        Some(1) => Ok(Some(true)),
        Some(_) => Err(SmartProviderFailureKind::MalformedResponse),
        None => Ok(None),
    }
}

/// Off-Linux stub: `/sys/class/nvme` doesn't exist on macOS/Windows, so there's
/// no sysfs hwmon to read. The typed unsupported result composes with the
/// command provider's explicit off-Linux unsupported state.
#[cfg(not(target_os = "linux"))]
fn read_sysfs_hwmon(_ctrl: &str) -> SmartSysfsObservation {
    SmartSysfsObservation::unavailable(SmartProviderFailureKind::UnsupportedProtocol)
}

/// Parse `nvme smart-log /dev/<ctrl>` output (dep-free command execution).
/// Returns a snapshot with explicit MissingTool/Unavailable/Unsupported state
/// when no valid telemetry is produced.
///
/// Cross-platform spine: the Linux-only `nvme` shell-out lives in
/// [`smart_log_stdout`] (cfg-gated, returns Unsupported off-Linux). The call
/// site remains compiled on every target, keeping the pure parser reachable.
fn read_nvme_smart_log(ctrl: &str) -> DiskSmart {
    parse_command_result(smart_log_stdout(ctrl), parse_smart_log_stdout)
}

/// Capture `nvme smart-log /dev/<ctrl>` stdout (Linux-only dep-free shell-out).
/// Classifies an absent executable separately; ioctl/access failures and a
/// non-zero exit status remain conservatively Unavailable.
#[cfg(target_os = "linux")]
fn smart_log_stdout(ctrl: &str) -> SmartCommandResult {
    let dev = format!("/dev/{ctrl}");
    let mut command = std::process::Command::new("nvme");
    command.args(["smart-log", &dev]);
    let output = match run_with_timeout(&mut command, SMART_COMMAND_TIMEOUT) {
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
    if !output.status.success() {
        if stderr_is_permission_denied(&output.stderr) {
            return SmartCommandResult::PermissionDenied;
        }
        return SmartCommandResult::Unavailable;
    }
    match String::from_utf8(output.stdout) {
        Ok(stdout) => SmartCommandResult::Output(stdout),
        Err(_) => SmartCommandResult::Unavailable,
    }
}

/// Off-Linux stub: no provider is implemented, so availability is Unsupported.
#[cfg(not(target_os = "linux"))]
fn smart_log_stdout(_ctrl: &str) -> SmartCommandResult {
    SmartCommandResult::Unsupported
}

/// Pure parser that turns the captured stdout of `nvme smart-log` into an
/// [`DiskSmart`]. Extracted from [`read_nvme_smart_log`] so the parse path is
/// unit-testable without shelling out. Returns `None` when nothing recognisable
/// parses. `pub` so the inline test module (and any future in-tree caller) can
/// reach it; the enclosing `smart` mod is itself private to `collector`.
pub fn parse_smart_log_stdout(stdout: &str) -> Option<DiskSmart> {
    let mut out = DiskSmart::default();
    for line in stdout.lines() {
        let Some((key, val)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let val = val.trim();
        match key.as_str() {
            "temperature" => {
                // nvme-cli prints "39°C (312.15 K)" — the leading token is
                // already °C, so parse it directly (no Kelvin conversion).
                out.temperature_c = parse_leading_f32(val);
            }
            "critical_warning" => {
                // Emitted as "0" or "0x0"; any nonzero value is a warning.
                let raw = val.split_whitespace().next().unwrap_or(val);
                let n = raw
                    .strip_prefix("0x")
                    .and_then(|h| u64::from_str_radix(h, 16).ok())
                    .or_else(|| raw.parse::<u64>().ok());
                if let Some(n) = n {
                    out.critical_warning = Some(n != 0);
                }
            }
            "percentage_used" => {
                out.percent_used = parse_leading_f32(val);
            }
            "power_on_hours" => {
                out.power_on_hours = parse_leading_u64(val);
            }
            _ => {}
        }
    }
    // If nothing parsed, treat as unavailable. A successful parse must not
    // inherit `DiskSmart::default()`'s degraded failure/state: the default
    // claims TemporarilyUnavailable + Stale, which would contradict the
    // available fields derived from the input.
    let any = out.temperature_c.is_some()
        || out.critical_warning.is_some()
        || out.percent_used.is_some()
        || out.power_on_hours.is_some();
    if any {
        out.availability = SmartAvailability::Available;
        out.failure = None;
        out.state.status = DeviceStatus::Healthy;
        Some(out)
    } else {
        None
    }
}

// ---- tiny dep-free sysfs readers (kept local so this module is standalone) ----
// Linux-only: these feed [`read_sysfs_hwmon`] exclusively (the `/sys/class/nvme`
// millidegree nodes), and sysfs is Linux-only. Gated alongside the sysfs path
// so neither becomes an orphaned private helper on macOS/Windows `cargo build`
// under `-D warnings`. The pure parsers below (`parse_leading_*`) stay
// cross-platform — [`parse_smart_log_stdout`] uses them on every OS.

#[cfg(target_os = "linux")]
fn read_milli(p: &std::path::Path) -> Result<Option<f32>, SmartProviderFailureKind> {
    Ok(read_u64(p)?.map(|milli| milli as f32 / 1000.0))
}

#[cfg(target_os = "linux")]
fn read_u64(p: &std::path::Path) -> Result<Option<u64>, SmartProviderFailureKind> {
    match std::fs::read_to_string(p) {
        Ok(value) => value
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|_| SmartProviderFailureKind::MalformedResponse),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(smart_io_failure(&error)),
    }
}

#[cfg(target_os = "linux")]
fn smart_io_failure(error: &std::io::Error) -> SmartProviderFailureKind {
    match error.kind() {
        std::io::ErrorKind::NotFound => SmartProviderFailureKind::UnsupportedProtocol,
        std::io::ErrorKind::PermissionDenied => SmartProviderFailureKind::PermissionDenied,
        std::io::ErrorKind::TimedOut => SmartProviderFailureKind::TimedOut,
        _ => SmartProviderFailureKind::TemporarilyUnavailable,
    }
}

/// Parse the leading numeric f32 out of a value string, ignoring trailing
/// unit suffixes: `"39°C (312.15 K)"` → 39.0, `"0%"` → 0.0, `"2.5 %"` → 2.5.
fn parse_leading_f32(s: &str) -> Option<f32> {
    let num: String = s
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if num.is_empty() {
        return None;
    }
    num.parse::<f32>().ok()
}

/// Parse the leading numeric u64 out of a value string, ignoring commas /
/// suffixes: `"5432"` → 5432, `"1,234,567"` → 1.
fn parse_leading_u64(s: &str) -> Option<u64> {
    let num: String = s
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if num.is_empty() {
        return None;
    }
    num.parse::<u64>().ok()
}

#[cfg(test)]
#[path = "../../tests/headless/engine/smart/proptests.rs"]
mod proptests;
#[cfg(test)]
#[path = "../../tests/headless/linux_engine_smart_tests.rs"]
mod tests;

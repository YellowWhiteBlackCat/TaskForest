//! Linux filesystem mount-health provider.

use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::time::Duration;

use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
pub use taskmanager_core::core::storage_health::{
    FilesystemHealth, FilesystemHealthSnapshot, FilesystemHealthStatus,
};
#[cfg(target_os = "linux")]
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

#[cfg(target_os = "linux")]
const FILESYSTEM_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(target_os = "linux")]
pub fn collect_filesystem_health(now_ms: u64) -> FilesystemHealthSnapshot {
    collect_filesystem_health_from(
        Path::new("/proc/self/mountinfo"),
        Path::new("/sys/fs"),
        now_ms,
    )
}

#[cfg(not(target_os = "linux"))]
pub fn collect_filesystem_health(_now_ms: u64) -> FilesystemHealthSnapshot {
    FilesystemHealthSnapshot::default()
}

#[cfg(target_os = "linux")]
pub fn collect_filesystem_health_from(
    mountinfo_path: &Path,
    sys_fs_root: &Path,
    now_ms: u64,
) -> FilesystemHealthSnapshot {
    let text = match std::fs::read_to_string(mountinfo_path) {
        Ok(text) => text,
        Err(error) => {
            let status = if error.kind() == std::io::ErrorKind::PermissionDenied {
                DeviceStatus::PermissionDenied
            } else {
                DeviceStatus::Stale
            };
            return FilesystemHealthSnapshot {
                state: DeviceState::default().transition(status, now_ms),
                filesystems: Vec::new(),
            };
        }
    };
    let mut filesystems = parse_mountinfo(&text, now_ms);
    for filesystem in &mut filesystems {
        let integrity = match filesystem.fs_type.as_str() {
            "ext4" => read_ext4_integrity(filesystem, sys_fs_root),
            "btrfs" => read_btrfs_integrity(filesystem, sys_fs_root),
            "xfs" => read_xfs_integrity(filesystem),
            _ => IntegrityObservation::unsupported(),
        };
        filesystem.error_count = integrity.error_count;
        filesystem.integrity_state = DeviceState::default().transition(integrity.status, now_ms);
        if filesystem.error_count.is_some_and(|count| count > 0) {
            filesystem.status = FilesystemHealthStatus::ErrorsReported;
        }
        filesystem.status = if filesystem.error_count.is_some_and(|count| count > 0) {
            FilesystemHealthStatus::ErrorsReported
        } else if filesystem.read_only == Some(true) {
            FilesystemHealthStatus::ReadOnly
        } else {
            FilesystemHealthStatus::Healthy
        };
    }
    FilesystemHealthSnapshot {
        state: DeviceState::healthy(now_ms),
        filesystems,
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
struct IntegrityObservation {
    error_count: Option<u64>,
    status: DeviceStatus,
}

#[cfg(target_os = "linux")]
impl IntegrityObservation {
    fn unsupported() -> Self {
        Self {
            error_count: None,
            status: DeviceStatus::Unsupported,
        }
    }
}

#[cfg(target_os = "linux")]
fn read_ext4_integrity(filesystem: &FilesystemHealth, sys_fs_root: &Path) -> IntegrityObservation {
    let Some(device) = filesystem
        .source
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
    else {
        return IntegrityObservation::unsupported();
    };
    let error_path = sys_fs_root.join("ext4").join(device).join("errors_count");
    match std::fs::read_to_string(error_path) {
        Ok(value) => match value.trim().parse() {
            Ok(error_count) => IntegrityObservation {
                error_count: Some(error_count),
                status: DeviceStatus::Healthy,
            },
            Err(_) => IntegrityObservation {
                error_count: None,
                status: DeviceStatus::Stale,
            },
        },
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            IntegrityObservation {
                error_count: None,
                status: DeviceStatus::PermissionDenied,
            }
        }
        Err(_) => IntegrityObservation::unsupported(),
    }
}

#[cfg(target_os = "linux")]
fn read_btrfs_integrity(filesystem: &FilesystemHealth, sys_fs_root: &Path) -> IntegrityObservation {
    let Some(device) = filesystem
        .source
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .and_then(|name| name.split('[').next())
    else {
        return IntegrityObservation::unsupported();
    };
    let root = sys_fs_root.join("btrfs");
    let Ok(filesystems) = std::fs::read_dir(root) else {
        return IntegrityObservation::unsupported();
    };
    for entry in filesystems.flatten() {
        let directory = entry.path();
        if !directory.join("devices").join(device).exists() {
            continue;
        }
        let Ok(devices) = std::fs::read_dir(directory.join("devinfo")) else {
            return IntegrityObservation {
                error_count: None,
                status: DeviceStatus::Stale,
            };
        };
        let mut total = 0u64;
        let mut observed = false;
        for device_info in devices.flatten() {
            match std::fs::read_to_string(device_info.path().join("error_stats")) {
                Ok(text) => {
                    if let Some(count) = parse_btrfs_error_stats(&text) {
                        observed = true;
                        total = total.saturating_add(count);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                    return IntegrityObservation {
                        error_count: None,
                        status: DeviceStatus::PermissionDenied,
                    };
                }
                Err(_) => {}
            }
        }
        return IntegrityObservation {
            error_count: observed.then_some(total),
            status: if observed {
                DeviceStatus::Healthy
            } else {
                DeviceStatus::Stale
            },
        };
    }
    IntegrityObservation {
        error_count: None,
        status: DeviceStatus::Stale,
    }
}

#[cfg(target_os = "linux")]
fn read_xfs_integrity(filesystem: &FilesystemHealth) -> IntegrityObservation {
    let mut command = Command::new("xfs_spaceman");
    command.args(["-c", "health", &filesystem.mount_point.to_string_lossy()]);
    match run_with_timeout(&mut command, FILESYSTEM_HEALTH_TIMEOUT) {
        Ok(output) if output.status.success() => {
            match parse_xfs_health_output(&String::from_utf8_lossy(&output.stdout)) {
                Some(error_count) => IntegrityObservation {
                    error_count: Some(error_count),
                    status: DeviceStatus::Healthy,
                },
                None => IntegrityObservation {
                    error_count: None,
                    status: DeviceStatus::Stale,
                },
            }
        }
        Ok(output) if permission_denied(&output.stderr) => IntegrityObservation {
            error_count: None,
            status: DeviceStatus::PermissionDenied,
        },
        Ok(_)
        | Err(
            BoundedCommandError::TimedOut
            | BoundedCommandError::ReaderTimedOut
            | BoundedCommandError::ReaderStart(_)
            | BoundedCommandError::ReaderFailed
            | BoundedCommandError::ProcessTree
            | BoundedCommandError::OutputTooLarge,
        ) => IntegrityObservation {
            error_count: None,
            status: DeviceStatus::Stale,
        },
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            IntegrityObservation {
                error_count: None,
                status: DeviceStatus::MissingTool,
            }
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            IntegrityObservation {
                error_count: None,
                status: DeviceStatus::PermissionDenied,
            }
        }
        Err(BoundedCommandError::Spawn(_)) => IntegrityObservation {
            error_count: None,
            status: DeviceStatus::Stale,
        },
    }
}

pub fn parse_btrfs_error_stats(text: &str) -> Option<u64> {
    let mut observed = false;
    let mut total = 0u64;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else { continue };
        if !matches!(
            name,
            "write_errs" | "read_errs" | "flush_errs" | "corruption_errs" | "generation_errs"
        ) {
            continue;
        }
        let value = fields.next()?.parse::<u64>().ok()?;
        observed = true;
        total = total.saturating_add(value);
    }
    observed.then_some(total)
}

pub fn parse_xfs_health_output(text: &str) -> Option<u64> {
    let mut observed = false;
    let mut warnings = 0u64;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("healthy") || lower.contains("clean") {
            observed = true;
        }
        if lower.contains("sick") || lower.contains("corrupt") || lower.contains("unhealthy") {
            observed = true;
            warnings = warnings.saturating_add(1);
        }
        let fields = lower.split_whitespace().collect::<Vec<_>>();
        for pair in fields.windows(2) {
            if pair[1].starts_with("warning")
                && let Ok(count) = pair[0]
                    .trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<u64>()
            {
                observed = true;
                warnings = warnings.saturating_add(count);
            }
        }
    }
    observed.then_some(warnings)
}

#[cfg(target_os = "linux")]
fn permission_denied(stderr: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    text.contains("permission denied") || text.contains("operation not permitted")
}

pub fn parse_mountinfo(text: &str, now_ms: u64) -> Vec<FilesystemHealth> {
    text.lines()
        .filter_map(|line| {
            let (mount, fs) = line.split_once(" - ")?;
            let mount_fields = mount.split_whitespace().collect::<Vec<_>>();
            let fs_fields = fs.split_whitespace().collect::<Vec<_>>();
            let mount_point = PathBuf::from(unescape_mount_field(mount_fields.get(4)?));
            let read_only = mount_fields.get(5).map(|options| {
                options
                    .split(',')
                    .any(|option| option.eq_ignore_ascii_case("ro"))
            });
            let fs_type = (*fs_fields.first()?).to_owned();
            let source = fs_fields
                .get(1)
                .filter(|source| **source != "none")
                .map(|source| PathBuf::from(unescape_mount_field(source)));
            Some(FilesystemHealth {
                mount_point,
                source,
                fs_type,
                read_only,
                error_count: None,
                status: if read_only == Some(true) {
                    FilesystemHealthStatus::ReadOnly
                } else {
                    FilesystemHealthStatus::Healthy
                },
                state: DeviceState::healthy(now_ms),
                integrity_state: DeviceState::default(),
            })
        })
        .collect()
}

fn unescape_mount_field(value: &str) -> String {
    value
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_storage_health_tests.rs"]
mod tests;

//! Systemd / OpenRC service enumeration, status mapping, and lifecycle control.

pub mod log_stream;
#[cfg(feature = "test-support")]
pub use log_stream::{ServiceLogStreamRequestError, ServiceLogStreamWorker};
mod log_fetch;
#[cfg(all(test, not(feature = "test-support")))]
pub(crate) use log_fetch::ServiceLogWorker;
#[cfg(feature = "test-support")]
pub use log_fetch::{ServiceLogCommandOutcome, ServiceLogWorker, classify_service_log_outcome};
#[cfg(not(feature = "test-support"))]
pub(crate) use log_fetch::{ServiceLogCommandOutcome, classify_service_log_outcome};
mod control;
mod dependencies;
mod init_runtime;
pub use init_runtime::InitSystem;
pub(crate) mod inventory;
mod parsing;
mod target;
#[cfg(feature = "test-support")]
pub use parsing::{
    parse_openrc_description, parse_openrc_status, parse_openrc_update, parse_systemctl_show_deps,
    parse_unit_description,
};
#[cfg(not(feature = "test-support"))]
pub(crate) use parsing::{parse_openrc_status, parse_openrc_update, parse_systemctl_show_deps};
pub(crate) use target::{valid_openrc_service_name, valid_systemd_service_name};

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
#[cfg(feature = "test-support")]
pub use taskmanager_core::core::services::{
    ServiceAction, ServiceDeps, ServiceItem, ServiceLogAvailability, ServiceLogEntries,
    ServiceLogEntry, ServiceLogErrorKind, ServiceLogFailure, ServiceLogFeed, ServiceLogLevel,
    ServiceLogLevelFilter, ServiceLogLines, ServiceLogProviderState, ServiceLogQuery,
    ServiceLogSnapshot, ServiceLogState, ServiceLogStreamEnd, ServiceLogStreamSnapshot,
    ServiceLogStreamState, ServiceLogTimeFilter, ServiceRelationEdge, ServiceRelationGraph,
    ServiceRelationKind, ServiceStatus,
};
#[cfg(not(feature = "test-support"))]
pub(crate) use taskmanager_core::core::services::{
    ServiceDeps, ServiceItem, ServiceLogEntry, ServiceLogErrorKind, ServiceLogFailure,
    ServiceLogLevel, ServiceLogQuery, ServiceLogState, ServiceLogStreamState, ServiceLogTimeFilter,
    ServiceRelationKind, ServiceStatus,
};
#[cfg(all(test, not(feature = "test-support")))]
pub(crate) use taskmanager_core::core::services::{
    ServiceLogAvailability, ServiceLogFeed, ServiceLogLevelFilter, ServiceLogSnapshot,
    ServiceLogStreamEnd, ServiceLogStreamSnapshot, ServiceRelationEdge,
};

#[cfg(target_os = "linux")]
use taskmanager_platform_portable::BoundedCommandError;
#[cfg(unix)]
use taskmanager_platform_portable::run_with_timeout;

pub const SERVICE_LOG_LINE_LIMIT: usize = 50;
pub const SERVICE_LOG_TIMEOUT: Duration = Duration::from_secs(2);
pub const SERVICE_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

pub struct ServiceManager;

impl ServiceManager {
    /// Scans unit files directly in systemd service directories
    #[cfg(target_os = "linux")]
    pub fn scan_unit_files() -> Vec<ServiceItem> {
        let mut services = Vec::new();
        let mut seen = HashSet::new();
        let paths = [
            "/etc/systemd/system",
            "/usr/lib/systemd/system",
            "/lib/systemd/system",
        ];

        for dir in paths {
            let path = Path::new(dir);
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if target::valid_systemd_service_name(&file_name) {
                        let name = file_name
                            .strip_suffix(".service")
                            .unwrap_or(&file_name)
                            .to_string();
                        if !seen.insert(name.clone()) {
                            continue;
                        }

                        let description = Self::extract_description(&entry.path())
                            .unwrap_or_else(|| "Systemd Service Unit".to_string());

                        services.push(ServiceItem::from_inventory(
                            target::systemd_service_id(&file_name),
                            name,
                            ServiceStatus::Inactive,
                            description,
                            "loaded",
                            "unknown",
                            "unknown",
                        ));
                    }
                }
            }
        }

        services
    }

    #[cfg(target_os = "linux")]
    fn extract_description(path: &Path) -> Option<String> {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("Description=") {
                    return Some(rest.trim().to_string());
                }
            }
        }
        None
    }

    /// Read the latest journal entries for one canonical service target.
    ///
    /// This method is blocking by design and product code calls it only from
    /// the shared service runtime lane. The spawned command is killed after
    /// [`SERVICE_LOG_TIMEOUT`].
    pub fn fetch_logs(
        target: &taskmanager_core::ServiceId,
    ) -> Result<ServiceLogState, taskmanager_platform_contract::ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            let target = target::resolve_active_service_target(target)?;
            if target.init() != InitSystem::Systemd {
                return Err(taskmanager_platform_contract::ProviderFailure::Unsupported);
            }
            let unit = target.native();
            let mut command = Command::new("journalctl");
            command.args([
                "--unit",
                unit,
                "--lines",
                "50",
                "--no-pager",
                "--output=short-iso",
                "--quiet",
            ]);
            Ok(classify_service_log_outcome(run_command_with_timeout(
                command,
                SERVICE_LOG_TIMEOUT,
            )))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = target;
            Err(taskmanager_platform_contract::ProviderFailure::Unsupported)
        }
    }

    /// Read one bounded increment of the provider's structured log stream.
    /// Native application adapters call this on their worker thread.
    pub fn fetch_log_stream(
        query: &ServiceLogQuery,
        observed_at_ms: u64,
    ) -> Result<ServiceLogStreamState, taskmanager_platform_contract::ProviderFailure> {
        log_stream::fetch(query, observed_at_ms)
    }

    // ── macOS / Windows stubs ────────────────────────────────────────────────
    // systemctl / rc-status / rc-update / rc-service / systemd-analyze and the
    // `/proc/1/comm` init probe are Linux-only. Off Linux the scan returns an
    // empty service list, the control actions return an `Err`, and the init
    // probe reports `Unsupported`. The
    // pure parsers (`parse_openrc_status`, `parse_openrc_update`,
    // `parse_systemctl_show_deps`, `parse_unit_description`,
    // `parse_openrc_description`) stay cross-platform so their unit tests run.
    // This crate intentionally owns only the Linux provider.
    #[cfg(not(target_os = "linux"))]
    pub fn scan_unit_files() -> Vec<ServiceItem> {
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn run_command_with_timeout(mut command: Command, timeout: Duration) -> ServiceLogCommandOutcome {
    match run_with_timeout(&mut command, timeout) {
        Ok(output) => ServiceLogCommandOutcome::Exited {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut) => {
            ServiceLogCommandOutcome::Failure(ServiceLogFailure::with_detail(
                ServiceLogErrorKind::TimedOut,
                format!("journalctl timed out after {} ms", timeout.as_millis()),
            ))
        }
        Err(BoundedCommandError::Spawn(error)) => {
            let kind = match error.kind() {
                std::io::ErrorKind::NotFound => ServiceLogErrorKind::MissingTool,
                std::io::ErrorKind::PermissionDenied => ServiceLogErrorKind::PermissionDenied,
                _ => ServiceLogErrorKind::ProviderFailed,
            };
            ServiceLogCommandOutcome::Failure(ServiceLogFailure::with_detail(
                kind,
                error.to_string(),
            ))
        }
        Err(
            BoundedCommandError::ReaderStart(_)
            | BoundedCommandError::ReaderFailed
            | BoundedCommandError::ProcessTree,
        ) => ServiceLogCommandOutcome::Failure(ServiceLogFailure::with_detail(
            ServiceLogErrorKind::ProviderFailed,
            "failed while waiting for journalctl",
        )),
        Err(BoundedCommandError::OutputTooLarge) => {
            ServiceLogCommandOutcome::Failure(ServiceLogFailure::with_detail(
                ServiceLogErrorKind::ProviderFailed,
                "journalctl output exceeded the hard capture limit",
            ))
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/engine/services.rs"]
mod tests;

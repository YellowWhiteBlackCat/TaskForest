//! Typed systemd-user failure and critical-chain evidence.

use std::process::Command;
use std::time::Duration;

use taskmanager_core::FailureKind;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::startup::{
    StartupBootEvidenceSnapshot, StartupCriticalChainNode, StartupEvidenceFailure,
    StartupFailedUnit,
};
#[cfg(target_os = "linux")]
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

use super::parse_duration_to_millis;
use crate::engine::services::{InitSystem, ServiceManager};

const STARTUP_EVIDENCE_TIMEOUT: Duration = Duration::from_secs(2);

pub fn parse_systemd_failed_units(output: &str) -> Vec<StartupFailedUnit> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let unit = fields.next()?;
            let load_state = fields.next()?;
            let active_state = fields.next()?;
            let sub_state = fields.next()?;
            if !unit.contains('.') {
                return None;
            }
            Some(StartupFailedUnit {
                unit: unit.to_owned(),
                load_state: load_state.to_owned(),
                active_state: active_state.to_owned(),
                sub_state: sub_state.to_owned(),
                description: fields.collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

pub fn parse_systemd_critical_chain(output: &str) -> Vec<StartupCriticalChainNode> {
    output
        .lines()
        .filter_map(|raw| {
            let line = raw.trim_start_matches(|character: char| {
                character.is_whitespace() || matches!(character, '└' | '├' | '─' | '│')
            });
            let mut fields = line.split_whitespace();
            let unit = fields.next()?;
            if !unit.contains('.') || unit.ends_with(':') {
                return None;
            }
            let mut activated_at_ms = None;
            let mut duration_ms = None;
            for token in fields {
                if let Some(duration) = token.strip_prefix('@').and_then(parse_duration_to_millis) {
                    activated_at_ms = Some(duration);
                } else if let Some(duration) =
                    token.strip_prefix('+').and_then(parse_duration_to_millis)
                {
                    duration_ms = Some(duration);
                }
            }
            Some(StartupCriticalChainNode {
                unit: unit.to_owned(),
                activated_at_ms,
                duration_ms,
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub fn collect_startup_boot_evidence(now_ms: u64) -> StartupBootEvidenceSnapshot {
    if let Some(unavailable) = unavailable_for_init(ServiceManager::detect_init(), now_ms) {
        return unavailable;
    }
    let mut failed_command = Command::new("systemctl");
    failed_command.args([
        "--user",
        "list-units",
        "--state=failed",
        "--no-legend",
        "--no-pager",
        "--plain",
    ]);
    let failed = collect_command(failed_command, now_ms, parse_systemd_failed_units);

    let mut chain_command = Command::new("systemd-analyze");
    chain_command.args(["--user", "critical-chain", "--no-pager"]);
    let chain = collect_command(chain_command, now_ms, parse_systemd_critical_chain);
    let state = aggregate_state(failed.state, chain.state, now_ms);
    StartupBootEvidenceSnapshot {
        state,
        failed_units_state: failed.state,
        critical_chain_state: chain.state,
        failed_units_failure: failed.failure,
        critical_chain_failure: chain.failure,
        failed_units: failed.values,
        critical_chain: chain.values,
    }
}

fn unavailable_for_init(
    detection: Result<InitSystem, FailureKind>,
    now_ms: u64,
) -> Option<StartupBootEvidenceSnapshot> {
    let (failure, status) = match detection {
        Ok(InitSystem::Systemd) => return None,
        Ok(InitSystem::Openrc | InitSystem::Unsupported) | Err(FailureKind::Unsupported) => (
            StartupEvidenceFailure::Unsupported,
            DeviceStatus::Unsupported,
        ),
        Err(FailureKind::MissingDependency) => (
            StartupEvidenceFailure::MissingTool,
            DeviceStatus::MissingTool,
        ),
        Err(FailureKind::PermissionDenied | FailureKind::RequiresEscalation) => (
            StartupEvidenceFailure::PermissionDenied,
            DeviceStatus::PermissionDenied,
        ),
        Err(FailureKind::TimedOut) => (StartupEvidenceFailure::TimedOut, DeviceStatus::Stale),
        Err(
            FailureKind::IdentityChanged
            | FailureKind::TemporarilyUnavailable
            | FailureKind::Rejected
            | FailureKind::ProviderFault,
        ) => (StartupEvidenceFailure::Unavailable, DeviceStatus::Stale),
    };
    let state = DeviceState::default().transition(status, now_ms);
    Some(StartupBootEvidenceSnapshot {
        state,
        failed_units_state: state,
        critical_chain_state: state,
        failed_units_failure: Some(failure),
        critical_chain_failure: Some(failure),
        ..Default::default()
    })
}

#[cfg(not(target_os = "linux"))]
pub fn collect_startup_boot_evidence(now_ms: u64) -> StartupBootEvidenceSnapshot {
    let state = DeviceState::default().transition(DeviceStatus::Unsupported, now_ms);
    StartupBootEvidenceSnapshot {
        state,
        failed_units_state: state,
        critical_chain_state: state,
        failed_units_failure: Some(StartupEvidenceFailure::Unsupported),
        critical_chain_failure: Some(StartupEvidenceFailure::Unsupported),
        ..Default::default()
    }
}

#[cfg(target_os = "linux")]
struct EvidenceResult<T> {
    state: DeviceState,
    failure: Option<StartupEvidenceFailure>,
    values: Vec<T>,
}

#[cfg(target_os = "linux")]
fn collect_command<T>(
    mut command: Command,
    now_ms: u64,
    parser: fn(&str) -> Vec<T>,
) -> EvidenceResult<T> {
    match run_with_timeout(&mut command, STARTUP_EVIDENCE_TIMEOUT) {
        Ok(output) if output.status.success() => EvidenceResult {
            state: DeviceState::healthy(now_ms),
            failure: None,
            values: parser(&String::from_utf8_lossy(&output.stdout)),
        },
        Ok(output) if permission_denied(&output.stderr) => failed_evidence(
            StartupEvidenceFailure::PermissionDenied,
            DeviceStatus::PermissionDenied,
            now_ms,
        ),
        Ok(_) => failed_evidence(
            StartupEvidenceFailure::Unavailable,
            DeviceStatus::Stale,
            now_ms,
        ),
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            failed_evidence(
                StartupEvidenceFailure::MissingTool,
                DeviceStatus::MissingTool,
                now_ms,
            )
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            failed_evidence(
                StartupEvidenceFailure::PermissionDenied,
                DeviceStatus::PermissionDenied,
                now_ms,
            )
        }
        Err(BoundedCommandError::TimedOut) => failed_evidence(
            StartupEvidenceFailure::TimedOut,
            DeviceStatus::Stale,
            now_ms,
        ),
        Err(_) => failed_evidence(
            StartupEvidenceFailure::Unavailable,
            DeviceStatus::Stale,
            now_ms,
        ),
    }
}

#[cfg(target_os = "linux")]
fn failed_evidence<T>(
    failure: StartupEvidenceFailure,
    status: DeviceStatus,
    now_ms: u64,
) -> EvidenceResult<T> {
    EvidenceResult {
        state: DeviceState::default().transition(status, now_ms),
        failure: Some(failure),
        values: Vec::new(),
    }
}

fn aggregate_state(left: DeviceState, right: DeviceState, now_ms: u64) -> DeviceState {
    let status = if [left.status, right.status].contains(&DeviceStatus::PermissionDenied) {
        DeviceStatus::PermissionDenied
    } else if [left.status, right.status].contains(&DeviceStatus::MissingTool) {
        DeviceStatus::MissingTool
    } else if [left.status, right.status].contains(&DeviceStatus::Stale) {
        DeviceStatus::Stale
    } else if [left.status, right.status].contains(&DeviceStatus::Healthy) {
        DeviceStatus::Healthy
    } else {
        DeviceStatus::Unsupported
    };
    DeviceState::default().transition(status, now_ms)
}

#[cfg(target_os = "linux")]
fn permission_denied(stderr: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    text.contains("permission denied") || text.contains("operation not permitted")
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_startup_evidence_tests.rs"]
mod tests;

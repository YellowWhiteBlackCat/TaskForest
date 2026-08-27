//! Typed init-system selection and service inventory command outcomes.

use std::io;
use std::process::Command;
use std::time::Duration;

use taskmanager_core::{FailureKind, ProviderId, SourceOutcome, SourceStatus};
use taskmanager_platform_contract::PartialSourceSnapshot;

use super::parsing::extract_openrc_description;
use super::target::{systemd_service_id, valid_openrc_service_name, valid_systemd_service_name};
use super::{
    InitSystem, SERVICE_COMMAND_TIMEOUT, ServiceItem, ServiceManager, ServiceStatus,
    parse_openrc_status, parse_openrc_update,
};
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const SYSTEMD_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("linux.service.systemd");
const OPENRC_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("linux.service.openrc");
const UNSUPPORTED_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.service.unsupported-init");
const INIT_DETECTION_PROVIDER: ProviderId = ProviderId::borrowed("linux.service.init-detection");

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InventoryCommandResult {
    Success(String),
    Failure(FailureKind),
}

trait InventoryCommandRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> InventoryCommandResult;
}

struct NativeInventoryCommandRunner;

impl InventoryCommandRunner for NativeInventoryCommandRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> InventoryCommandResult {
        run_inventory_command(program, args, SERVICE_COMMAND_TIMEOUT)
    }
}

pub(crate) fn run_inventory_command(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> InventoryCommandResult {
    let mut command = Command::new(program);
    command.args(args);
    match run_with_timeout(&mut command, timeout) {
        Ok(output) if output.status.success() => {
            InventoryCommandResult::Success(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(_) => InventoryCommandResult::Failure(classify_nonzero_exit()),
        Err(BoundedCommandError::Spawn(error)) => {
            InventoryCommandResult::Failure(classify_spawn_error(error.kind()))
        }
        Err(BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut) => {
            InventoryCommandResult::Failure(FailureKind::TimedOut)
        }
        Err(
            BoundedCommandError::ReaderStart(_)
            | BoundedCommandError::ReaderFailed
            | BoundedCommandError::ProcessTree
            | BoundedCommandError::OutputTooLarge,
        ) => InventoryCommandResult::Failure(FailureKind::ProviderFault),
    }
}

struct ParsedInventory {
    items: Vec<ServiceItem>,
    malformed: bool,
}

struct InventoryObservation {
    items: Vec<ServiceItem>,
    outcome: SourceOutcome,
}

impl InventoryObservation {
    fn complete(items: Vec<ServiceItem>) -> Self {
        let outcome = if items.is_empty() {
            SourceOutcome::Empty
        } else {
            SourceOutcome::Available
        };
        Self { items, outcome }
    }

    fn failed(items: Vec<ServiceItem>, failure: FailureKind) -> Self {
        let outcome = if items.is_empty() {
            SourceOutcome::Unavailable(failure)
        } else {
            SourceOutcome::Partial(failure)
        };
        Self { items, outcome }
    }

    fn recovered(items: Vec<ServiceItem>, failure: FailureKind) -> Self {
        Self {
            items,
            outcome: SourceOutcome::Partial(failure),
        }
    }
}

impl ServiceManager {
    /// Scan through exactly one init backend selected at runtime. The returned
    /// source identifies that backend and preserves command failure state, so
    /// an empty vector is authoritative only when the selected command
    /// completed successfully with a valid empty response.
    pub fn scan_snapshot() -> PartialSourceSnapshot<ServiceItem> {
        let mut runner = NativeInventoryCommandRunner;
        Self::scan_snapshot_from_detection(Self::detect_init(), &mut runner, Self::scan_unit_files)
    }

    fn scan_snapshot_with(
        init: InitSystem,
        runner: &mut impl InventoryCommandRunner,
        systemd_fallback: impl FnOnce() -> Vec<ServiceItem>,
    ) -> PartialSourceSnapshot<ServiceItem> {
        let (provider, observation) = match init {
            InitSystem::Systemd => (
                SYSTEMD_INVENTORY_PROVIDER,
                scan_systemd(runner, systemd_fallback),
            ),
            InitSystem::Openrc => (OPENRC_INVENTORY_PROVIDER, scan_openrc(runner)),
            InitSystem::Unsupported => (
                UNSUPPORTED_INVENTORY_PROVIDER,
                InventoryObservation {
                    items: Vec::new(),
                    outcome: SourceOutcome::Unavailable(FailureKind::Unsupported),
                },
            ),
        };
        let item_count = observation.items.len();
        PartialSourceSnapshot::new(
            observation.items,
            vec![SourceStatus {
                provider,
                outcome: observation.outcome,
                item_count,
            }],
        )
    }

    fn scan_snapshot_from_detection(
        detection: Result<InitSystem, FailureKind>,
        runner: &mut impl InventoryCommandRunner,
        systemd_fallback: impl FnOnce() -> Vec<ServiceItem>,
    ) -> PartialSourceSnapshot<ServiceItem> {
        match detection {
            Ok(init) => Self::scan_snapshot_with(init, runner, systemd_fallback),
            Err(failure) => PartialSourceSnapshot::new(
                Vec::new(),
                vec![SourceStatus {
                    provider: INIT_DETECTION_PROVIDER,
                    outcome: SourceOutcome::Unavailable(failure),
                    item_count: 0,
                }],
            ),
        }
    }
}

fn scan_systemd(
    runner: &mut impl InventoryCommandRunner,
    systemd_fallback: impl FnOnce() -> Vec<ServiceItem>,
) -> InventoryObservation {
    let args = [
        "list-units",
        "--type=service",
        "--no-pager",
        "--plain",
        "--all",
        "--no-legend",
    ];
    match runner.run("systemctl", &args) {
        InventoryCommandResult::Success(stdout) => {
            let parsed = parse_systemd_inventory(&stdout);
            if !parsed.malformed {
                InventoryObservation::complete(parsed.items)
            } else if parsed.items.is_empty() {
                InventoryObservation::failed(systemd_fallback(), FailureKind::ProviderFault)
            } else {
                InventoryObservation::failed(parsed.items, FailureKind::ProviderFault)
            }
        }
        InventoryCommandResult::Failure(failure) => {
            InventoryObservation::failed(systemd_fallback(), failure)
        }
    }
}

fn scan_openrc(runner: &mut impl InventoryCommandRunner) -> InventoryObservation {
    match runner.run("rc-status", &["--servicelist"]) {
        InventoryCommandResult::Success(stdout) => {
            let parsed = parse_openrc_status_inventory(&stdout);
            if !parsed.malformed {
                describe_openrc(InventoryObservation::complete(parsed.items))
            } else if parsed.items.is_empty() {
                scan_openrc_update(runner, FailureKind::ProviderFault)
            } else {
                describe_openrc(InventoryObservation::failed(
                    parsed.items,
                    FailureKind::ProviderFault,
                ))
            }
        }
        InventoryCommandResult::Failure(failure) => scan_openrc_update(runner, failure),
    }
}

fn scan_openrc_update(
    runner: &mut impl InventoryCommandRunner,
    primary_failure: FailureKind,
) -> InventoryObservation {
    match runner.run("rc-update", &["show"]) {
        InventoryCommandResult::Success(stdout) => {
            let parsed = parse_openrc_update_inventory(&stdout);
            let failure = if parsed.malformed {
                preferred_failure(primary_failure, FailureKind::ProviderFault)
            } else {
                primary_failure
            };
            describe_openrc(InventoryObservation::recovered(parsed.items, failure))
        }
        InventoryCommandResult::Failure(fallback_failure) => InventoryObservation::failed(
            Vec::new(),
            preferred_failure(primary_failure, fallback_failure),
        ),
    }
}

fn describe_openrc(mut observation: InventoryObservation) -> InventoryObservation {
    for service in &mut observation.items {
        if service.description.is_empty() {
            service.description = extract_openrc_description(&service.name)
                .unwrap_or_else(|| "OpenRC Service".to_string());
        }
    }
    observation
}

fn parse_systemd_inventory(output: &str) -> ParsedInventory {
    let mut items = Vec::new();
    let mut malformed = false;
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 4 || !valid_systemd_service_name(parts[0]) {
            malformed = true;
            continue;
        }
        let raw_name = parts[0];
        let active_state = parts[2].to_string();
        items.push(ServiceItem::from_inventory(
            systemd_service_id(raw_name),
            raw_name
                .strip_suffix(".service")
                .unwrap_or(raw_name)
                .to_string(),
            ServiceStatus::from(active_state.as_str()),
            parts
                .get(4..)
                .map_or_else(String::new, |tail| tail.join(" ")),
            parts[1],
            active_state,
            parts[3],
        ));
    }
    ParsedInventory { items, malformed }
}

fn parse_openrc_status_inventory(output: &str) -> ParsedInventory {
    let items = parse_openrc_status(output);
    let malformed = output.lines().map(str::trim).any(|line| {
        !line.is_empty()
            && !line.starts_with("Runlevel:")
            && !line.starts_with("Dynamic Runlevel:")
            && !openrc_status_row_is_valid(line)
    });
    ParsedInventory { items, malformed }
}

fn openrc_status_row_is_valid(line: &str) -> bool {
    let Some((name, state)) = line.split_once('[') else {
        return false;
    };
    let native_name = name.split_whitespace().next().unwrap_or_default();
    valid_openrc_service_name(native_name)
        && state
            .split_once(']')
            .is_some_and(|(inside, _)| !inside.trim().is_empty())
}

fn parse_openrc_update_inventory(output: &str) -> ParsedInventory {
    let items = parse_openrc_update(output);
    let malformed = output.lines().map(str::trim).any(|line| {
        if line.is_empty() {
            return false;
        }
        let Some((name, runlevels)) = line.split_once('|') else {
            return true;
        };
        !valid_openrc_service_name(name.trim()) || runlevels.trim().is_empty()
    });
    ParsedInventory { items, malformed }
}

fn classify_spawn_error(kind: io::ErrorKind) -> FailureKind {
    match kind {
        io::ErrorKind::NotFound => FailureKind::MissingDependency,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        _ => FailureKind::ProviderFault,
    }
}

const fn classify_nonzero_exit() -> FailureKind {
    // The command ran and rejected the operation. Its stderr is provider- and
    // locale-specific text, so it must never be used as a failure discriminator.
    FailureKind::Rejected
}

fn preferred_failure(left: FailureKind, right: FailureKind) -> FailureKind {
    if failure_priority(right) > failure_priority(left) {
        right
    } else {
        left
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::TimedOut => 7,
        FailureKind::MissingDependency => 6,
        FailureKind::Rejected => 5,
        FailureKind::ProviderFault => 4,
        FailureKind::TemporarilyUnavailable => 3,
        FailureKind::Unsupported => 2,
        FailureKind::IdentityChanged => 1,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_services_inventory_tests.rs"]
mod tests;

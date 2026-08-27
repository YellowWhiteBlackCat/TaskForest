//! Mutually exclusive init-specific startup sources selected at runtime.

use std::time::Duration;

use taskmanager_core::{FailureKind, ProviderId, SourceStatus};
use taskmanager_platform_contract::ProviderFailure;

use super::{
    BlameSnapshot, STARTUP_CONTROL_TIMEOUT, SYSTEMD_PROVIDER_ID, StartupControlPolicy,
    StartupEntry, StartupImpact, StartupImpactEvidence, StartupImpactUnknownReason, StartupManager,
    StartupScope, StartupSource, blame_source_status, native_startup_id, parse_systemd_blame,
    source_status,
};
use crate::engine::services::inventory::{InventoryCommandResult, run_inventory_command};
use crate::engine::services::{
    InitSystem, ServiceManager, parse_openrc_update, valid_openrc_service_name,
    valid_systemd_service_name,
};

const OPENRC_PROVIDER_ID: ProviderId = ProviderId::borrowed("linux.startup.openrc");
const UNSUPPORTED_PROVIDER_ID: ProviderId = ProviderId::borrowed("linux.startup.unsupported-init");
const INIT_DETECTION_PROVIDER_ID: ProviderId = ProviderId::borrowed("linux.startup.init-detection");

pub(super) struct SelectedInitSnapshot {
    pub(super) entries: Vec<StartupEntry>,
    pub(super) sources: Vec<SourceStatus>,
    pub(super) blame: BlameSnapshot,
}

impl StartupManager {
    pub(super) fn scan_selected_init_sources(&self) -> SelectedInitSnapshot {
        let mut runner = run_inventory_command;
        match ServiceManager::detect_init() {
            Ok(init) => self.scan_selected_init_sources_with(init, &mut runner),
            Err(failure) => SelectedInitSnapshot {
                entries: Vec::new(),
                sources: vec![source_status(INIT_DETECTION_PROVIDER_ID, 0, Some(failure))],
                blame: BlameSnapshot::NotSelected,
            },
        }
    }

    fn scan_selected_init_sources_with(
        &self,
        init: InitSystem,
        runner: &mut impl FnMut(&str, &[&str], Duration) -> InventoryCommandResult,
    ) -> SelectedInitSnapshot {
        match init {
            InitSystem::Systemd => {
                let (entries, systemd_status) = scan_systemd_startup(runner);
                let blame = scan_systemd_blame(runner);
                let blame_status = blame_source_status(&blame);
                SelectedInitSnapshot {
                    entries,
                    sources: vec![systemd_status, blame_status],
                    blame,
                }
            }
            InitSystem::Openrc => {
                let (entries, status) = scan_openrc_startup(runner);
                SelectedInitSnapshot {
                    entries,
                    sources: vec![status],
                    blame: BlameSnapshot::NotSelected,
                }
            }
            InitSystem::Unsupported => SelectedInitSnapshot {
                entries: Vec::new(),
                sources: vec![source_status(
                    UNSUPPORTED_PROVIDER_ID,
                    0,
                    Some(FailureKind::Unsupported),
                )],
                blame: BlameSnapshot::NotSelected,
            },
        }
    }

    pub(super) fn set_init_source_enabled(
        &self,
        entry: &StartupEntry,
        enabled: bool,
    ) -> Result<(), ProviderFailure> {
        let mut detector = ServiceManager::detect_init;
        let mut runner = run_inventory_command;
        self.set_init_source_enabled_with(entry, enabled, &mut detector, &mut runner)
    }

    fn set_init_source_enabled_with(
        &self,
        entry: &StartupEntry,
        enabled: bool,
        detector: &mut impl FnMut() -> Result<InitSystem, FailureKind>,
        runner: &mut impl FnMut(&str, &[&str], Duration) -> InventoryCommandResult,
    ) -> Result<(), ProviderFailure> {
        let (expected_init, program, args) = match entry.source {
            StartupSource::UserService => {
                if !valid_systemd_user_unit(entry.locator.as_str()) {
                    return Err(ProviderFailure::Rejected);
                }
                let action = if enabled { "enable" } else { "disable" };
                (
                    InitSystem::Systemd,
                    "systemctl",
                    vec!["--user", action, "--", entry.locator.as_str()],
                )
            }
            StartupSource::RunLevel => {
                if !valid_openrc_service(entry.locator.as_str()) {
                    return Err(ProviderFailure::Rejected);
                }
                let action = if enabled { "add" } else { "del" };
                (
                    InitSystem::Openrc,
                    "rc-update",
                    vec![action, entry.locator.as_str(), "default"],
                )
            }
            StartupSource::DesktopEntry => {
                return Err(ProviderFailure::Rejected);
            }
            StartupSource::SystemService
            | StartupSource::RegistryEntry
            | StartupSource::ScheduledTask
            | StartupSource::LoginItem
            | StartupSource::StartupFolder
            | StartupSource::Other => {
                return Err(ProviderFailure::Unsupported);
            }
        };
        verify_selected_init(detector(), expected_init)?;
        revalidate_init_target(entry, expected_init, runner)?;
        // Close the runtime-switch window between target preflight and mutation.
        // A stale row from the old init backend is an identity change, never
        // authorization to reinterpret its locator under the new backend.
        verify_selected_init(detector(), expected_init)?;
        match runner(program, &args, STARTUP_CONTROL_TIMEOUT) {
            InventoryCommandResult::Success(_) => Ok(()),
            InventoryCommandResult::Failure(failure) => Err(provider_failure(failure)),
        }
    }
}

fn verify_selected_init(
    detection: Result<InitSystem, FailureKind>,
    expected: InitSystem,
) -> Result<(), ProviderFailure> {
    match detection {
        Ok(actual) if actual == expected => Ok(()),
        Ok(InitSystem::Unsupported) => Err(ProviderFailure::Unsupported),
        Ok(_) => Err(ProviderFailure::IdentityChanged),
        Err(failure) => Err(provider_failure(failure)),
    }
}

fn revalidate_init_target(
    entry: &StartupEntry,
    init: InitSystem,
    runner: &mut impl FnMut(&str, &[&str], Duration) -> InventoryCommandResult,
) -> Result<(), ProviderFailure> {
    let (program, args) = match init {
        InitSystem::Systemd => (
            "systemctl",
            vec![
                "--user",
                "list-unit-files",
                "--type=service",
                "--no-legend",
                "--no-pager",
                entry.locator.as_str(),
            ],
        ),
        InitSystem::Openrc => ("rc-update", vec!["-v", "show"]),
        InitSystem::Unsupported => return Err(ProviderFailure::Unsupported),
    };
    let output = match runner(program, &args, STARTUP_CONTROL_TIMEOUT) {
        InventoryCommandResult::Success(output) => output,
        InventoryCommandResult::Failure(failure) => return Err(provider_failure(failure)),
    };
    let current_enabled = match init {
        InitSystem::Systemd => systemd_unit_enabled(&output, entry.locator.as_str())
            .ok_or(ProviderFailure::IdentityChanged)?,
        InitSystem::Openrc => parse_openrc_update(&output)
            .iter()
            .find(|service| service.name == entry.locator.as_str())
            .map(|service| {
                service
                    .description
                    .split_whitespace()
                    .any(|runlevel| runlevel == "default")
            })
            .ok_or(ProviderFailure::IdentityChanged)?,
        InitSystem::Unsupported => return Err(ProviderFailure::Unsupported),
    };
    if current_enabled != entry.enabled {
        return Err(ProviderFailure::IdentityChanged);
    }
    Ok(())
}

fn systemd_unit_enabled(output: &str, expected_unit: &str) -> Option<bool> {
    output.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let unit = fields.next()?;
        let state = fields.next()?;
        (unit == expected_unit).then_some(matches!(state, "enabled" | "enabled-runtime"))
    })
}

fn valid_systemd_user_unit(locator: &str) -> bool {
    valid_systemd_service_name(locator)
}

fn valid_openrc_service(locator: &str) -> bool {
    valid_openrc_service_name(locator)
}

fn scan_systemd_startup(
    runner: &mut impl FnMut(&str, &[&str], Duration) -> InventoryCommandResult,
) -> (Vec<StartupEntry>, SourceStatus) {
    let args = [
        "--user",
        "list-unit-files",
        "--type=service",
        "--no-legend",
        "--no-pager",
    ];
    match runner("systemctl", &args, STARTUP_CONTROL_TIMEOUT) {
        InventoryCommandResult::Success(stdout) => {
            let (entries, malformed) = parse_systemd_startup(&stdout);
            let failure = malformed.then_some(FailureKind::ProviderFault);
            let status = source_status(SYSTEMD_PROVIDER_ID, entries.len(), failure);
            (entries, status)
        }
        InventoryCommandResult::Failure(failure) => (
            Vec::new(),
            source_status(SYSTEMD_PROVIDER_ID, 0, Some(failure)),
        ),
    }
}

fn parse_systemd_startup(output: &str) -> (Vec<StartupEntry>, bool) {
    let mut malformed = false;
    let entries = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let parts = line.split_whitespace().collect::<Vec<_>>();
            let name = parts.first().copied()?;
            if parts.len() < 2 || !valid_systemd_user_unit(name) {
                malformed = true;
                return None;
            }
            let state = parts[1];
            let (enabled, control_policy) = match state {
                "enabled" | "enabled-runtime" => (true, StartupControlPolicy::Direct),
                "disabled" => (false, StartupControlPolicy::Direct),
                _ => (false, StartupControlPolicy::Unsupported),
            };
            Some(StartupEntry {
                id: native_startup_id(StartupSource::UserService, name),
                name: name.to_string(),
                exec: name.to_string(),
                enabled,
                source: StartupSource::UserService,
                scope: StartupScope::User,
                control_policy,
                locator: name.to_string().into(),
                impact: StartupImpact::None,
                impact_evidence: StartupImpactEvidence::Unknown {
                    reason: StartupImpactUnknownReason::NoRecordForThisBoot,
                },
            })
        })
        .collect();
    (entries, malformed)
}

fn scan_systemd_blame(
    runner: &mut impl FnMut(&str, &[&str], Duration) -> InventoryCommandResult,
) -> BlameSnapshot {
    let args = ["--user", "blame", "--no-pager"];
    match runner("systemd-analyze", &args, STARTUP_CONTROL_TIMEOUT) {
        InventoryCommandResult::Success(stdout) => {
            let durations = parse_systemd_blame(&stdout);
            let meaningful_lines = stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count();
            if meaningful_lines > durations.len() {
                if durations.is_empty() {
                    BlameSnapshot::Failed(FailureKind::ProviderFault)
                } else {
                    BlameSnapshot::Partial(durations, FailureKind::ProviderFault)
                }
            } else {
                BlameSnapshot::Ready(durations)
            }
        }
        InventoryCommandResult::Failure(failure) => BlameSnapshot::Failed(failure),
    }
}

fn scan_openrc_startup(
    runner: &mut impl FnMut(&str, &[&str], Duration) -> InventoryCommandResult,
) -> (Vec<StartupEntry>, SourceStatus) {
    match runner("rc-update", &["-v", "show"], STARTUP_CONTROL_TIMEOUT) {
        InventoryCommandResult::Success(stdout) => {
            let malformed = stdout.lines().map(str::trim).any(|line| {
                if line.is_empty() {
                    return false;
                }
                let Some((name, runlevels)) = line.split_once('|') else {
                    return true;
                };
                let _ = runlevels;
                !valid_openrc_service(name.trim())
            });
            let entries = parse_openrc_update(&stdout)
                .into_iter()
                .map(|service| {
                    let enabled = service
                        .description
                        .split_whitespace()
                        .any(|runlevel| runlevel == "default");
                    StartupEntry {
                        id: native_startup_id(StartupSource::RunLevel, &service.name),
                        exec: format!("rc-service {} start", service.name),
                        locator: service.name.clone().into(),
                        name: service.name,
                        enabled,
                        source: StartupSource::RunLevel,
                        scope: StartupScope::System,
                        control_policy: StartupControlPolicy::Direct,
                        impact: StartupImpact::None,
                        impact_evidence: StartupImpactEvidence::Unknown {
                            reason: StartupImpactUnknownReason::NotInstrumented,
                        },
                    }
                })
                .collect::<Vec<_>>();
            let failure = malformed.then_some(FailureKind::ProviderFault);
            let status = source_status(OPENRC_PROVIDER_ID, entries.len(), failure);
            (entries, status)
        }
        InventoryCommandResult::Failure(failure) => (
            Vec::new(),
            source_status(OPENRC_PROVIDER_ID, 0, Some(failure)),
        ),
    }
}

fn provider_failure(failure: FailureKind) -> ProviderFailure {
    match failure {
        FailureKind::Unsupported => ProviderFailure::Unsupported,
        FailureKind::RequiresEscalation => ProviderFailure::RequiresEscalation,
        FailureKind::PermissionDenied => ProviderFailure::PermissionDenied,
        FailureKind::MissingDependency => ProviderFailure::MissingDependency,
        FailureKind::TimedOut => ProviderFailure::TimedOut,
        FailureKind::IdentityChanged => ProviderFailure::IdentityChanged,
        FailureKind::TemporarilyUnavailable => ProviderFailure::TemporarilyUnavailable,
        FailureKind::Rejected => ProviderFailure::Rejected,
        FailureKind::ProviderFault => ProviderFailure::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_startup_init_source_tests.rs"]
mod tests;

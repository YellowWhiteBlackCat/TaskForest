//! macOS service-domain providers built on bounded `std::process` shell-outs
//! to `launchctl` and `log` — the same shell-out pattern the Linux adapter
//! uses for systemctl/journalctl (ADR-019).
//!
//! `launchctl list` inventories the user's launchd domain (the system domain
//! needs privileges and is honestly omitted, recorded in ADR-019); control
//! uses `launchctl kickstart / kill / enable / disable`; log snapshots use
//! `log show` against the unified logging store. Dependency graphs and live
//! log streaming have no safe equivalent yet and stay unsupported.

use std::time::Duration;

use taskmanager_application::{
    ServiceControlRequest, ServiceDependenciesRequest, ServiceInventoryRequest,
    ServiceLogSnapshotRequest, ServiceLogStreamRequest,
};
use taskmanager_core::core::services::{ServiceAction, ServiceLogQuery};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    ProviderId, ServiceDeps, ServiceId, ServiceItem, ServiceLogLevel, ServiceLogState,
    ServiceLogStreamState, ServiceStatus,
};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    ServiceControlProvider, ServiceDependenciesProvider, ServiceInventoryProvider,
    ServiceLogSnapshotProvider, ServiceLogStreamProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, ServiceExecutors, ServiceProviderBindings,
};

use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const SERVICE_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.service.inventory.launchctl");

/// Service target identity carried by `ServiceId`: `macos:gui:<uid>:<label>`
/// for the user domain, `macos:system:<label>` for the system domain.
fn parse_service_target(id: &ServiceId) -> (String, String) {
    let id_str = id.as_str();
    if let Some(rest) = id_str.strip_prefix("macos:gui:")
        && let Some((uid, label)) = rest.split_once(':')
    {
        return (format!("gui/{uid}"), label.to_string());
    }
    if let Some(label) = id_str.strip_prefix("macos:system:") {
        return (String::from("system"), label.to_string());
    }
    (String::new(), id_str.to_string())
}

fn run_launchctl(args: &[&str]) -> Result<String, ProviderFailure> {
    let mut command = std::process::Command::new("launchctl");
    command.args(args);
    match run_with_timeout(&mut command, Duration::from_secs(2)) {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) if String::from_utf8_lossy(&output.stderr).contains("permission") => {
            Err(ProviderFailure::PermissionDenied)
        }
        Ok(_) => Err(ProviderFailure::Rejected),
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(ProviderFailure::MissingDependency)
        }
        Err(_) => Err(ProviderFailure::TemporarilyUnavailable),
    }
}

fn uid_domain() -> String {
    std::env::var("UID").unwrap_or_else(|_| String::from("501"))
}

/// Inventory from `launchctl list` (user domain): the PID column says
/// "running", a nonzero last-exit status marks a failed service. The system
/// daemon domain requires privileges and is honestly omitted (ADR-019).
pub struct MacServiceInventoryProvider;

impl ServiceInventoryProvider for MacServiceInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<ServiceItem>, ProviderFailure> {
        let output = run_launchctl(&["list"])?;
        let uid = uid_domain();
        let mut items = Vec::new();
        for line in output.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 3 {
                continue;
            }
            let pid = fields[0];
            let last_exit = fields[1];
            let label = fields[2].to_string();
            let running = pid != "-";
            let failed = !running && last_exit != "-" && last_exit != "0";
            let status = if failed {
                ServiceStatus::Failed
            } else if running {
                ServiceStatus::Active
            } else {
                ServiceStatus::Inactive
            };
            items.push(ServiceItem::from_inventory(
                ServiceId::new(format!("macos:gui:{uid}:{label}")),
                label,
                status,
                "",
                "",
                if running { "running" } else { "not running" },
                if failed { "failed" } else { "" },
            ));
        }
        if items.is_empty() {
            return Err(ProviderFailure::TemporarilyUnavailable);
        }
        let item_count = items.len();
        Ok(PartialSourceSnapshot::new(
            items,
            vec![SourceStatus {
                provider: SERVICE_INVENTORY_PROVIDER,
                outcome: SourceOutcome::Available,
                item_count,
            }],
        ))
    }
}

/// Service control through `launchctl`:
/// start/restart = `kickstart -k`, stop = `kill SIGTERM`,
/// enable = `enable`, disable = `disable`.
pub struct MacServiceControlProvider;

impl ServiceControlProvider for MacServiceControlProvider {
    fn control(
        &mut self,
        service_id: &ServiceId,
        action: ServiceAction,
    ) -> Result<(), ProviderFailure> {
        let (domain, label) = parse_service_target(service_id);
        if domain.is_empty() {
            return Err(ProviderFailure::IdentityChanged);
        }
        let target = format!("{domain}/{label}");
        match action {
            ServiceAction::Start | ServiceAction::Restart => {
                run_launchctl(&["kickstart", "-k", &target]).map(|_| ())
            }
            ServiceAction::Stop => run_launchctl(&["kill", "SIGTERM", &target]).map(|_| ()),
            ServiceAction::Enable => run_launchctl(&["enable", &target]).map(|_| ()),
            ServiceAction::Disable => run_launchctl(&["disable", &target]).map(|_| ()),
        }
    }
}

/// Log snapshot from the unified logging store via `log show` (bounded):
/// last-30-minute entries for the service label.
pub struct MacServiceLogSnapshotProvider;

impl ServiceLogSnapshotProvider for MacServiceLogSnapshotProvider {
    fn snapshot(&mut self, service_id: &ServiceId) -> Result<ServiceLogState, ProviderFailure> {
        let (_, label) = parse_service_target(service_id);
        let predicate = format!("process == \"{label}\"");
        let mut command = std::process::Command::new("log");
        command.args([
            "show",
            "--last",
            "30m",
            "--style",
            "syslog",
            "--predicate",
            &predicate,
        ]);
        let output = match run_with_timeout(&mut command, Duration::from_secs(5)) {
            Ok(output) if output.status.success() => output,
            Ok(_) => return Err(ProviderFailure::Rejected),
            Err(BoundedCommandError::Spawn(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                return Err(ProviderFailure::MissingDependency);
            }
            Err(_) => return Err(ProviderFailure::TemporarilyUnavailable),
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let mut lines = Vec::new();
        for line in text.lines() {
            if log_line_level(line) != ServiceLogLevel::Unknown {
                lines.push(line.trim_end().to_string());
            }
        }
        Ok(ServiceLogState::from_lines(lines))
    }
}

/// Extract the level token from one unified-logging syslog-style line
/// (`2026-08-02 10:00:00.123456+0800  label[123:456] info: message`).
fn log_line_level(line: &str) -> ServiceLogLevel {
    // `log show --style syslog` lines: `YYYY-MM-DD HH:MM:SS.mmmuuu+ZZZZ  label[pid:tid] level: message`
    // (two spaces after the timestamp; whitespace-split keeps tokens stable).
    let mut parts = line.split_whitespace();
    let _timestamp = parts.next();
    let _time_part = parts.next();
    let _label = parts.next();
    let Some(level_part) = parts.next() else {
        return ServiceLogLevel::Unknown;
    };
    if level_part.contains("error") || level_part.contains("fault") {
        ServiceLogLevel::Error
    } else if level_part.contains("warning") {
        ServiceLogLevel::Warning
    } else if level_part.contains("info") {
        ServiceLogLevel::Info
    } else if level_part.contains("debug") {
        ServiceLogLevel::Debug
    } else {
        ServiceLogLevel::Unknown
    }
}

/// launchd has no dependency graph; typed unsupported.
pub struct PendingServiceDependenciesProvider;

impl ServiceDependenciesProvider for PendingServiceDependenciesProvider {
    fn dependencies(&mut self, _service_id: &ServiceId) -> Result<ServiceDeps, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// `log stream` (live tail) is not safely bounded; typed unsupported.
pub struct PendingServiceLogStreamProvider;

impl ServiceLogStreamProvider for PendingServiceLogStreamProvider {
    fn stream(
        &mut self,
        _query: &ServiceLogQuery,
        _observed_at_ms: u64,
    ) -> Result<ServiceLogStreamState, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

pub struct MacServiceProviders {
    inventory: ProviderRegistration<ServiceInventoryRequest, Box<dyn ServiceInventoryProvider>>,
    dependencies:
        ProviderRegistration<ServiceDependenciesRequest, Box<dyn ServiceDependenciesProvider>>,
    control: ProviderRegistration<ServiceControlRequest, Box<dyn ServiceControlProvider>>,
    log_snapshot:
        ProviderRegistration<ServiceLogSnapshotRequest, Box<dyn ServiceLogSnapshotProvider>>,
    log_stream: ProviderRegistration<ServiceLogStreamRequest, Box<dyn ServiceLogStreamProvider>>,
}

impl MacServiceProviders {
    #[must_use]
    pub fn new<I, D, C, S, L>(
        inventory: ProviderRegistration<ServiceInventoryRequest, I>,
        dependencies: ProviderRegistration<ServiceDependenciesRequest, D>,
        control: ProviderRegistration<ServiceControlRequest, C>,
        log_snapshot: ProviderRegistration<ServiceLogSnapshotRequest, S>,
        log_stream: ProviderRegistration<ServiceLogStreamRequest, L>,
    ) -> Self
    where
        I: ServiceInventoryProvider,
        D: ServiceDependenciesProvider,
        C: ServiceControlProvider,
        S: ServiceLogSnapshotProvider,
        L: ServiceLogStreamProvider,
    {
        Self {
            inventory: inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn ServiceInventoryProvider>),
            dependencies: dependencies.map_provider(|provider| {
                Box::new(provider) as Box<dyn ServiceDependenciesProvider>
            }),
            control: control
                .map_provider(|provider| Box::new(provider) as Box<dyn ServiceControlProvider>),
            log_snapshot: log_snapshot
                .map_provider(|provider| Box::new(provider) as Box<dyn ServiceLogSnapshotProvider>),
            log_stream: log_stream
                .map_provider(|provider| Box::new(provider) as Box<dyn ServiceLogStreamProvider>),
        }
    }

    pub(crate) fn runtime_bindings(&self) -> ServiceProviderBindings {
        ServiceProviderBindings::from_registrations(
            &self.inventory,
            &self.dependencies,
            &self.control,
            &self.log_snapshot,
            &self.log_stream,
        )
    }

    pub(crate) fn into_runtime(self) -> ServiceExecutors {
        let Self {
            inventory,
            dependencies,
            control,
            log_snapshot,
            log_stream,
        } = self;
        let mut inventory = inventory.into_provider();
        let mut dependencies = dependencies.into_provider();
        let mut control = control.into_provider();
        let mut log_snapshot = log_snapshot.into_provider();
        let mut log_stream = log_stream.into_provider();
        ServiceExecutors::new(
            move || inventory.refresh(),
            move |service_id| dependencies.dependencies(&service_id),
            move |service_id, action| control.control(&service_id, action),
            move |service_id| log_snapshot.snapshot(&service_id),
            move |query, observed_at_ms| log_stream.stream(&query, observed_at_ms),
        )
    }
}

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_service.rs"]
mod tests;

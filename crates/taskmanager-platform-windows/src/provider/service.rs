//! Windows service-domain providers built on the mature safe
//! `windows-service` and `windows-registry` crates plus the audited winevt
//! boundary.
//!
//! The service-control-manager (SCM) database is the authoritative source for
//! inventory, dependency names, status, and lifecycle control. Service-log
//! snapshots and incremental streams read the `System` channel through
//! `EvtQuery`/`EvtNext`/`EvtRender`, attributing events to a service by the
//! provider name equal to its native SCM service name; a service that emits
//! no such events is an honest empty snapshot, never a borrowed or fabricated
//! one. No command interpreter is part of this provider.

use std::time::{Duration, Instant};

use taskmanager_application::{
    ServiceAction, ServiceControlRequest, ServiceDependenciesRequest, ServiceInventoryRequest,
    ServiceLogSnapshotRequest, ServiceLogStreamRequest,
};
#[cfg(windows)]
use taskmanager_core::ServiceStatus;
use taskmanager_core::{FailureKind, ProviderId, ServiceDeps, ServiceId, ServiceItem};
#[cfg(any(windows, test))]
use taskmanager_core::{ServiceLogEntry, ServiceLogLevel};
#[cfg(windows)]
use taskmanager_core::{ServiceRelationEdge, ServiceRelationGraph, ServiceRelationKind};
use taskmanager_platform_contract::{
    PartialSourceSnapshot, ProviderFailure, SourceOutcome, SourceStatus,
};
use taskmanager_platform_provider::{
    ServiceControlProvider, ServiceDependenciesProvider, ServiceInventoryProvider,
    ServiceLogSnapshotProvider, ServiceLogStreamProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, ServiceExecutors, ServiceProviderBindings,
};

mod log_runtime;
#[cfg(windows)]
use taskmanager_windows_api::{ServiceStartMode, WindowsApiError};

#[cfg(windows)]
use windows_service::service::{ServiceAccess, ServiceStartType, ServiceState};
#[cfg(windows)]
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

const SERVICE_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.service.inventory.scm");
const SERVICE_CACHE_TTL: Duration = Duration::from_secs(5);
/// Windows Event Log channel that standard users can read and where service
/// lifetime events are recorded.
#[cfg(windows)]
const SERVICE_LOG_CHANNEL: &str = "System";
/// Newest entries shown in one snapshot — mirrors the Linux journalctl lane.
#[cfg(windows)]
const SERVICE_LOG_SNAPSHOT_LIMIT: usize = 50;
/// Largest follow batch — mirrors the Linux stream lane's bounded increment.
#[cfg(windows)]
#[allow(dead_code)]
const SERVICE_LOG_STREAM_LIMIT: usize = 200;
/// Display bound for one formatted log line.
#[cfg(any(windows, test))]
const SERVICE_LOG_LINE_MAX_CHARS: usize = 8_192;
#[cfg(windows)]
const MAX_SCANNED_SERVICES: usize = 4_096;
#[cfg(windows)]
const MAX_SERVICE_ID_CHARS: usize = 256;
#[cfg(windows)]
const MAX_SERVICE_DEPENDENCIES: usize = 512;
#[cfg(windows)]
const MAX_DEPENDENCY_TEXT_BYTES: usize = 8 * 1024;

#[derive(Default)]
struct ScmInventory {
    items: Vec<ServiceItem>,
    skipped: usize,
    truncated: bool,
}

/// Windows SCM inventory, refreshed at most every five seconds. A failed
/// individual service query is skipped so one protected or disappearing
/// service does not blank the complete inventory.
pub struct WinServiceInventoryProvider {
    cache: ScmInventory,
    cache_at: Option<Instant>,
}

impl WinServiceInventoryProvider {
    pub fn new() -> Self {
        Self {
            cache: ScmInventory::default(),
            cache_at: None,
        }
    }

    fn fresh(&mut self, now: Instant) -> Option<&ScmInventory> {
        let stale = self
            .cache_at
            .is_none_or(|at| now.duration_since(at) >= SERVICE_CACHE_TTL);
        if stale {
            self.cache = scm_services()?;
            self.cache_at = Some(now);
        }
        Some(&self.cache)
    }
}

impl Default for WinServiceInventoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceInventoryProvider for WinServiceInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<ServiceItem>, ProviderFailure> {
        let cached = self
            .fresh(Instant::now())
            .ok_or(ProviderFailure::TemporarilyUnavailable)?;
        let items = cached.items.clone();
        let outcome = scm_inventory_outcome(items.len(), cached.skipped, cached.truncated);
        Ok(PartialSourceSnapshot::new(
            items,
            vec![SourceStatus {
                provider: SERVICE_INVENTORY_PROVIDER,
                outcome,
                item_count: cached.items.len(),
            }],
        ))
    }
}

#[cfg(windows)]
fn scm_services() -> Option<ScmInventory> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::ENUMERATE_SERVICE,
    )
    .ok()?;
    let services_key = windows_registry::LOCAL_MACHINE
        .open("SYSTEM\\CurrentControlSet\\Services")
        .ok()?;
    let names = services_key.keys().ok()?;
    let mut items = Vec::new();
    let mut skipped = 0usize;
    let mut truncated = false;
    for (index, name) in names.enumerate() {
        if index >= MAX_SCANNED_SERVICES {
            truncated = true;
            break;
        }
        if let Ok(service_type) = services_key.open(&name).and_then(|k| k.get_u32("Type")) {
            // Ignore kernel/filesystem/recognizer drivers (Type 1, 2, 8)
            if (service_type & 0x30) == 0
                && (service_type & 0x100) == 0
                && service_type != 0x10
                && service_type != 0x20
            {
                continue;
            }
        }
        let Ok(service) = manager.open_service(
            &name,
            ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG,
        ) else {
            skipped += 1;
            continue;
        };
        let Ok(config) = service.query_config() else {
            skipped += 1;
            continue;
        };
        let state = service
            .query_status()
            .map(|status| status.current_state)
            .ok();
        let state_label = state.map(service_state_label).unwrap_or("Unknown");
        let dependency_targets = config
            .dependencies
            .iter()
            .map(|dependency| {
                ServiceId::new(
                    dependency
                        .to_system_identifier()
                        .to_string_lossy()
                        .into_owned(),
                )
            })
            .collect::<Vec<_>>();
        let dependency_text_bytes = dependency_targets
            .iter()
            .map(|target| target.as_str().len())
            .sum::<usize>()
            .saturating_add(dependency_targets.len().saturating_sub(1));
        if config.dependencies.len() > MAX_SERVICE_DEPENDENCIES
            || dependency_text_bytes > MAX_DEPENDENCY_TEXT_BYTES
            || config.display_name.len() > MAX_DEPENDENCY_TEXT_BYTES
        {
            skipped += 1;
            continue;
        }
        let relations = ServiceRelationGraph::from_edges(
            dependency_targets
                .into_iter()
                .map(|target| ServiceRelationEdge::new(ServiceRelationKind::Requires, target)),
        );
        items.push(
            ServiceItem::from_inventory(
                ServiceId::new(name.clone()),
                name,
                ServiceStatus::from(state_label),
                config.display_name.to_string_lossy().into_owned(),
                service_start_type_label(config.start_type),
                state_label,
                state_label,
            )
            .with_relations(relations),
        );
    }
    Some(ScmInventory {
        items,
        skipped,
        truncated,
    })
}

#[cfg(not(windows))]
fn scm_services() -> Option<ScmInventory> {
    None
}

fn scm_inventory_outcome(item_count: usize, skipped: usize, truncated: bool) -> SourceOutcome {
    if truncated || (item_count == 0 && skipped > 0) {
        SourceOutcome::Partial(FailureKind::ProviderFault)
    } else if item_count == 0 {
        SourceOutcome::Empty
    } else {
        SourceOutcome::Available
    }
}

#[cfg(windows)]
fn service_state_label(state: ServiceState) -> &'static str {
    match state {
        ServiceState::Stopped => "Stopped",
        ServiceState::StartPending => "StartPending",
        ServiceState::StopPending => "StopPending",
        ServiceState::Running => "Running",
        ServiceState::ContinuePending => "ContinuePending",
        ServiceState::PausePending => "PausePending",
        ServiceState::Paused => "Paused",
    }
}

#[cfg(windows)]
fn service_start_type_label(start_type: ServiceStartType) -> &'static str {
    match start_type {
        ServiceStartType::AutoStart => "Auto",
        ServiceStartType::OnDemand => "Manual",
        ServiceStartType::Disabled => "Disabled",
        ServiceStartType::SystemStart => "System",
        ServiceStartType::BootStart => "Boot",
    }
}

/// SCM dependency names are native service relationships. Only `requires` is
/// populated because the systemd-shaped `wants`/`after` projections have no
/// equivalent meaning on Windows. Details are queried for the requested
/// service only; they must not repeat the full inventory scan.
pub struct WinServiceDependenciesProvider;

impl WinServiceDependenciesProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinServiceDependenciesProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceDependenciesProvider for WinServiceDependenciesProvider {
    fn dependencies(&mut self, service_id: &ServiceId) -> Result<ServiceDeps, ProviderFailure> {
        #[cfg(windows)]
        {
            scm_service_dependencies(service_id)
        }
        #[cfg(not(windows))]
        {
            let _ = service_id;
            Err(ProviderFailure::MissingDependency)
        }
    }
}

#[cfg(windows)]
fn valid_service_id(service_id: &ServiceId) -> Result<&str, ProviderFailure> {
    let name = service_id.as_str();
    if name.is_empty()
        || name.len() > MAX_SERVICE_ID_CHARS
        || name.contains('\0')
        || name.trim().is_empty()
    {
        return Err(ProviderFailure::IdentityChanged);
    }
    Ok(name)
}

#[cfg(windows)]
fn scm_service_dependencies(service_id: &ServiceId) -> Result<ServiceDeps, ProviderFailure> {
    let name = valid_service_id(service_id)?;
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
    let service = manager
        .open_service(name, ServiceAccess::QUERY_CONFIG)
        .map_err(|_| ProviderFailure::IdentityChanged)?;
    let config = service
        .query_config()
        .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
    if config.dependencies.len() > MAX_SERVICE_DEPENDENCIES {
        return Err(ProviderFailure::ProviderFault);
    }
    let requires = config
        .dependencies
        .iter()
        .map(|dependency| {
            dependency
                .to_system_identifier()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>()
        .join(" ");
    if requires.len() > MAX_DEPENDENCY_TEXT_BYTES {
        return Err(ProviderFailure::ProviderFault);
    }
    let mut dependencies = ServiceDeps::default();
    dependencies.replace_relation_targets(
        ServiceRelationKind::Requires,
        requires.split_whitespace().map(ServiceId::new),
    );
    Ok(dependencies)
}

/// Native SCM lifecycle control through the mature `windows-service` crate.
/// The crate owns all handles and FFI details; this adapter exposes only typed
/// provider failures.
pub struct WinServiceControlProvider;

impl WinServiceControlProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinServiceControlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceControlProvider for WinServiceControlProvider {
    fn control(
        &mut self,
        service_id: &ServiceId,
        action: ServiceAction,
    ) -> Result<(), ProviderFailure> {
        #[cfg(windows)]
        {
            control_scm(service_id, action)
        }
        #[cfg(not(windows))]
        {
            let _ = (service_id, action);
            Err(ProviderFailure::MissingDependency)
        }
    }
}

#[cfg(windows)]
fn control_scm(service_id: &ServiceId, action: ServiceAction) -> Result<(), ProviderFailure> {
    if let Some(mode) = match action {
        ServiceAction::Enable => Some(ServiceStartMode::Automatic),
        ServiceAction::Disable => Some(ServiceStartMode::Disabled),
        _ => None,
    } {
        return taskmanager_windows_api::set_service_start_mode(service_id.as_str(), mode)
            .map_err(map_service_start_mode_error);
    }

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
    let access = match action {
        ServiceAction::Start => ServiceAccess::START,
        ServiceAction::Stop => ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
        ServiceAction::Restart => {
            ServiceAccess::START | ServiceAccess::STOP | ServiceAccess::QUERY_STATUS
        }
        ServiceAction::Enable | ServiceAction::Disable => ServiceAccess::QUERY_STATUS,
    };
    let service = manager
        .open_service(service_id.as_str(), access)
        .map_err(|_| ProviderFailure::IdentityChanged)?;
    match action {
        ServiceAction::Start => service
            .start::<&str>(&[])
            .map_err(|_| ProviderFailure::ProviderFault),
        ServiceAction::Stop => service
            .stop()
            .map(|_| ())
            .map_err(|_| ProviderFailure::ProviderFault),
        ServiceAction::Restart => {
            let status = service
                .query_status()
                .map_err(|_| ProviderFailure::ProviderFault)?;
            if status.current_state != ServiceState::Stopped {
                service.stop().map_err(|_| ProviderFailure::ProviderFault)?;
                wait_for_service_state(&service, ServiceState::Stopped)?;
            }
            service
                .start::<&str>(&[])
                .map_err(|_| ProviderFailure::ProviderFault)
        }
        ServiceAction::Enable | ServiceAction::Disable => Err(ProviderFailure::Unsupported),
    }
}

#[cfg(windows)]
fn map_service_start_mode_error(error: WindowsApiError) -> ProviderFailure {
    match error {
        WindowsApiError::InvalidInput | WindowsApiError::IdentityChanged => {
            ProviderFailure::IdentityChanged
        }
        WindowsApiError::Unsupported => ProviderFailure::MissingDependency,
        WindowsApiError::PermissionDenied => ProviderFailure::PermissionDenied,
        WindowsApiError::QueryFailed
        | WindowsApiError::InvalidText
        | WindowsApiError::ResourceLimit => ProviderFailure::ProviderFault,
    }
}

#[cfg(windows)]
fn wait_for_service_state(
    service: &windows_service::service::Service,
    expected: ServiceState,
) -> Result<(), ProviderFailure> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .unwrap_or_else(Instant::now);
    loop {
        let status: windows_service::service::ServiceStatus = service
            .query_status()
            .map_err(|_| ProviderFailure::ProviderFault)?;
        if status.current_state == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ProviderFailure::TimedOut);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Windows Event Log integration through the audited winevt boundary. The
/// `System` channel is attributed per service by its native SCM name; an
/// absent provider trail is an honest empty snapshot per the service-log
/// contract, never a borrowed or fabricated one.
pub struct WinServiceLogSnapshotProvider;

impl WinServiceLogSnapshotProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinServiceLogSnapshotProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Incremental service-log stream. The cursor is the `System` channel's
/// monotonically increasing event record id, the Windows counterpart of the
/// journalctl cursor the Linux lane uses; each poll returns only entries with
/// a record id strictly greater than the cursor.
pub struct WinServiceLogStreamProvider;

/// The stream cursor is the decimal event record id of the last delivered
/// entry. Anything else is a stale or foreign cursor and is reported as an
/// identity change instead of replaying or guessing.
#[cfg(any(windows, test))]
#[allow(dead_code)]
fn parse_stream_cursor(after_cursor: Option<&str>) -> Result<Option<u64>, ProviderFailure> {
    after_cursor
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| ProviderFailure::IdentityChanged)
}

/// Remap the raw Windows level onto the syslog-style priority scale the
/// service-log filters understand (critical/error -> err, warning, info,
/// verbose -> debug). `None` stays unknown rather than defaulting to info.
#[cfg(any(windows, test))]
fn windows_level_priority(level: Option<u8>) -> Option<u8> {
    match level {
        Some(1 | 2) => Some(2),
        Some(3) => Some(4),
        Some(4) => Some(6),
        Some(5) => Some(7),
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn priority_log_level(priority: Option<u8>) -> ServiceLogLevel {
    match priority {
        Some(0..=3) => ServiceLogLevel::Error,
        Some(4) => ServiceLogLevel::Warning,
        Some(5..=6) => ServiceLogLevel::Info,
        Some(7) => ServiceLogLevel::Debug,
        _ => ServiceLogLevel::Unknown,
    }
}

/// The display text for one entry: the publisher-formatted message when the
/// boundary could format one, the rendered event data when it could not, and
/// an identification line — never invented content — when neither exists.
#[cfg(any(windows, test))]
fn event_log_message(entry: &taskmanager_windows_api::WindowsEventLogEntry) -> String {
    if !entry.message.is_empty() {
        return entry.message.clone();
    }
    if !entry.properties.is_empty() {
        let data = entry
            .properties
            .iter()
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if !data.is_empty() {
            return data;
        }
    }
    format!(
        "event {} from {}",
        entry.event_id,
        entry.provider.as_deref().unwrap_or("unknown provider")
    )
}

#[cfg(any(windows, test))]
fn truncated_line(line: String) -> String {
    if line.chars().count() <= SERVICE_LOG_LINE_MAX_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(SERVICE_LOG_LINE_MAX_CHARS).collect();
    cut.push_str("…[truncated]");
    cut
}

/// Map bounded native entries to the structured stream contract; the cursor
/// is the event record id string. Pure so the mapping is testable off-Windows.
#[cfg(any(windows, test))]
#[allow(dead_code)]
fn event_log_entries(
    entries: Vec<taskmanager_windows_api::WindowsEventLogEntry>,
) -> Vec<ServiceLogEntry> {
    entries
        .into_iter()
        .map(|entry| {
            let priority = windows_level_priority(entry.level);
            ServiceLogEntry {
                cursor: entry.record_id.to_string(),
                realtime_timestamp_micros: entry.timestamp_ms.map(|ms| ms.saturating_mul(1_000)),
                priority,
                level: priority_log_level(priority),
                message: truncated_line(event_log_message(&entry)),
            }
        })
        .collect()
}

/// Format one bounded snapshot line per entry, chronological order preserved.
#[cfg(any(windows, test))]
fn event_log_lines(entries: &[taskmanager_windows_api::WindowsEventLogEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| {
            let timestamp = entry
                .timestamp_ms
                .map(format_event_log_timestamp)
                .unwrap_or_else(|| "-".to_string());
            let level = match priority_log_level(windows_level_priority(entry.level)) {
                ServiceLogLevel::Error => "error",
                ServiceLogLevel::Warning => "warning",
                ServiceLogLevel::Info => "info",
                ServiceLogLevel::Debug => "debug",
                ServiceLogLevel::Unknown => "unknown",
            };
            truncated_line(format!(
                "{timestamp} [{level}] {}",
                event_log_message(entry)
            ))
        })
        .collect()
}

/// Render epoch milliseconds as a UTC `YYYY-MM-DDTHH:MM:SS.mmmZ` stamp; pure
/// inverse of the boundary's XML timestamp parser.
#[cfg(any(windows, test))]
fn format_event_log_timestamp(timestamp_ms: u64) -> String {
    let millis = timestamp_ms % 1_000;
    let seconds_total = timestamp_ms / 1_000;
    let second = seconds_total % 60;
    let minute = (seconds_total / 60) % 60;
    let hour = (seconds_total / 3_600) % 24;
    let days = (seconds_total / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Proleptic Gregorian date from days since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
#[cfg(any(windows, test))]
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u64;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u64;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

pub struct WinServiceProviders {
    inventory: ProviderRegistration<ServiceInventoryRequest, Box<dyn ServiceInventoryProvider>>,
    dependencies:
        ProviderRegistration<ServiceDependenciesRequest, Box<dyn ServiceDependenciesProvider>>,
    control: ProviderRegistration<ServiceControlRequest, Box<dyn ServiceControlProvider>>,
    log_snapshot:
        ProviderRegistration<ServiceLogSnapshotRequest, Box<dyn ServiceLogSnapshotProvider>>,
    log_stream: ProviderRegistration<ServiceLogStreamRequest, Box<dyn ServiceLogStreamProvider>>,
}

impl WinServiceProviders {
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
#[path = "../../tests/headless/platform_windows_provider_service.rs"]
mod tests;

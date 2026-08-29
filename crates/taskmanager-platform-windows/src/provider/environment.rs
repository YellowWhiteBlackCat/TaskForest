//! Windows environment-domain providers built on safe wrapper crates and the
//! audited native WTS/Known Folder/winevt boundary (ADR-018/031).
//!
//! Startup inventory/control read the Run registry keys through Microsoft's
//! safe `windows-registry` crate, plus the user Startup folder via `std::env`
//! and `std::fs`, plus logon/boot-triggered Task Scheduler tasks through the
//! audited Task Scheduler COM boundary. Run and folder approval both use the
//! 12-byte `Explorer\StartupApproved` blob (`Run` and `StartupFolder` keys),
//! and control flips only that blob's status byte — Run values, folder files,
//! and the task store are never deleted or reconstructed. Boot evidence reads
//! the boot event (ID 100) from the
//! `Microsoft-Windows-Diagnostics-Performance/Operational` channel through
//! the winevt boundary. Windows has no systemd-style failed-units or
//! critical-chain concept, so the snapshot's failed-units list is honestly
//! empty and the critical chain carries the single documented boot node with
//! the measured boot duration. Sessions use WTS enumeration/logoff through
//! the boundary; `Lock` maps to `LockWorkStation`, which locks the calling
//! interactive session — the correct semantics for this desktop application.

#[cfg(windows)]
use std::path::PathBuf;

use taskmanager_application::{
    SessionControlRequest, SessionInventoryRequest, StartupControlRequest, StartupEvidenceRequest,
    StartupInventoryRequest,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{FailureKind, ProviderId, StartupEntry};
#[cfg(windows)]
use taskmanager_core::{
    StartupControlPolicy, StartupEntryId, StartupEntryLocator, StartupImpact,
    StartupImpactEvidence, StartupScope, StartupSource,
};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    SessionControlProvider, SessionInventoryProvider, StartupControlProvider,
    StartupEvidenceProvider, StartupInventoryProvider,
};
use taskmanager_platform_runtime::{
    EnvironmentExecutors, EnvironmentProviderBindings, ProviderRegistration,
};

mod boot_evidence;
mod sessions;

const STARTUP_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.startup.inventory.registry");
const SESSION_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.session.inventory.wts");

#[cfg(windows)]
const MAX_STARTUP_REGISTRY_ENTRIES_PER_KEY: usize = 1_024;
#[cfg(windows)]
const MAX_STARTUP_FOLDER_ENTRIES: usize = 1_024;
#[cfg(windows)]
const MAX_STARTUP_ENTRIES_TOTAL: usize = 4_096;
#[cfg(windows)]
const MAX_STARTUP_TEXT_BYTES: usize = 64 * 1024;
/// Documented channel carrying the Windows boot diagnostics events.
#[cfg(windows)]
const BOOT_EVIDENCE_CHANNEL: &str = "Microsoft-Windows-Diagnostics-Performance/Operational";
/// The boot event: "Windows has started" with the measured boot durations.
#[cfg(windows)]
const BOOT_EVIDENCE_BOOT_EVENT_ID: u32 = 100;

/// Registry Run-key roots scanned for startup entries. Order matters for
/// locator stability: HKCU first (user scope), then HKLM (system scope).
#[cfg(windows)]
const RUN_KEY_PATHS: &[(&str, &str, StartupScope, bool)] = &[
    // (root, subkey, scope, system)
    (
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        "win:run:hkcu",
        StartupScope::User,
        false,
    ),
    (
        "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
        "win:runonce:hkcu",
        StartupScope::User,
        false,
    ),
    (
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        "win:run:hklm",
        StartupScope::System,
        true,
    ),
    (
        "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
        "win:runonce:hklm",
        StartupScope::System,
        true,
    ),
];

/// Startup entries from the user/system Run registry keys, the Startup
/// folders, and the Task Scheduler's logon/boot-triggered tasks. Run and
/// folder entries are included only when their `StartupApproved` state is
/// absent or one of the bounded, recognized 12-byte states. Malformed
/// approval blobs are counted as partial-source failures rather than being
/// presented as enabled.
pub struct WinStartupInventoryProvider {
    /// Resolved once at construction; the Startup folder only exists on Windows.
    #[cfg(windows)]
    cached_startup_folder: Option<PathBuf>,
    /// Keep a failed Known Folder query visible instead of treating the
    /// registry-only subset as a complete Startup inventory.
    #[cfg(windows)]
    cached_startup_folder_failure: Option<FailureKind>,
}

impl WinStartupInventoryProvider {
    pub fn new() -> Self {
        #[cfg(windows)]
        let (cached_startup_folder, cached_startup_folder_failure) = match user_startup_folder() {
            Ok(path) => (Some(path), None),
            Err(failure) => (None, Some(failure)),
        };
        Self {
            #[cfg(windows)]
            cached_startup_folder,
            #[cfg(windows)]
            cached_startup_folder_failure,
        }
    }
}

#[cfg(windows)]
fn user_startup_folder() -> Result<PathBuf, FailureKind> {
    taskmanager_windows_api::known_folder_path(taskmanager_windows_api::KnownFolder::Startup)
        .map_err(|error| map_windows_api_failure(error).kind())
}

#[cfg(windows)]
fn scan_registry(
    key: &windows_registry::Key,
    subkey: &str,
    id_prefix: &str,
    scope: StartupScope,
    entries: &mut Vec<StartupEntry>,
    mut failures: usize,
) -> usize {
    let Ok(run_key) = key.open(subkey) else {
        return failures;
    };
    let Ok(values) = run_key.values() else {
        return failures;
    };
    for (index, (name, value)) in values.enumerate() {
        if index >= MAX_STARTUP_REGISTRY_ENTRIES_PER_KEY
            || entries.len() >= MAX_STARTUP_ENTRIES_TOTAL
        {
            // The source is bounded, but truncation is not presented as a
            // complete inventory.
            failures += 1;
            break;
        }
        if name.len() > MAX_STARTUP_TEXT_BYTES {
            failures += 1;
            continue;
        }
        let Ok(exec) = String::try_from(value) else {
            failures += 1;
            continue;
        };
        if exec.len() > MAX_STARTUP_TEXT_BYTES {
            failures += 1;
            continue;
        }
        let approval = startup_approval_state(key, id_prefix, &name);
        let (enabled, control_policy) = match approval {
            StartupApproval::Enabled => (true, StartupControlPolicy::Direct),
            StartupApproval::Disabled => (false, StartupControlPolicy::Direct),
            // Without an existing approval value we know Windows' default is
            // enabled, but we do not claim that this row is safely mutable:
            // creating a new undocumented blob is outside this boundary.
            StartupApproval::Missing => (true, StartupControlPolicy::Unsupported),
            StartupApproval::Unknown => {
                failures += 1;
                continue;
            }
        };
        entries.push(StartupEntry {
            id: StartupEntryId::new(format!("{id_prefix}:{name}")),
            name: name.clone(),
            exec,
            enabled,
            source: StartupSource::RegistryEntry,
            scope,
            control_policy,
            locator: StartupEntryLocator::new(format!("{id_prefix}:{name}")),
            impact: StartupImpact::None,
            impact_evidence: StartupImpactEvidence::Unknown {
                reason: taskmanager_core::StartupImpactUnknownReason::Unsupported,
            },
        });
    }
    failures
}

#[cfg(windows)]
fn scan_startup_folder(
    folder: &PathBuf,
    entries: &mut Vec<StartupEntry>,
    mut failures: usize,
) -> usize {
    let Ok(read_dir) = std::fs::read_dir(folder) else {
        return failures.saturating_add(1);
    };
    for (index, entry_result) in read_dir.enumerate() {
        if index >= MAX_STARTUP_FOLDER_ENTRIES || entries.len() >= MAX_STARTUP_ENTRIES_TOTAL {
            failures += 1;
            break;
        }
        let Ok(entry) = entry_result else {
            failures += 1;
            continue;
        };
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if !matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("lnk") | Some("url") | Some("exe") | Some("bat") | Some("cmd")
        ) {
            continue;
        }
        let exec = path.display().to_string();
        if name.len() > MAX_STARTUP_TEXT_BYTES || exec.len() > MAX_STARTUP_TEXT_BYTES {
            failures += 1;
            continue;
        }
        let approval = startup_folder_approval_state(&name);
        let (enabled, control_policy) = match approval {
            StartupApproval::Enabled => (true, StartupControlPolicy::Direct),
            StartupApproval::Disabled => (false, StartupControlPolicy::Direct),
            // No approval value means Windows' default enabled state, and
            // control can create the documented blob keyed by the file name,
            // so the row is honestly mutable without touching the file.
            StartupApproval::Missing => (true, StartupControlPolicy::Direct),
            StartupApproval::Unknown => {
                failures += 1;
                continue;
            }
        };
        entries.push(StartupEntry {
            id: StartupEntryId::new(format!("win:folder:{name}")),
            name: name.clone(),
            exec,
            enabled,
            source: StartupSource::StartupFolder,
            scope: StartupScope::User,
            control_policy,
            locator: StartupEntryLocator::new(format!("win:folder:{name}")),
            impact: StartupImpact::None,
            impact_evidence: StartupImpactEvidence::Unknown {
                reason: taskmanager_core::StartupImpactUnknownReason::Unsupported,
            },
        });
    }
    failures
}

/// Third startup source: Task Scheduler tasks with a logon or boot trigger,
/// enumerated through the audited COM boundary (no `schtasks.exe`). A whole
/// unavailable source degrades to `Some(kind)` so the snapshot never presents
/// the remaining sources as a complete inventory.
#[cfg(windows)]
fn scan_scheduled_tasks(entries: &mut Vec<StartupEntry>) -> Option<FailureKind> {
    let tasks = match taskmanager_windows_api::enumerate_startup_tasks() {
        Ok(tasks) => tasks,
        Err(error) => return Some(map_windows_api_failure(error).kind()),
    };
    for task in tasks {
        if !task.has_logon_or_boot_trigger {
            continue;
        }
        if entries.len() >= MAX_STARTUP_ENTRIES_TOTAL {
            return Some(FailureKind::ProviderFault);
        }
        let entry = match scheduled_task_entry(task) {
            Ok(entry) => entry,
            Err(kind) => return Some(kind),
        };
        entries.push(entry);
    }
    None
}

/// Map one boundary task onto the neutral contract row. `win:task:{path}`
/// mirrors the `win:run:*`/`win:folder:*` id scheme; the locator is the task
/// path itself. Scope stays honestly `Unknown`: User vs System is decided by
/// the definition principal's UserId/logon type, which the pinned boundary
/// surface does not expose yet. Task mutation stays unsupported until a
/// control seam for the task store is chartered.
#[cfg(windows)]
fn scheduled_task_entry(
    task: taskmanager_windows_api::WindowsStartupTask,
) -> Result<StartupEntry, FailureKind> {
    let path = task.task_path;
    if path.is_empty() || path.len() > MAX_STARTUP_TEXT_BYTES {
        return Err(FailureKind::ProviderFault);
    }
    let name = match task.name {
        Some(name) if !name.is_empty() && name.len() <= MAX_STARTUP_TEXT_BYTES => name,
        // Fall back to the task's own name portion of its path.
        _ => path.rsplit('\\').next().unwrap_or_default().to_string(),
    };
    if name.is_empty() {
        return Err(FailureKind::ProviderFault);
    }
    Ok(StartupEntry {
        id: StartupEntryId::new(format!("win:task:{path}")),
        // The boundary surface does not expose the task action's command
        // line yet; `exec` carries the task's backslash path, matching how
        // folder rows carry the item's file path rather than a resolved
        // command.
        exec: path.clone(),
        name,
        enabled: task.enabled,
        source: StartupSource::ScheduledTask,
        scope: StartupScope::Unknown,
        control_policy: StartupControlPolicy::Unsupported,
        locator: StartupEntryLocator::new(path),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: taskmanager_core::StartupImpactUnknownReason::Unsupported,
        },
    })
}

#[cfg(windows)]
impl StartupInventoryProvider for WinStartupInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<StartupEntry>, ProviderFailure> {
        let mut entries = Vec::new();
        let mut failures = 0usize;
        for (subkey, id_prefix, scope, system) in RUN_KEY_PATHS {
            let root = if *system {
                windows_registry::LOCAL_MACHINE
            } else {
                windows_registry::CURRENT_USER
            };
            failures = scan_registry(root, subkey, id_prefix, *scope, &mut entries, failures);
        }
        if let Some(folder) = &self.cached_startup_folder {
            failures = scan_startup_folder(folder, &mut entries, failures);
        }
        let task_source_failure = scan_scheduled_tasks(&mut entries);

        let outcome = startup_inventory_outcome(
            failures,
            self.cached_startup_folder_failure,
            task_source_failure,
        );
        let sources = vec![SourceStatus {
            provider: STARTUP_INVENTORY_PROVIDER,
            outcome,
            item_count: entries.len(),
        }];
        Ok(PartialSourceSnapshot::new(entries, sources))
    }
}

#[cfg(windows)]
fn startup_inventory_outcome(
    failures: usize,
    startup_folder_failure: Option<FailureKind>,
    task_source_failure: Option<FailureKind>,
) -> SourceOutcome {
    // A wholly missing source outranks per-item decode failures: the
    // snapshot must never present the remaining sources as complete.
    if let Some(failure) = startup_folder_failure.or(task_source_failure) {
        SourceOutcome::Partial(failure)
    } else if failures > 0 {
        SourceOutcome::Partial(FailureKind::ProviderFault)
    } else {
        SourceOutcome::Available
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartupApproval {
    Missing,
    Enabled,
    Disabled,
    Unknown,
}

#[cfg(windows)]
const STARTUP_APPROVED_RUN_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";

/// Task Manager's approval store for Startup-folder items: the same 12-byte
/// blob format as `StartupApproved\Run`, keyed by the item's file name. The
/// key lives in HKCU and is user-writable, so folder control never needs to
/// touch the item file itself.
#[cfg(windows)]
const STARTUP_APPROVED_FOLDER_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder";

#[cfg(windows)]
fn startup_approval_key(
    root: &windows_registry::Key,
    id_prefix: &str,
) -> Option<windows_registry::Key> {
    id_prefix
        .starts_with("win:run:")
        .then(|| root.open(STARTUP_APPROVED_RUN_KEY).ok())
        .flatten()
}

#[cfg(windows)]
fn decode_startup_approval(value: &windows_registry::Value) -> StartupApproval {
    // Windows stores StartupApproved\Run values as a 12-byte REG_BINARY.
    // Only the documented status byte is changed; the remaining timestamp and
    // metadata bytes are preserved verbatim. Unknown type/length/status is
    // never guessed.
    if value.ty() != windows_registry::Type::Bytes || value.len() != 12 {
        return StartupApproval::Unknown;
    }
    match value.first().copied() {
        Some(0x02) => StartupApproval::Enabled,
        Some(0x03) => StartupApproval::Disabled,
        Some(_) | None => StartupApproval::Unknown,
    }
}

#[cfg(windows)]
fn startup_approval_state(
    root: &windows_registry::Key,
    id_prefix: &str,
    name: &str,
) -> StartupApproval {
    let Some(key) = startup_approval_key(root, id_prefix) else {
        return StartupApproval::Missing;
    };
    match key.get_value(name) {
        Ok(value) => decode_startup_approval(&value),
        Err(_) => StartupApproval::Missing,
    }
}

/// Approval state of one Startup-folder item, keyed by its file name in the
/// HKCU `StartupApproved\StartupFolder` store. A missing key or value is the
/// documented default (enabled); an unrecognized blob is never guessed.
#[cfg(windows)]
fn startup_folder_approval_state(name: &str) -> StartupApproval {
    let Ok(key) = windows_registry::CURRENT_USER.open(STARTUP_APPROVED_FOLDER_KEY) else {
        return StartupApproval::Missing;
    };
    match key.get_value(name) {
        Ok(value) => decode_startup_approval(&value),
        Err(_) => StartupApproval::Missing,
    }
}

/// Off-Windows fallback: the Run registry keys, Startup folder, and Task
/// Scheduler are absent, so the inventory completes honestly empty with a
/// `MissingDependency` source. Keeps the adapter composable + contract-testable
/// on the Linux CI gate (mirrors the macOS adapter's cross-target model).
#[cfg(not(windows))]
impl StartupInventoryProvider for WinStartupInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<StartupEntry>, ProviderFailure> {
        Ok(PartialSourceSnapshot::new(
            Vec::new(),
            vec![SourceStatus {
                provider: STARTUP_INVENTORY_PROVIDER,
                outcome: SourceOutcome::Unavailable(FailureKind::MissingDependency),
                item_count: 0,
            }],
        ))
    }
}

/// Registry and folder control change only the `StartupApproved` status blob;
/// neither Run values nor folder files are ever deleted, moved, or
/// reconstructed. Scheduled-task mutation stays unsupported until a control
/// seam for the task store is chartered.
pub struct WinStartupControlProvider;

#[cfg(windows)]
impl StartupControlProvider for WinStartupControlProvider {
    fn set_enabled(&mut self, entry: &StartupEntry, enabled: bool) -> Result<(), ProviderFailure> {
        if entry.source == StartupSource::RegistryEntry {
            let (root, subkey, prefix) = registry_root_for(&entry.id);
            if !prefix.starts_with("win:run:") {
                return Err(ProviderFailure::Unsupported);
            }
            let key = root
                .open(subkey)
                .map_err(|_| ProviderFailure::PermissionDenied)?;
            let name = entry
                .id
                .as_str()
                .strip_prefix(&format!("{prefix}:"))
                .ok_or(ProviderFailure::IdentityChanged)?;
            if key.get_value(name).is_err() {
                return Err(ProviderFailure::IdentityChanged);
            }
            let approval_key =
                startup_approval_key(root, prefix).ok_or(ProviderFailure::Unsupported)?;
            let approval = approval_key
                .get_value(name)
                .map_err(|_| ProviderFailure::Unsupported)?;
            if decode_startup_approval(&approval) == StartupApproval::Unknown {
                return Err(ProviderFailure::Unsupported);
            }
            let mut bytes = approval.to_vec();
            if bytes.len() != 12 {
                return Err(ProviderFailure::Unsupported);
            }
            bytes[0] = if enabled { 0x02 } else { 0x03 };
            approval_key
                .set_bytes(name, windows_registry::Type::Bytes, &bytes)
                .map_err(|_| ProviderFailure::PermissionDenied)
        } else if entry.source == StartupSource::StartupFolder {
            set_startup_folder_item_enabled(entry, enabled)
        } else {
            Err(ProviderFailure::Unsupported)
        }
    }
}

/// Folder-item control mirrors Task Manager: the same 12-byte approval blob
/// under `Explorer\StartupApproved\StartupFolder` (HKCU, user-writable), keyed
/// by the item's file name. An existing decodable blob keeps its timestamp
/// and metadata bytes — only the status byte flips; an absent value is
/// created with the documented layout (status byte, zeroed timestamp). The
/// item file itself is never renamed, moved, or deleted, and the file must
/// still exist in the user's Startup folder before anything is written.
#[cfg(windows)]
fn set_startup_folder_item_enabled(
    entry: &StartupEntry,
    enabled: bool,
) -> Result<(), ProviderFailure> {
    let name = entry
        .id
        .as_str()
        .strip_prefix("win:folder:")
        .ok_or(ProviderFailure::IdentityChanged)?;
    if name.is_empty() || name.contains(['\\', '/']) {
        return Err(ProviderFailure::IdentityChanged);
    }
    let folder = user_startup_folder().map_err(ProviderFailure::from_kind)?;
    if !folder.join(name).is_file() {
        return Err(ProviderFailure::IdentityChanged);
    }
    let approval_key = windows_registry::CURRENT_USER
        .create(STARTUP_APPROVED_FOLDER_KEY)
        .map_err(|_| ProviderFailure::PermissionDenied)?;
    let mut bytes = match approval_key.get_value(name) {
        Ok(existing) => {
            if decode_startup_approval(&existing) == StartupApproval::Unknown {
                return Err(ProviderFailure::Unsupported);
            }
            existing.to_vec()
        }
        // No approval value exists yet: create the documented 12-byte layout
        // (status byte, zeroed timestamp and metadata) rather than guessing
        // from an absent or malformed blob.
        Err(_) => vec![0_u8; 12],
    };
    if bytes.len() != 12 {
        return Err(ProviderFailure::Unsupported);
    }
    bytes[0] = if enabled { 0x02 } else { 0x03 };
    approval_key
        .set_bytes(name, windows_registry::Type::Bytes, &bytes)
        .map_err(|_| ProviderFailure::PermissionDenied)
}

/// Off-Windows fallback: no Run registry to mutate, so startup control
/// completes with an honest `MissingDependency` (never fabricates success).
#[cfg(not(windows))]
impl StartupControlProvider for WinStartupControlProvider {
    fn set_enabled(
        &mut self,
        _entry: &StartupEntry,
        _enabled: bool,
    ) -> Result<(), ProviderFailure> {
        Err(ProviderFailure::MissingDependency)
    }
}

#[cfg(windows)]
fn registry_root_for(
    id: &StartupEntryId,
) -> (&'static windows_registry::Key, &'static str, &'static str) {
    let id_str = id.as_str();
    for (subkey, prefix, _, system) in RUN_KEY_PATHS {
        if id_str.starts_with(&format!("{prefix}:")) {
            return (
                if *system {
                    windows_registry::LOCAL_MACHINE
                } else {
                    windows_registry::CURRENT_USER
                },
                subkey,
                prefix,
            );
        }
    }
    // Fall back to the user Run key; callers check `source` before this.
    (
        windows_registry::CURRENT_USER,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        "win:run:hkcu",
    )
}

/// Boot evidence from the Diagnostics-Performance event log. Windows has no
/// systemd-style failed-units or critical-chain concept, so failed units are
/// honestly empty; the critical chain carries one node for the measured boot
/// from the documented boot event (ID 100). Unreadable channels degrade to
/// typed failures instead of an empty success.
pub struct WinStartupEvidenceProvider;

/// Login-session inventory from the native WTS API. WTS does not expose a
/// portable Unix UID, so `uid = 0` remains an explicit compatibility field;
/// session name and user are native strings and RDP sessions are identified by
/// their WTS station name.
pub struct WinSessionInventoryProvider;

/// Login-session control through WTS. `Disconnect` targets a specific WTS
/// session; `Lock` maps to `LockWorkStation`, which locks the calling
/// interactive session — the session this desktop application runs in — and
/// therefore takes no session id.
pub struct WinSessionControlProvider;

fn map_windows_api_failure(error: taskmanager_windows_api::WindowsApiError) -> ProviderFailure {
    match error {
        taskmanager_windows_api::WindowsApiError::Unsupported => ProviderFailure::MissingDependency,
        taskmanager_windows_api::WindowsApiError::PermissionDenied => {
            ProviderFailure::PermissionDenied
        }
        taskmanager_windows_api::WindowsApiError::IdentityChanged
        | taskmanager_windows_api::WindowsApiError::InvalidInput => {
            ProviderFailure::IdentityChanged
        }
        taskmanager_windows_api::WindowsApiError::InvalidText
        | taskmanager_windows_api::WindowsApiError::ResourceLimit
        | taskmanager_windows_api::WindowsApiError::QueryFailed => {
            ProviderFailure::TemporarilyUnavailable
        }
    }
}

pub struct WinEnvironmentProviders {
    startup_inventory:
        ProviderRegistration<StartupInventoryRequest, Box<dyn StartupInventoryProvider>>,
    startup_evidence:
        ProviderRegistration<StartupEvidenceRequest, Box<dyn StartupEvidenceProvider>>,
    startup_control: ProviderRegistration<StartupControlRequest, Box<dyn StartupControlProvider>>,
    session_inventory:
        ProviderRegistration<SessionInventoryRequest, Box<dyn SessionInventoryProvider>>,
    session_control: ProviderRegistration<SessionControlRequest, Box<dyn SessionControlProvider>>,
}

impl WinEnvironmentProviders {
    #[must_use]
    pub fn new<I, E, C, S, C2>(
        startup_inventory: ProviderRegistration<StartupInventoryRequest, I>,
        startup_evidence: ProviderRegistration<StartupEvidenceRequest, E>,
        startup_control: ProviderRegistration<StartupControlRequest, C>,
        session_inventory: ProviderRegistration<SessionInventoryRequest, S>,
        session_control: ProviderRegistration<SessionControlRequest, C2>,
    ) -> Self
    where
        I: StartupInventoryProvider,
        E: StartupEvidenceProvider,
        C: StartupControlProvider,
        S: SessionInventoryProvider,
        C2: SessionControlProvider,
    {
        Self {
            startup_inventory: startup_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn StartupInventoryProvider>),
            startup_evidence: startup_evidence
                .map_provider(|provider| Box::new(provider) as Box<dyn StartupEvidenceProvider>),
            startup_control: startup_control
                .map_provider(|provider| Box::new(provider) as Box<dyn StartupControlProvider>),
            session_inventory: session_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn SessionInventoryProvider>),
            session_control: session_control
                .map_provider(|provider| Box::new(provider) as Box<dyn SessionControlProvider>),
        }
    }

    pub(crate) fn runtime_bindings(&self) -> EnvironmentProviderBindings {
        EnvironmentProviderBindings::from_registrations(
            &self.startup_inventory,
            &self.startup_evidence,
            &self.startup_control,
            &self.session_inventory,
            &self.session_control,
        )
    }

    pub(crate) fn into_runtime(self) -> EnvironmentExecutors {
        let Self {
            startup_inventory,
            startup_evidence,
            startup_control,
            session_inventory,
            session_control,
        } = self;
        let mut startup_inventory = startup_inventory.into_provider();
        let mut startup_evidence = startup_evidence.into_provider();
        let mut startup_control = startup_control.into_provider();
        let mut session_inventory = session_inventory.into_provider();
        let mut session_control = session_control.into_provider();
        EnvironmentExecutors::new(
            move || startup_inventory.refresh(),
            move |observed_at_ms| startup_evidence.observe(observed_at_ms),
            move |entry, enabled| startup_control.set_enabled(entry, enabled),
            move || session_inventory.refresh(),
            move |session_id, action| session_control.control(session_id, action),
        )
    }
}

#[cfg(test)]
#[path = "../../tests/headless/platform_windows_provider_environment.rs"]
mod tests;

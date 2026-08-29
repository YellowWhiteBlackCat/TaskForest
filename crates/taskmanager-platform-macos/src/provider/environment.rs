//! macOS environment-domain providers built exclusively on safe wrappers and
//! bounded `std::process` shell-outs (ADR-019).
//!
//! Startup inventory/control parse LaunchAgents/LaunchDaemons plists with the
//! pure-Rust `plist` crate and rewrite the `Disabled` key for control.
//! Sessions come from `who` (+ `id -u` for uid). Boot evidence shells out to
//! `sysctl -n kern.boottime` (the kernel's boot timestamp struct: a present
//! record means the system booted cleanly). macOS has no systemd-style
//! failed-units or critical-chain concept, so the snapshot's failed-units and
//! critical-chain lists are honestly empty and the wall-clock boot timestamp
//! carried by `kern.boottime` is not surfaced as a duration (the snapshot type
//! has no duration field — future work). Session control (lock/disconnect) has
//! no safe source yet and completes with a typed unsupported outcome.

use std::path::{Path, PathBuf};

use taskmanager_application::{
    SessionControlRequest, SessionInventoryRequest, StartupControlRequest, StartupEvidenceRequest,
    StartupInventoryRequest,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    DeviceState, FailureKind, ProviderId, SessionControlAction, SessionId, SessionItem,
    StartupBootEvidenceSnapshot, StartupControlPolicy, StartupEntry, StartupEntryId,
    StartupEntryLocator, StartupEvidenceFailure, StartupImpact, StartupImpactEvidence,
    StartupScope, StartupSource,
};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    SessionControlProvider, SessionInventoryProvider, StartupControlProvider,
    StartupEvidenceProvider, StartupInventoryProvider,
};
use taskmanager_platform_runtime::{
    EnvironmentExecutors, EnvironmentProviderBindings, ProviderRegistration,
};

const STARTUP_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.startup.inventory.plist");
const SESSION_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("macos.session.inventory.who");

use taskmanager_platform_portable::run_with_timeout;

/// LaunchAgents (user scope) and LaunchDaemons (system scope) directories.
/// `/System/Library/...` is read-only: entries there get an unsupported
/// control policy.
const LAUNCH_DIRS: &[(&str, StartupScope, bool)] = &[
    // (dir, scope, writable)
    ("~/Library/LaunchAgents", StartupScope::User, true),
    ("/Library/LaunchAgents", StartupScope::System, true),
    ("/Library/LaunchDaemons", StartupScope::System, true),
    ("/System/Library/LaunchDaemons", StartupScope::System, false),
];

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

/// One parsed launchd plist row.
struct LaunchPlist {
    label: String,
    program_arguments: Vec<String>,
    disabled: bool,
}

/// Parse a LaunchAgent/Daemon plist with the safe `plist` crate. Missing or
/// malformed files yield `None` (the scan continues; failures are counted).
fn parse_launch_plist(path: &Path) -> Option<LaunchPlist> {
    let value = plist::Value::from_file(path).ok()?;
    let dict = value.as_dictionary()?;
    let label = dict
        .get("Label")
        .and_then(|value| value.as_string())
        .map(ToString::to_string)?;
    let program_arguments = dict
        .get("ProgramArguments")
        .and_then(|value| value.as_array())
        .map(|args| {
            args.iter()
                .filter_map(|arg| arg.as_string())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .or_else(|| {
            dict.get("Program")
                .and_then(|value| value.as_string())
                .map(|program| vec![program.to_string()])
        })
        .unwrap_or_default();
    let disabled = dict
        .get("Disabled")
        .and_then(|value| value.as_boolean())
        .unwrap_or(false);
    Some(LaunchPlist {
        label,
        program_arguments,
        disabled,
    })
}

/// Startup entries from LaunchAgents/LaunchDaemons plists. `enabled` follows
/// the `Disabled` key; `control_policy` is Direct for writable directories
/// and Unsupported for `/System/...`.
pub struct MacStartupInventoryProvider;

impl StartupInventoryProvider for MacStartupInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<StartupEntry>, ProviderFailure> {
        let mut entries = Vec::new();
        let mut failures = 0usize;
        for (dir, scope, writable) in LAUNCH_DIRS {
            let dir = expand_home(dir);
            let Ok(read_dir) = std::fs::read_dir(&dir) else {
                failures += 1;
                continue;
            };
            for entry in read_dir.flatten() {
                let path = entry.path();
                if !matches!(path.extension().and_then(|ext| ext.to_str()), Some("plist")) {
                    continue;
                }
                let Some(parsed) = parse_launch_plist(&path) else {
                    failures += 1;
                    continue;
                };
                let is_daemon = *scope == StartupScope::System;
                entries.push(StartupEntry {
                    id: StartupEntryId::new(format!("macos:launchd:{}", parsed.label)),
                    name: parsed.label.clone(),
                    exec: parsed.program_arguments.join(" "),
                    enabled: !parsed.disabled,
                    source: if is_daemon {
                        StartupSource::SystemService
                    } else {
                        StartupSource::UserService
                    },
                    scope: *scope,
                    control_policy: if *writable {
                        StartupControlPolicy::Direct
                    } else {
                        StartupControlPolicy::Unsupported
                    },
                    locator: StartupEntryLocator::new(path.display().to_string()),
                    impact: StartupImpact::None,
                    impact_evidence: StartupImpactEvidence::Unknown {
                        reason: taskmanager_core::StartupImpactUnknownReason::NotInstrumented,
                    },
                });
            }
        }
        let item_count = entries.len();
        let outcome = if entries.is_empty() && failures > 0 {
            SourceOutcome::Partial(FailureKind::ProviderFault)
        } else {
            SourceOutcome::Available
        };
        Ok(PartialSourceSnapshot::new(
            entries,
            vec![SourceStatus {
                provider: STARTUP_INVENTORY_PROVIDER,
                outcome,
                item_count,
            }],
        ))
    }
}

/// Enable/disable a launchd entry by rewriting the `Disabled` key of its
/// plist (locator = plist path). `/System/...` entries report Unsupported.
pub struct MacStartupControlProvider;

impl StartupControlProvider for MacStartupControlProvider {
    fn set_enabled(&mut self, entry: &StartupEntry, enabled: bool) -> Result<(), ProviderFailure> {
        if entry.control_policy != StartupControlPolicy::Direct {
            return Err(ProviderFailure::Unsupported);
        }
        let path = PathBuf::from(entry.locator.as_str());
        let mut value =
            plist::Value::from_file(&path).map_err(|_| ProviderFailure::IdentityChanged)?;
        let Some(dict) = value.as_dictionary_mut() else {
            return Err(ProviderFailure::IdentityChanged);
        };
        dict.insert("Disabled".into(), plist::Value::Boolean(!enabled));
        value
            .to_file_xml(&path)
            .map_err(|_| ProviderFailure::PermissionDenied)?;
        Ok(())
    }
}

/// Sessions from `who` (bounded shell-out): user, tty, login timestamp and
/// remote host are real; uid comes from `id -u` best-effort.
pub struct MacSessionInventoryProvider;

fn run_capture(command: &mut std::process::Command) -> Option<String> {
    let output = run_with_timeout(command, std::time::Duration::from_secs(2)).ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

impl SessionInventoryProvider for MacSessionInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<SessionItem>, ProviderFailure> {
        let Some(output) = run_capture(&mut std::process::Command::new("who")) else {
            return Err(ProviderFailure::TemporarilyUnavailable);
        };
        let mut items = Vec::new();
        let mut uid_cache: std::collections::HashMap<String, u32> = Default::default();
        for line in output.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 2 {
                continue;
            }
            let user = fields[0].to_string();
            let tty = fields[1].to_string();
            let timestamp = if fields.len() >= 4 {
                Some(format!(
                    "{} {} {}",
                    fields[2],
                    fields[3],
                    fields.get(4).copied().unwrap_or("")
                ))
            } else {
                None
            };
            // `who` shows a remote host in parentheses for SSH sessions.
            let remote = fields
                .last()
                .map(|field| field.starts_with('(') && field.ends_with(')'))
                .unwrap_or(false);
            let uid = *uid_cache.entry(user.clone()).or_insert_with(|| {
                let Some(id_output) =
                    run_capture(std::process::Command::new("id").args(["-u", &user]))
                else {
                    return 0;
                };
                id_output.trim().parse::<u32>().unwrap_or(0)
            });
            items.push(SessionItem {
                id: format!("macos:session:{tty}"),
                uid,
                user,
                seat: None,
                tty: Some(tty),
                remote,
                timestamp,
            });
        }
        if items.is_empty() {
            return Err(ProviderFailure::TemporarilyUnavailable);
        }
        let item_count = items.len();
        Ok(PartialSourceSnapshot::new(
            items,
            vec![SourceStatus {
                provider: SESSION_INVENTORY_PROVIDER,
                outcome: SourceOutcome::Available,
                item_count,
            }],
        ))
    }
}

/// Boot evidence from `sysctl -n kern.boottime` via a bounded shell-out. macOS
/// has no systemd-style failed-units or critical-chain concept, so a present
/// `kern.boottime` struct means the system booted cleanly and both evidence
/// lists stay empty; a missing record (the OID exists but the value did not
/// parse) is reported via the typed `failed_units_failure`/`critical_chain_failure`
/// markers rather than fabricated. On a non-macOS kernel (Linux CI) procps
/// `sysctl` runs but exits non-zero for the BSD-specific `kern.boottime` OID,
/// so the probe degrades to `MissingTool` (never Err, never fabricated). The
/// wall-clock boot timestamp carried by `kern.boottime` is not surfaced as a
/// duration — the snapshot type carries no duration field (future work).
/// `sysctl -n kern.boottime` is a single cheap syscall, so no cache is kept.
pub struct MacStartupEvidenceProvider;

impl StartupEvidenceProvider for MacStartupEvidenceProvider {
    fn observe(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StartupBootEvidenceSnapshot, ProviderFailure> {
        let probe = probe_boottime();
        let failure = match probe {
            BootProbe::Present => None,
            BootProbe::NoRecord => Some(StartupEvidenceFailure::Unavailable),
            BootProbe::AbsentTool => Some(StartupEvidenceFailure::MissingTool),
        };
        Ok(StartupBootEvidenceSnapshot {
            state: DeviceState::healthy(observed_at_ms),
            failed_units_state: DeviceState::healthy(observed_at_ms),
            critical_chain_state: DeviceState::healthy(observed_at_ms),
            failed_units_failure: failure,
            critical_chain_failure: failure,
            failed_units: Vec::new(),
            critical_chain: Vec::new(),
        })
    }
}

/// Outcome of the `sysctl -n kern.boottime` probe.
enum BootProbe {
    /// `sysctl` did not spawn, or ran but the macOS-specific `kern.boottime`
    /// OID is unknown on this kernel (Linux CI: procps `sysctl` exits 1).
    AbsentTool,
    /// `sysctl` ran cleanly but the boottime struct did not parse (macOS-only
    /// anomaly; the OID exists but the value was malformed).
    NoRecord,
    /// `kern.boottime` parsed — the system booted cleanly.
    Present,
}

/// Pure parser for `sysctl -n kern.boottime` stdout: returns true only when a
/// boottime struct line carrying both `sec = ` and `usec = ` fields appears.
/// Unit-tested; does not shell out.
fn parse_boottime_marker(stdout: &str) -> bool {
    // The macOS `kern.boottime` struct prints both fields on one line:
    // `{ sec = <epoch>, usec = <µs> } ...`. Match at TOKEN granularity so the
    // `sec` field is not confused with the tail of `usec` (a naive
    // `contains("sec = ")` would also match `"usec = 0"`).
    stdout.lines().any(|line| {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        tokens.contains(&"sec") && tokens.contains(&"usec")
    })
}

/// Probe `sysctl -n kern.boottime` once per `observe()` call. Degrades to
/// `AbsentTool` when `sysctl` cannot spawn or the BSD `kern.boottime` OID is
/// unknown on the running kernel (Linux CI), and `NoRecord` when `sysctl`
/// succeeds but the boottime struct does not parse — never fabricated.
fn probe_boottime() -> BootProbe {
    let mut command = std::process::Command::new("sysctl");
    command.args(["-n", "kern.boottime"]);
    let output = match run_with_timeout(&mut command, std::time::Duration::from_secs(2)) {
        Ok(output) if output.status.success() => output,
        // procps `sysctl` on Linux exits non-zero for the macOS-only
        // `kern.boottime` OID -> the boottime tool is absent on this kernel.
        Ok(_) => return BootProbe::AbsentTool,
        // Spawn failed (no sysctl binary) or the bounded wait broke: the
        // macOS boottime OID is not reachable on this host.
        Err(_) => return BootProbe::AbsentTool,
    };
    let text = String::from_utf8_lossy(&output.stdout);
    if parse_boottime_marker(&text) {
        BootProbe::Present
    } else {
        BootProbe::NoRecord
    }
}

/// Session control (disconnect/lock) has no safe API yet.
pub struct PendingSessionControlProvider;

impl SessionControlProvider for PendingSessionControlProvider {
    fn control(
        &mut self,
        _session_id: &SessionId,
        _action: SessionControlAction,
    ) -> Result<(), ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

pub struct MacEnvironmentProviders {
    startup_inventory:
        ProviderRegistration<StartupInventoryRequest, Box<dyn StartupInventoryProvider>>,
    startup_evidence:
        ProviderRegistration<StartupEvidenceRequest, Box<dyn StartupEvidenceProvider>>,
    startup_control: ProviderRegistration<StartupControlRequest, Box<dyn StartupControlProvider>>,
    session_inventory:
        ProviderRegistration<SessionInventoryRequest, Box<dyn SessionInventoryProvider>>,
    session_control: ProviderRegistration<SessionControlRequest, Box<dyn SessionControlProvider>>,
}

impl MacEnvironmentProviders {
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
#[path = "../../tests/headless/macos_provider_environment.rs"]
mod tests;

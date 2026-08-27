//! Startup applications — entries that launch automatically at login.
//!
//! XDG is always scanned, then exactly one init-specific source is selected:
//! - **XDG autostart**: freedesktop `.desktop` files under the autostart dirs
//!   (`$XDG_CONFIG_HOME/autostart`, `~/.config/autostart`, and every
//!   `$XDG_CONFIG_DIRS` system autostart dir). `Hidden=true` / `NoDisplay=true`
//!   marks a disabled entry (the spec's toggle).
//! - **systemd user units**: units in the user manager that are enabled for the
//!   default/graphical startup targets.
//! - **OpenRC runlevels**: available services and default-runlevel membership
//!   reported by verbose `rc-update show`.
//!
//! This module is a pure data layer — no gpui/UI deps — so it is unit-testable in
//! isolation and reusable by any frontend. Enable/disable writes back through the
//! same source it came from (toggle `Hidden` for XDG, `systemctl --user` for
//! systemd, or `rc-update` for OpenRC).

mod control;
pub mod evidence;
mod init_source;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
pub use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupEntryId, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};
use taskmanager_core::{FailureKind, ProviderId, SourceOutcome, SourceStatus};
use taskmanager_platform_contract::PartialSourceSnapshot;

pub(super) const STARTUP_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const XDG_PROVIDER_ID: ProviderId = ProviderId::borrowed("linux.startup.xdg");
pub(super) const SYSTEMD_PROVIDER_ID: ProviderId =
    ProviderId::borrowed("linux.startup.systemd-user");
pub(super) const BLAME_PROVIDER_ID: ProviderId =
    ProviderId::borrowed("linux.startup.systemd-blame");

// ── .desktop parsing (pure, unit-tested) ─────────────────────────────────────

/// The subset of a `[Desktop Entry]` we care about for autostart. `hidden` is
/// true when either `Hidden=true` or `NoDisplay=true` (both suppress autostart).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct DesktopEntry {
    pub name: Option<String>,
    pub exec: Option<String>,
    pub hidden: bool,
}

/// Parse a freedesktop `.desktop` file's `[Desktop Entry]` group. Pure; tolerant
/// of blank lines, comments (`#`), localized `Name[xx]=`, and other groups.
pub(crate) fn parse_desktop_entry(text: &str) -> DesktopEntry {
    let mut entry = DesktopEntry::default();
    let mut in_entry = false;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_entry = header.eq_ignore_ascii_case("Desktop Entry");
            continue;
        }
        if !in_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        // Localized keys (`Name[de]=`) bind to the same field; the bare `Name=`
        // is the fallback. First write wins so the unlocalized value sticks.
        let key = key.trim();
        let is_localized = key.contains('[');
        let base = key.split('[').next().unwrap_or(key).trim();
        match base {
            "Name" if entry.name.is_none() && !is_localized => {
                entry.name = Some(value.trim().to_string())
            }
            "Exec" if entry.exec.is_none() => entry.exec = Some(value.trim().to_string()),
            "Hidden" | "NoDisplay" if value.trim().eq_ignore_ascii_case("true") => {
                entry.hidden = true
            }
            _ => {}
        }
    }
    entry
}

// ── systemd-analyze blame parsing (pure) ─────────────────────────────────────

/// Parse `systemd-analyze blame` output into a map of **unit name → activation
/// time in milliseconds**. Each blame line looks like:
///
/// ```text
///   1.234s service-name.service
///   567ms another.service
/// ```
///
/// The first whitespace token is the duration (`<float>s`, `<int>ms`, or rarely
/// `<float>min`); the second is the unit name. Lines that don't match this shape
/// (blank lines, headers, trailing summaries) are silently skipped. Pure (no I/O)
/// so it is unit-testable in isolation. Later looked up by a `UserService`
/// entry's `locator` (which is the unit name) to bucket its [`StartupImpact`].
pub fn parse_systemd_blame(output: &str) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for raw in output.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(time_tok) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(millis) = parse_duration_to_millis(time_tok) else {
            continue;
        };
        map.insert(name.to_string(), millis);
    }
    map
}

/// Parse a single `systemd-analyze blame` duration token into milliseconds.
/// Accepts `567ms`, `1.234s`, `8s`, and `1min` (seconds/minutes rounded to the
/// nearest millisecond). Returns `None` for anything else.
fn parse_duration_to_millis(tok: &str) -> Option<u64> {
    if let Some(num) = tok.strip_suffix("ms") {
        return num.parse::<u64>().ok();
    }
    if let Some(num) = tok.strip_suffix('s') {
        let secs: f64 = num.parse().ok()?;
        return Some((secs * 1000.0).round() as u64);
    }
    if let Some(num) = tok.strip_suffix("min") {
        let mins: f64 = num.parse().ok()?;
        return Some((mins * 60_000.0).round() as u64);
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BlameSnapshot {
    Ready(HashMap<String, u64>),
    Partial(HashMap<String, u64>, FailureKind),
    Failed(FailureKind),
    NotSelected,
}

fn impact_evidence_for(
    source: StartupSource,
    locator: &str,
    blame: &BlameSnapshot,
) -> StartupImpactEvidence {
    if source != StartupSource::UserService {
        return StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        };
    }
    match blame {
        BlameSnapshot::Ready(durations) | BlameSnapshot::Partial(durations, _) => durations
            .get(locator)
            .copied()
            .map(|duration_ms| StartupImpactEvidence::Measured { duration_ms })
            .unwrap_or(StartupImpactEvidence::Unknown {
                reason: StartupImpactUnknownReason::NoRecordForThisBoot,
            }),
        BlameSnapshot::Failed(FailureKind::TimedOut) => StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::TimedOut,
        },
        BlameSnapshot::Failed(FailureKind::Unsupported) | BlameSnapshot::NotSelected => {
            StartupImpactEvidence::Unknown {
                reason: StartupImpactUnknownReason::Unsupported,
            }
        }
        BlameSnapshot::Failed(_) => StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::ProviderUnavailable,
        },
    }
}

// ── XDG autostart directories ────────────────────────────────────────────────

/// Build the autostart scan list from env-shaped inputs — pure (no env access),
/// so the path-resolution logic is unit-testable independent of the process
/// environment and free of `HOME`/`XDG_*` leakage between tests.
///
/// Order follows the freedesktop basedir spec so a user-level entry shadows a
/// same-named system one:
/// 1. user dir — `xdg_config_home` wins; otherwise `home/.config` when `home` is
///    set; omitted entirely when neither is present.
/// 2. system dirs — every absolute `:`-delimited segment of `xdg_config_dirs`,
///    falling back to `/etc/xdg` when unset or invalid.
///
/// Each entry has `/autostart` appended. The list is NOT deduped or
/// existence-filtered here — callers locator missing dirs (see `scan_xdg`).
pub fn autostart_dirs_from_env(
    home: Option<&str>,
    xdg_config_home: Option<&str>,
    xdg_config_dirs: Option<&str>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let user_config = xdg_config_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|value| !value.is_empty())
                .map(|value| PathBuf::from(value).join(".config"))
        })
        .filter(|path| path.is_absolute());
    if let Some(d) = &user_config {
        dirs.push(d.join("autostart"));
    }
    let mut system_configs = xdg_config_dirs
        .filter(|value| !value.is_empty())
        .into_iter()
        .flat_map(|value| value.split(':'))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .collect::<Vec<_>>();
    if system_configs.is_empty() {
        system_configs.push(PathBuf::from("/etc/xdg"));
    }
    for config in system_configs {
        let autostart = config.join("autostart");
        if !dirs.contains(&autostart) {
            dirs.push(autostart);
        }
    }
    dirs
}

fn user_autostart_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok();
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
    user_autostart_dir_from_env(home.as_deref(), xdg_config_home.as_deref())
}

fn user_autostart_dir_from_env(
    home: Option<&str>,
    xdg_config_home: Option<&str>,
) -> Option<PathBuf> {
    xdg_config_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|value| !value.is_empty())
                .map(|value| PathBuf::from(value).join(".config"))
        })
        .filter(|path| path.is_absolute())
        .map(|path| path.join("autostart"))
}

fn native_startup_id(source: StartupSource, locator: &str) -> StartupEntryId {
    let key = match source {
        StartupSource::DesktopEntry => Path::new(locator)
            .file_name()
            .map(|name| format!("desktop:{}", name.to_string_lossy()))
            .unwrap_or_else(|| format!("desktop:{locator}")),
        StartupSource::UserService => format!("user-service:{locator}"),
        StartupSource::RunLevel => format!("runlevel:default:{locator}"),
        StartupSource::SystemService => format!("system-service:{locator}"),
        StartupSource::RegistryEntry => format!("registry:{locator}"),
        StartupSource::ScheduledTask => format!("scheduled-task:{locator}"),
        StartupSource::LoginItem => format!("login-item:{locator}"),
        StartupSource::StartupFolder => format!("startup-folder:{locator}"),
        StartupSource::Other => format!("other:{locator}"),
    };
    StartupEntryId::new(key)
}

/// The autostart dirs to scan for the live process — delegates to
/// [`autostart_dirs_from_env`] reading the current environment.
fn autostart_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").ok();
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let xdg_config_dirs = std::env::var("XDG_CONFIG_DIRS").ok();
    autostart_dirs_from_env(
        home.as_deref(),
        xdg_config_home.as_deref(),
        xdg_config_dirs.as_deref(),
    )
}

// ── manager ──────────────────────────────────────────────────────────────────

/// Scans + controls startup entries. Stateless across calls (each `scan` re-reads
/// the sources); the UI snapshots a `Vec<StartupEntry>` per render the way it does
/// for processes/services.
#[derive(Default, Debug, Clone)]
pub struct StartupManager;

impl StartupManager {
    pub fn new() -> Self {
        Self
    }

    /// All currently-installed startup entries, sorted by display name and
    /// provider-native locator. XDG precedence is resolved by desktop-file id;
    /// unrelated providers are never collapsed merely because labels match.
    #[cfg(feature = "test-support")]
    pub fn scan(&self) -> Vec<StartupEntry> {
        self.scan_snapshot().items
    }

    /// Mixed-source startup observation. Successful entries survive sibling
    /// provider failures and every source reports whether its empty result was
    /// authoritative.
    pub fn scan_snapshot(&self) -> PartialSourceSnapshot<StartupEntry> {
        let selected = self.scan_selected_init_sources();
        let (mut entries, xdg_status) = self.scan_xdg_source();
        entries.extend(selected.entries);
        // Stamp each entry's impact. DesktopEntry stays None (no reliable metric);
        // UserService is bucketed from the blame map (None if the unit is absent).
        for e in &mut entries {
            e.impact_evidence = impact_evidence_for(e.source, e.locator.as_str(), &selected.blame);
            e.impact = match e.impact_evidence {
                StartupImpactEvidence::Measured { duration_ms } => {
                    StartupImpact::from_millis(duration_ms)
                }
                StartupImpactEvidence::Unknown { .. } => StartupImpact::None,
            };
        }
        entries.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.source.as_str().cmp(right.source.as_str()))
                .then_with(|| left.locator.as_str().cmp(right.locator.as_str()))
        });
        let mut sources = vec![xdg_status];
        sources.extend(selected.sources);
        PartialSourceSnapshot::new(entries, sources)
    }

    fn scan_xdg_source(&self) -> (Vec<StartupEntry>, SourceStatus) {
        let mut out = Vec::new();
        let mut failure = None;
        let mut shadowed_desktop_ids = HashSet::new();
        let user_dir = user_autostart_dir();
        for dir in autostart_dirs() {
            let is_user_entry = user_dir.as_ref() == Some(&dir);
            let rd = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    failure = Some(if error.kind() == std::io::ErrorKind::PermissionDenied {
                        FailureKind::PermissionDenied
                    } else {
                        FailureKind::ProviderFault
                    });
                    continue;
                }
            };
            for entry in rd {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        failure = Some(if error.kind() == std::io::ErrorKind::PermissionDenied {
                            FailureKind::PermissionDenied
                        } else {
                            FailureKind::ProviderFault
                        });
                        continue;
                    }
                };
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let Some(desktop_id) = path.file_name().map(|name| name.to_os_string()) else {
                    continue;
                };
                if !shadowed_desktop_ids.insert(desktop_id) {
                    continue;
                }
                let text = match fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(error) => {
                        failure = Some(if error.kind() == std::io::ErrorKind::PermissionDenied {
                            FailureKind::PermissionDenied
                        } else {
                            FailureKind::ProviderFault
                        });
                        continue;
                    }
                };
                let parsed = parse_desktop_entry(&text);
                let Some(name) = parsed.name.clone() else {
                    continue;
                };
                let locator = path.to_string_lossy().into_owned();
                let id = native_startup_id(StartupSource::DesktopEntry, &locator);
                out.push(StartupEntry {
                    id,
                    name,
                    exec: parsed.exec.unwrap_or_default(),
                    // hidden (Hidden/NoDisplay=true) ⇒ disabled.
                    enabled: !parsed.hidden,
                    source: StartupSource::DesktopEntry,
                    scope: if is_user_entry {
                        StartupScope::User
                    } else {
                        StartupScope::System
                    },
                    control_policy: if is_user_entry {
                        StartupControlPolicy::Direct
                    } else {
                        StartupControlPolicy::UserOverride
                    },
                    locator: locator.into(),
                    // XDG has no reliable per-app boot metric → None (set here;
                    // scan() leaves DesktopEntry impact untouched).
                    impact: StartupImpact::None,
                    impact_evidence: StartupImpactEvidence::Unknown {
                        reason: StartupImpactUnknownReason::NotInstrumented,
                    },
                });
            }
        }
        let status = source_status(XDG_PROVIDER_ID, out.len(), failure);
        (out, status)
    }
}

fn source_status(
    provider: ProviderId,
    item_count: usize,
    failure: Option<FailureKind>,
) -> SourceStatus {
    let outcome = match (item_count, failure) {
        (0, Some(failure)) => SourceOutcome::Unavailable(failure),
        (_, Some(failure)) => SourceOutcome::Partial(failure),
        (0, None) => SourceOutcome::Empty,
        (_, None) => SourceOutcome::Available,
    };
    SourceStatus {
        provider,
        outcome,
        item_count,
    }
}

fn blame_source_status(blame: &BlameSnapshot) -> SourceStatus {
    match blame {
        BlameSnapshot::Ready(durations) => source_status(BLAME_PROVIDER_ID, durations.len(), None),
        BlameSnapshot::Partial(durations, failure) => {
            source_status(BLAME_PROVIDER_ID, durations.len(), Some(*failure))
        }
        BlameSnapshot::Failed(failure) => source_status(BLAME_PROVIDER_ID, 0, Some(*failure)),
        BlameSnapshot::NotSelected => {
            source_status(BLAME_PROVIDER_ID, 0, Some(FailureKind::Unsupported))
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_startup_tests.rs"]
mod tests;

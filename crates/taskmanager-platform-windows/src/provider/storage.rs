//! Windows storage-health providers.
//!
//! Filesystem read-only state comes from `sysinfo` (safe). SMART self-test
//! *observation* shells out to bounded `smartctl -a` (route C, ADR-018) and
//! parses the plain-text self-test log section into a typed report — the same
//! smartctl pattern the Linux/macOS adapters use, with an honest
//! `Err(MissingDependency)` when smartmontools is absent (Linux CI / a Windows
//! host without it), never fabricated data. SMART self-test *control* (start)
//! shells out to bounded `smartctl -t <kind> <device>` — the same command the
//! macOS adapter uses — and reports `Running` at 0% on success; smartmontools
//! absent (Linux CI) degrades to `Err(MissingDependency)`, never Unsupported.
//! The optional directory-usage scan facet delegates to the shared pure-safe
//! `std::fs` scanner (`taskmanager_platform_portable::DirectoryUsageScanner`,
//! ADR-018 registered-pending → real wiring, 2026-08-18): the Windows adapter
//! carries no traversal logic of its own.

use std::path::PathBuf;
use std::time::Duration;

use taskmanager_application::{
    DirectoryUsageRequest, SmartControlRequest, SmartObservationRequest, StorageHealthRequest,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    DeviceState, DeviceStatus, DirectoryScanControl, DirectoryScanSpec, DirectoryUsageSnapshot,
    FailureKind, FilesystemHealth, FilesystemHealthSnapshot, FilesystemHealthStatus, ProviderId,
    SmartSelfTestFailure, SmartSelfTestIntent, SmartSelfTestKind, SmartSelfTestPhase,
    SmartSelfTestReport, StorageDeviceTarget,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};
use taskmanager_platform_portable::DirectoryUsageScanner;
use taskmanager_platform_provider::{
    DirectoryUsageProvider, FilesystemHealthProvider, SmartSelfTestControlProvider,
    SmartSelfTestObservationProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, StorageExecutors, StorageProviderBindings,
};

use taskmanager_platform_portable::BoundedCommandError;

use crate::command::run_with_timeout;

const FILESYSTEM_HEALTH_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.storage.filesystem.sysinfo");

/// Filesystem health from `sysinfo`: mount point, fs type and the read-only
/// flag are real. Error counts and integrity state have no safe accessor yet
/// and stay absent (recorded in ADR-018).
pub struct WinFilesystemHealthProvider;

impl WinFilesystemHealthProvider {
    pub fn new() -> Self {
        Self
    }
}

impl FilesystemHealthProvider for WinFilesystemHealthProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<CompositeSourceSnapshot<FilesystemHealthSnapshot>, ProviderFailure> {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let filesystems: Vec<FilesystemHealth> = disks
            .list()
            .iter()
            .map(|disk| {
                let read_only = disk.is_read_only();
                FilesystemHealth {
                    mount_point: PathBuf::from(disk.mount_point()),
                    source: None,
                    fs_type: disk.file_system().to_string_lossy().into_owned(),
                    read_only: Some(read_only),
                    error_count: None,
                    status: if read_only {
                        FilesystemHealthStatus::ReadOnly
                    } else {
                        FilesystemHealthStatus::Healthy
                    },
                    state: DeviceState::healthy(observed_at_ms),
                    // Integrity (CHKDSK/ReFS health) has no safe accessor yet;
                    // the default Unsupported state is the honest marker.
                    integrity_state: DeviceState::default(),
                }
            })
            .collect();
        let outcome = if filesystems.is_empty() {
            SourceOutcome::Unavailable(FailureKind::TemporarilyUnavailable)
        } else {
            SourceOutcome::Available
        };
        Ok(CompositeSourceSnapshot::new(
            FilesystemHealthSnapshot {
                state: DeviceState::healthy(observed_at_ms),
                filesystems: filesystems.clone(),
            },
            vec![SourceStatus {
                provider: FILESYSTEM_HEALTH_PROVIDER,
                outcome,
                item_count: filesystems.len(),
            }],
        ))
    }
}

/// SMART self-test *observation* via a bounded `smartctl -a` shell-out
/// (ADR-018 route C). smartctl's plain-text self-test log section is parsed
/// into the report by the pure [`parse_selftest_log`] helper. When smartctl is
/// absent (Linux CI, or a Windows host without smartmontools) the refresh
/// degrades to `Err(MissingDependency)` — an honest non-Unsupported outcome.
/// The shell-out itself never runs in tests; only the pure parser does.
pub struct WinSmartSelfTestObservationProvider;

impl WinSmartSelfTestObservationProvider {
    pub fn new() -> Self {
        Self
    }
}

impl SmartSelfTestObservationProvider for WinSmartSelfTestObservationProvider {
    fn refresh(
        &mut self,
        target: &StorageDeviceTarget,
        previous: DeviceState,
        observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure> {
        let text = smartctl_text(target.locator.as_str())?;
        let parsed = parse_selftest_log(&text);
        let mut report = SmartSelfTestReport {
            state: previous.transition(DeviceStatus::Healthy, observed_at_ms),
            phase: parsed.phase,
            kind: None,
            progress_pct: parsed.progress_pct,
            lifetime_hours: parsed.lifetime_hours,
            first_error_lba: parsed.first_error_lba,
            failure: None,
        };
        if !parsed.found {
            // smartctl ran but emitted no self-test log section at all: the
            // device has no self-test capability to report. Mirror the macOS
            // adapter's `status_text.is_none() && table.is_none()` marker.
            report.failure = Some(SmartSelfTestFailure::ProviderUnavailable);
        }
        Ok(report)
    }
}

/// smartctl's exit code is a bitmask: bit 0 set means a command-line parse
/// failure (we treat that as `Rejected`); any other bit combination (device
/// open failure, SMART threshold exceeded, ...) still means smartctl produced
/// usable self-test log output. This is intentionally narrower than the
/// macOS/Linux JSON transport, where bits 1–2 also invalidate the sample.
fn smartctl_exit_allows_command(exit_code: Option<i32>) -> bool {
    exit_code.is_some_and(|exit_code| exit_code & 0b001 == 0)
}

/// Run `smartctl -a <device>` and return its plain-text stdout. A missing
/// smartctl binary (Linux CI) maps to `MissingDependency`; a permission-denied
/// spawn maps to `PermissionDenied`; a timeout or bounded-runner failure maps to
/// `TemporarilyUnavailable`; a non-allowed exit code maps to `Rejected`. The
/// caller therefore never fabricates a self-test report.
fn smartctl_text(device: &str) -> Result<String, ProviderFailure> {
    let mut command = std::process::Command::new("smartctl");
    command.args(["-a", device]);
    match run_with_timeout(&mut command, Duration::from_secs(5)) {
        Ok(output) if smartctl_exit_allows_command(output.status.code()) => {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(_) => Err(ProviderFailure::Rejected),
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(ProviderFailure::MissingDependency)
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            Err(ProviderFailure::PermissionDenied)
        }
        Err(_) => Err(ProviderFailure::TemporarilyUnavailable),
    }
}

/// Map a self-test status string onto the platform-neutral phase vocabulary.
/// Mirrors the macOS adapter's parser: keyword substring match, never a
/// fabricated value.
fn parse_phase(status: &str) -> SmartSelfTestPhase {
    let lower = status.to_ascii_lowercase();
    if lower.contains("completed") {
        SmartSelfTestPhase::Completed
    } else if lower.contains("aborted") {
        SmartSelfTestPhase::Aborted
    } else if lower.contains("failed") {
        SmartSelfTestPhase::Failed
    } else if lower.contains("remaining") || lower.contains("in progress") {
        SmartSelfTestPhase::Running
    } else {
        SmartSelfTestPhase::Unknown
    }
}

/// Extract the completion percent from a "(NN% remaining)" fragment, returning
/// `100 - remaining` clamped to `[0, 100]`. Reads digits backwards from the
/// `%` so wrapped forms like "(90% remaining)" parse correctly. Pure: tested.
fn parse_remaining_percent(line: &str) -> Option<f32> {
    let lower = line.to_ascii_lowercase();
    let remaining_at = lower.find("remaining")?;
    let before = &line[..remaining_at];
    let pct_pos = before.rfind('%')?;
    let bytes = before.as_bytes();
    let mut start = pct_pos;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start == pct_pos {
        return None;
    }
    let remaining: f32 = before[start..pct_pos].parse().ok()?;
    Some((100.0 - remaining).clamp(0.0, 100.0))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum SelfTestSection {
    Ata,
    Nvme,
}

/// One parsed self-test observation. Fields left `None`/default when smartctl
/// emits no corresponding data; `found` is false until a recognized self-test
/// log section header (or in-progress status line) is seen.
#[derive(Clone, PartialEq, Debug, Default)]
struct SelfTestParse {
    phase: SmartSelfTestPhase,
    progress_pct: Option<f32>,
    lifetime_hours: Option<u64>,
    first_error_lba: Option<u64>,
    found: bool,
}

/// Parse the most recent entry of an ATA self-test log row:
/// `# N <description> <status> NN% <lifetime_hours> <lba>`. The status keyword
/// drives the phase; the percentage (remaining) drives progress when running;
/// the two trailing columns drive lifetime hours and first-error LBA. A `-` in
/// either trailing column yields `None`. Pure: unit-tested.
fn parse_ata_selftest_row(row: &str) -> SelfTestParse {
    let mut entry = SelfTestParse {
        phase: parse_phase(row),
        ..Default::default()
    };
    let tokens: Vec<&str> = row.split_whitespace().collect();
    let pct_idx = tokens.iter().position(|token| {
        token.len() > 1
            && token.ends_with('%')
            && token[..token.len() - 1]
                .bytes()
                .all(|byte| byte.is_ascii_digit())
    });
    if let Some(idx) = pct_idx
        && let Some(remaining) = tokens[idx]
            .strip_suffix('%')
            .and_then(|digits| digits.parse::<f32>().ok())
    {
        if entry.phase == SmartSelfTestPhase::Running {
            entry.progress_pct = Some((100.0 - remaining).clamp(0.0, 100.0));
        }
        if let Some(lifetime) = tokens.get(idx + 1) {
            entry.lifetime_hours = lifetime.parse::<u64>().ok();
        }
        if let Some(lba) = tokens.get(idx + 2)
            && *lba != "-"
        {
            entry.first_error_lba = lba.parse::<u64>().ok();
        }
    }
    entry
}

/// Parse smartctl's plain-text self-test output — ATA log section, NVMe log
/// section, and the standalone "Self-test execution status" / "Self-test
/// status:" lines — into one observation. Missing sections leave every field
/// absent and `found` false so the caller marks the report
/// `ProviderUnavailable`. Pure: unit-tested with fixture excerpts (no shell-out).
fn parse_selftest_log(text: &str) -> SelfTestParse {
    let mut parse = SelfTestParse::default();
    let mut section: Option<SelfTestSection> = None;
    let mut ata_header_consumed = false;
    let mut ata_row_taken = false;
    let mut nvme_in_target_block = false;

    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();

        if lower.starts_with("smart self-test log") {
            section = Some(SelfTestSection::Ata);
            parse.found = true;
            ata_header_consumed = false;
            ata_row_taken = false;
            nvme_in_target_block = false;
            continue;
        }
        if lower.starts_with("nvme self-test log") {
            section = Some(SelfTestSection::Nvme);
            parse.found = true;
            nvme_in_target_block = false;
            continue;
        }
        // ATA current self-test execution status (appears before the log).
        if lower.starts_with("self-test execution status") {
            parse.found = true;
            if lower.contains("remaining") {
                parse.phase = SmartSelfTestPhase::Running;
                parse.progress_pct = parse_remaining_percent(trimmed);
            } else {
                let detected = parse_phase(trimmed);
                if detected != SmartSelfTestPhase::Unknown {
                    parse.phase = detected;
                }
            }
            continue;
        }
        // NVMe current self-test status line. smartctl prints the actual
        // remaining percent only while a test runs; the idle marker
        // "No self-test in progress" contains the negated substring
        // "in progress", so we deliberately do NOT fall back to parse_phase
        // here -- the result[0] block below drives the phase for idle disks.
        if lower.starts_with("self-test status:") {
            parse.found = true;
            if lower.contains("remaining") {
                parse.phase = SmartSelfTestPhase::Running;
                parse.progress_pct = parse_remaining_percent(trimmed);
            }
            continue;
        }

        match section {
            Some(SelfTestSection::Ata) => {
                if !ata_header_consumed && lower.starts_with("num") {
                    ata_header_consumed = true;
                    continue;
                }
                if ata_header_consumed && !ata_row_taken && trimmed.starts_with('#') {
                    let row = parse_ata_selftest_row(trimmed);
                    // The log row only refines an Idle phase; an in-progress
                    // signal from the status line above is authoritative.
                    if parse.phase == SmartSelfTestPhase::Idle {
                        parse.phase = row.phase;
                    }
                    parse.lifetime_hours = row.lifetime_hours;
                    parse.first_error_lba = row.first_error_lba;
                    if parse.phase == SmartSelfTestPhase::Running && parse.progress_pct.is_none() {
                        parse.progress_pct = row.progress_pct;
                    }
                    ata_row_taken = true;
                }
            }
            Some(SelfTestSection::Nvme) => {
                if lower.starts_with("self-test result[") {
                    // result[0] is the most recent; ignore later blocks.
                    nvme_in_target_block = lower
                        .strip_prefix("self-test result[")
                        .is_some_and(|rest| rest.starts_with('0'));
                    continue;
                }
                if nvme_in_target_block && let Some(rest) = lower.strip_prefix("status:") {
                    let detected = parse_phase(rest.trim());
                    if parse.phase == SmartSelfTestPhase::Idle
                        && detected != SmartSelfTestPhase::Unknown
                    {
                        parse.phase = detected;
                    }
                    continue;
                }
                if nvme_in_target_block
                    && let Some(rest) = lower.strip_prefix("power on hours:")
                    && parse.lifetime_hours.is_none()
                {
                    parse.lifetime_hours = rest.trim().parse::<u64>().ok();
                    continue;
                }
                if nvme_in_target_block
                    && let Some(rest) = lower.strip_prefix("lba of first error:")
                    && parse.first_error_lba.is_none()
                {
                    let value = rest.trim();
                    if value != "-" {
                        parse.first_error_lba = value.parse::<u64>().ok();
                    }
                }
            }
            None => {}
        }
    }
    parse
}

/// Map a [`SmartSelfTestKind`] onto smartctl's `-t` token. Mirrors the macOS
/// adapter's helper exactly so the three platform control providers agree on
/// the command surface.
fn smartctl_token(kind: SmartSelfTestKind) -> &'static str {
    match kind {
        SmartSelfTestKind::Short => "short",
        SmartSelfTestKind::Extended => "long",
        SmartSelfTestKind::Conveyance => "conveyance",
    }
}

/// Start a SMART self-test through a bounded `smartctl -t <kind> <device>`
/// shell-out (ADR-018 route C). Mirrors the macOS control provider in spirit:
/// on an allowed smartctl exit it returns a `Running` report at 0% progress; a
/// command-line parse failure (smartctl exit bit 0) is `Rejected`; smartmontools
/// absent (Linux CI, or a Windows host without it) degrades to
/// `Err(MissingDependency)`; a timeout or wait failure is
/// `TemporarilyUnavailable`. Never Unsupported, never a fabricated success.
pub struct WinSmartSelfTestControlProvider;

impl WinSmartSelfTestControlProvider {
    pub fn new() -> Self {
        Self
    }
}

impl SmartSelfTestControlProvider for WinSmartSelfTestControlProvider {
    fn start(
        &mut self,
        intent: &SmartSelfTestIntent,
        _observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure> {
        let device = intent.device_key.as_str();
        let mut command = std::process::Command::new("smartctl");
        command.args(["-t", smartctl_token(intent.kind), device]);
        match run_with_timeout(&mut command, Duration::from_secs(5)) {
            Ok(output) if smartctl_exit_allows_command(output.status.code()) => {
                Ok(SmartSelfTestReport {
                    state: DeviceState::healthy(intent.device_generation.get()),
                    phase: SmartSelfTestPhase::Running,
                    kind: Some(intent.kind),
                    progress_pct: Some(0.0),
                    ..SmartSelfTestReport::default()
                })
            }
            Ok(_) => Err(ProviderFailure::Rejected),
            Err(BoundedCommandError::Spawn(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                Err(ProviderFailure::MissingDependency)
            }
            Err(_) => Err(ProviderFailure::TemporarilyUnavailable),
        }
    }
}

/// Windows directory-usage scan provider: a one-line forwarder over the
/// shared [`DirectoryUsageScanner`] (pure safe `std::fs`, bounded depth +
/// entries + report, symlink-loop-safe, per-directory typed
/// `PermissionDenied`, cancellable chunks). Kept as a distinct type so the
/// Windows provider graph retains its concrete provider identity and
/// registration. This closes the registered-pending facet from the ADR-018
/// table (owner decision 5-5 sequenced it after the per-process history
/// ring, which shipped as G-20).
pub struct WinDirectoryUsageProvider(DirectoryUsageScanner);

impl WinDirectoryUsageProvider {
    pub fn new() -> Self {
        Self(DirectoryUsageScanner::new())
    }
}

impl Default for WinDirectoryUsageProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryUsageProvider for WinDirectoryUsageProvider {
    fn scan_chunk(
        &mut self,
        spec: &DirectoryScanSpec,
        control: &DirectoryScanControl,
        observed_at_ms: u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure> {
        self.0.scan_chunk(spec, control, observed_at_ms)
    }
}

pub struct WinStorageProviders {
    filesystems: ProviderRegistration<StorageHealthRequest, Box<dyn FilesystemHealthProvider>>,
    smart_observation:
        ProviderRegistration<SmartObservationRequest, Box<dyn SmartSelfTestObservationProvider>>,
    smart_control: ProviderRegistration<SmartControlRequest, Box<dyn SmartSelfTestControlProvider>>,
    directory_usage:
        Option<ProviderRegistration<DirectoryUsageRequest, Box<dyn DirectoryUsageProvider>>>,
}

impl WinStorageProviders {
    #[must_use]
    pub fn new<F, O, C>(
        filesystems: ProviderRegistration<StorageHealthRequest, F>,
        smart_observation: ProviderRegistration<SmartObservationRequest, O>,
        smart_control: ProviderRegistration<SmartControlRequest, C>,
    ) -> Self
    where
        F: FilesystemHealthProvider,
        O: SmartSelfTestObservationProvider,
        C: SmartSelfTestControlProvider,
    {
        Self {
            filesystems: filesystems
                .map_provider(|provider| Box::new(provider) as Box<dyn FilesystemHealthProvider>),
            smart_observation: smart_observation.map_provider(|provider| {
                Box::new(provider) as Box<dyn SmartSelfTestObservationProvider>
            }),
            smart_control: smart_control.map_provider(|provider| {
                Box::new(provider) as Box<dyn SmartSelfTestControlProvider>
            }),
            directory_usage: None,
        }
    }

    /// Attach the optional directory-usage scan facet (the same builder shape
    /// the Linux/macOS adapters use for their real providers).
    #[must_use]
    pub fn with_directory_usage<D>(
        mut self,
        directory_usage: ProviderRegistration<DirectoryUsageRequest, D>,
    ) -> Self
    where
        D: DirectoryUsageProvider,
    {
        self.directory_usage = Some(
            directory_usage
                .map_provider(|provider| Box::new(provider) as Box<dyn DirectoryUsageProvider>),
        );
        self
    }

    pub(crate) fn runtime_bindings(&self) -> StorageProviderBindings {
        let bindings = StorageProviderBindings::from_registrations(
            &self.filesystems,
            &self.smart_observation,
            &self.smart_control,
        );
        match &self.directory_usage {
            Some(directory_usage) => bindings.with_directory_usage(directory_usage),
            None => bindings,
        }
    }

    pub(crate) fn into_runtime(self) -> StorageExecutors {
        let Self {
            filesystems,
            smart_observation,
            smart_control,
            directory_usage,
        } = self;
        let mut filesystems = filesystems.into_provider();
        let mut smart_observation = smart_observation.into_provider();
        let mut smart_control = smart_control.into_provider();
        let executors = StorageExecutors::new(
            move |observed_at_ms| filesystems.refresh(observed_at_ms),
            move |target, previous, observed_at_ms| {
                smart_observation.refresh(target, previous, observed_at_ms)
            },
            move |intent, observed_at_ms| smart_control.start(intent, observed_at_ms),
        );
        match directory_usage {
            Some(directory_usage) => {
                let mut directory_usage = directory_usage.into_provider();
                executors.with_directory_usage(move |spec, control, observed_at_ms| {
                    directory_usage.scan_chunk(spec, control, observed_at_ms)
                })
            }
            None => executors,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/platform_windows_provider_storage.rs"]
mod tests;

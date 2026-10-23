//! `systemd-analyze` boot-phase summary: firmware / loader / kernel / user space.
//!
//! The systemd boot summary is the one Linux source that splits a boot into
//! the phases the shared contract publishes. It is a plain text report, so the
//! parsing lives here as a pure function and the bounded command execution
//! stays in the collector.
//!
//! Honesty contract: a phase the platform did not measure is simply absent
//! from the summary (a host without firmware timing prints no `firmware`
//! term), so it stays unmeasured instead of borrowing a neighbouring phase's
//! value. A source that cannot be read answers through the typed failure
//! path, never with a zero-length phase.

use std::process::Command;
use std::time::Duration;

use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::startup::{
    StartupEvidenceFailure, StartupPhaseKind, StartupTimelineSnapshot,
};
#[cfg(target_os = "linux")]
use taskmanager_platform_portable::run_with_timeout;

use super::evidence::{command_failure, failed_output_failure};

/// Timeout for the bounded `systemd-analyze` invocation.
const PHASE_TIMEOUT: Duration = Duration::from_secs(2);

/// The systemd time units the boot summary uses. The order matters: the
/// longest suffix that matches wins, so `ms` never loses to `s`.
const TIMESPAN_UNITS: [&str; 7] = ["min", "us", "ms", "h", "d", "w", "s"];

/// The typed outcome of the boot-phase summary source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootPhaseTimeline {
    /// The typed state of the source.
    pub state: DeviceState,
    /// The typed failure of the source; `None` when the source was read.
    pub failure: Option<StartupEvidenceFailure>,
    /// The measured phase split; `None` when this boot published no split.
    pub timeline: Option<StartupTimelineSnapshot>,
}

/// Parse the `systemd-analyze` boot summary into a typed phase split.
///
/// The summary line looks like:
///
/// ```text
/// Startup finished in 3.192s (firmware) + 1.281s (loader) + 6.585s (kernel) + 30.101s (userspace) = 41.159s
/// ```
///
/// A phase the platform did not measure is absent from the line, so it stays
/// unmeasured. The trailing `= <total>` is deliberately ignored: the total is
/// a sum of phases this contract does not model (the initrd span), and using
/// it as a phase duration would be a claim the summary never made.
pub fn parse_systemd_analyze_phases(output: &str) -> StartupTimelineSnapshot {
    let mut snapshot = StartupTimelineSnapshot::default();
    for (kind, duration_ms) in parse_systemd_analyze_phase_terms(output) {
        match kind {
            StartupPhaseKind::Firmware => snapshot.firmware_ms = Some(duration_ms),
            StartupPhaseKind::Loader => snapshot.loader_ms = Some(duration_ms),
            StartupPhaseKind::Kernel => snapshot.kernel_ms = Some(duration_ms),
            StartupPhaseKind::UserSpace => snapshot.user_space_ms = Some(duration_ms),
        }
    }
    snapshot
}

/// The measured phase terms of the summary, in the order systemd printed them.
fn parse_systemd_analyze_phase_terms(output: &str) -> Vec<(StartupPhaseKind, u64)> {
    let mut terms = Vec::new();
    for line in output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        for (index, token) in tokens.iter().enumerate() {
            let Some(label) = token
                .strip_prefix('(')
                .and_then(|rest| rest.strip_suffix(')'))
            else {
                continue;
            };
            let Some(kind) = phase_kind(label) else {
                continue;
            };
            let Some(duration_ms) = parse_timespan_before(&tokens[..index]) else {
                continue;
            };
            terms.push((kind, duration_ms));
        }
    }
    terms
}

/// The phase a systemd summary label names. Labels this contract does not
/// model (for example the initrd span) stay unparsed.
fn phase_kind(label: &str) -> Option<StartupPhaseKind> {
    match label {
        "firmware" => Some(StartupPhaseKind::Firmware),
        "loader" => Some(StartupPhaseKind::Loader),
        "kernel" => Some(StartupPhaseKind::Kernel),
        "userspace" => Some(StartupPhaseKind::UserSpace),
        _ => None,
    }
}

/// Parse the systemd timespan that ends at the last token of `tokens`.
///
/// systemd prints a compound span for long durations (`1min 3.500s`), so a
/// coarser unit in front of the finer one is accepted as well.
fn parse_timespan_before(tokens: &[&str]) -> Option<u64> {
    let last = *tokens.last()?;
    let fine = parse_timespan_component(last);
    if tokens.len() >= 2 {
        let previous = tokens[tokens.len() - 2];
        if let Some((value, unit)) = split_timespan_component(previous)
            && is_coarse_timespan_unit(unit)
            && let (Some(coarse), Some(fine)) = (scale_timespan(value, unit), fine)
        {
            return coarse.checked_add(fine);
        }
    }
    fine
}

/// Parse one systemd timespan token into milliseconds.
fn parse_timespan_component(token: &str) -> Option<u64> {
    let (value, unit) = split_timespan_component(token)?;
    scale_timespan(value, unit)
}

/// Split a systemd timespan token into its numeric part and its unit.
fn split_timespan_component(token: &str) -> Option<(f64, &'static str)> {
    for unit in TIMESPAN_UNITS {
        if let Some(number) = token.strip_suffix(unit) {
            return number.parse::<f64>().ok().map(|value| (value, unit));
        }
    }
    None
}

/// Whether a unit is coarse enough to precede a finer one in a compound span.
fn is_coarse_timespan_unit(unit: &str) -> bool {
    matches!(unit, "min" | "h" | "d" | "w")
}

/// Scale a systemd timespan component into milliseconds.
fn scale_timespan(value: f64, unit: &str) -> Option<u64> {
    let millis = match unit {
        "us" => value / 1_000.0,
        "ms" => value,
        "s" => value * 1_000.0,
        "min" => value * 60_000.0,
        "h" => value * 3_600_000.0,
        "d" => value * 86_400_000.0,
        "w" => value * 604_800_000.0,
        _ => return None,
    };
    millis.is_finite().then(|| millis.round() as u64)
}

/// Collect the boot-phase split from the systemd boot summary.
#[cfg(target_os = "linux")]
pub fn collect_boot_phase_timeline(now_ms: u64) -> BootPhaseTimeline {
    let mut command = Command::new("systemd-analyze");
    command.args(["--no-pager"]);
    match run_with_timeout(&mut command, PHASE_TIMEOUT) {
        Ok(output) if output.status.success() => {
            let snapshot = parse_systemd_analyze_phases(&String::from_utf8_lossy(&output.stdout));
            BootPhaseTimeline {
                state: DeviceState::healthy(now_ms),
                failure: None,
                timeline: snapshot.has_measured_phase().then_some(snapshot),
            }
        }
        Ok(output) => {
            let (failure, status) = failed_output_failure(&output.stderr);
            failed_timeline(failure, status, now_ms)
        }
        Err(error) => {
            let (failure, status) = command_failure(error);
            failed_timeline(failure, status, now_ms)
        }
    }
}

/// Collect the boot-phase split on a platform without a systemd source.
#[cfg(not(target_os = "linux"))]
pub fn collect_boot_phase_timeline(now_ms: u64) -> BootPhaseTimeline {
    failed_timeline(
        StartupEvidenceFailure::Unsupported,
        DeviceStatus::Unsupported,
        now_ms,
    )
}

/// A typed-unavailable phase source: the split stays absent instead of
/// collapsing into a set of zero-length phases.
fn failed_timeline(
    failure: StartupEvidenceFailure,
    status: DeviceStatus,
    now_ms: u64,
) -> BootPhaseTimeline {
    BootPhaseTimeline {
        state: DeviceState::default().transition(status, now_ms),
        failure: Some(failure),
        timeline: None,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_startup_phase_tests.rs"]
mod tests;

//! Escalation crossing for the RAPL package-power helper (ADR-023,
//! Boundary 2): the `pkexec` driver + the std-only contract parser for the
//! helper's JSON. Mirrors the perf and SMBIOS crossings — a thin process seam,
//! a fail-closed parser, and [`invoke_rapl_helper`] as the one public entry.
//!
//! Shared JSON contract (must match `taskmanager-rapl-helper` exactly):
//! ```text
//! SUCCESS: {"schema":1,"sample_ms":<u32>,
//!           "packages":[{"name":"<string>","power_w":<f32 finite >=0>,
//!                        "energy_delta_uj":<u64>}]}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_rapl"|"open_failed"|"read_failed",
//!           "detail":"<string>"}
//! ```
//! A SUCCESS object has NO `status`; an ERROR object has NO `packages`.

#![forbid(unsafe_code)]

use std::io;

use crate::EscalationDenialReason;

use super::json_reader::{Json, JsonReader};
use super::{HelperOutput, classify_pkexec_no_contract};

/// The helper program the polkit `.policy` authorizes; must equal the
/// `org.freedesktop.policykit.exec.path` annotation in
/// `polkit/io.github.YellowWhiteBlackCat.TaskForest.rapl-helper.policy.in`.
#[cfg(target_os = "linux")]
pub(crate) const RAPL_HELPER_PATH: &str = "/usr/libexec/taskforest-rapl-helper";

/// The sanity ceiling for a package power reading, in watts. RAPL packages
/// are physical CPU sockets (well under 1000 W); the bound only rejects
/// non-physical garbage so a consumer never renders an absurd watt figure as
/// if it were real telemetry.
const MAX_PLAUSIBLE_POWER_W: f64 = 100_000.0;

/// One package power reading parsed from a SUCCESS object's `packages` array.
#[derive(Debug, Clone, PartialEq)]
pub struct RaplPackageReading {
    /// Package label from sysfs, e.g. `"package-1"`.
    pub name: String,
    /// Average package power over the helper's sample window, in watts.
    pub power_w: f32,
    /// The raw energy delta over the sample window, in microjoules.
    pub energy_delta_uj: u64,
}

/// The typed SUCCESS payload: the sample window and the per-package readings.
#[derive(Debug, Clone, PartialEq)]
pub struct RaplPowerSuccess {
    /// Contract schema version; the parser requires exactly `1`.
    pub schema: u32,
    /// The measurement window in milliseconds the helper sampled.
    pub sample_ms: u32,
    /// The per-package power readings, sorted by package index.
    pub packages: Vec<RaplPackageReading>,
}

/// The typed error `kind` the helper emits in an ERROR object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaplHelperErrorKind {
    /// `permission_denied` — the helper still lacked read permission on the
    /// 0400 `energy_uj` nodes.
    PermissionDenied,
    /// `no_rapl` — no top-level `intel-rapl:*` packages on this host.
    NoRapl,
    /// `open_failed` — the powercap directory could not be opened for a
    /// non-permission reason.
    OpenFailed,
    /// `read_failed` — a package node read failed for a non-permission
    /// reason.
    ReadFailed,
}

impl RaplHelperErrorKind {
    /// The lowercase contract string this variant maps to.
    #[must_use]
    pub const fn as_contract_str(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::NoRapl => "no_rapl",
            Self::OpenFailed => "open_failed",
            Self::ReadFailed => "read_failed",
        }
    }

    fn from_contract_str(raw: &str) -> Option<Self> {
        Some(match raw {
            "permission_denied" => Self::PermissionDenied,
            "no_rapl" => Self::NoRapl,
            "open_failed" => Self::OpenFailed,
            "read_failed" => Self::ReadFailed,
            _ => return None,
        })
    }
}

/// A typed ERROR payload the helper emitted (it ran, but produced no power
/// reading for a documented reason).
#[derive(Debug, Clone, PartialEq)]
pub struct RaplHelperError {
    /// The contract `kind`, typed.
    pub kind: RaplHelperErrorKind,
    /// The helper's human-readable detail string.
    pub detail: String,
}

/// The full typed outcome of one RAPL escalation attempt. Mirrors
/// [`super::PerfHelperOutcome`]: no variant fabricates a power figure.
#[derive(Debug, Clone, PartialEq)]
pub enum RaplHelperOutcome {
    /// The helper ran and emitted a valid SUCCESS object.
    Success(RaplPowerSuccess),
    /// The helper ran and emitted a valid ERROR object.
    HelperError(RaplHelperError),
    /// The escalation layer could not deliver usable data (typed reason).
    Unavailable {
        /// Typed reason the escalation could not deliver data.
        reason: EscalationDenialReason,
        /// Host-specific detail for logs/diagnostics.
        detail: String,
    },
}

/// What `parse_helper_output` extracted from a raw stdout blob.
#[derive(Debug, Clone, PartialEq)]
enum ParsedOutput {
    Success(RaplPowerSuccess),
    HelperError(RaplHelperError),
    NotContract,
}

/// Parse a raw helper stdout blob into one of the three contract outcomes.
/// SUCCESS is distinguished by the presence of `packages`; ERROR by
/// `status == "error"`. Anything else is `NotContract`.
fn parse_helper_output(stdout: &str) -> ParsedOutput {
    let json = match JsonReader::parse(stdout) {
        Ok(value) => value,
        Err(()) => return ParsedOutput::NotContract,
    };
    if json.get("packages").is_some() {
        return match parse_success(&json) {
            Some(success) => ParsedOutput::Success(success),
            None => ParsedOutput::NotContract,
        };
    }
    if json.get("status").and_then(Json::as_str) == Some("error") {
        return match parse_error(&json) {
            Some(error) => ParsedOutput::HelperError(error),
            None => ParsedOutput::NotContract,
        };
    }
    ParsedOutput::NotContract
}

/// Validate a SUCCESS-shaped object; `None` on any contract violation.
fn parse_success(json: &Json) -> Option<RaplPowerSuccess> {
    let schema = super::integer_field(json, "schema")?;
    if schema != 1 {
        return None;
    }
    let sample_ms = super::integer_field(json, "sample_ms")?;
    let raw_packages = json.get("packages")?.as_array()?;
    let mut packages = Vec::with_capacity(raw_packages.len());
    for entry in raw_packages {
        let name = entry.get("name")?.as_str()?;
        if name.is_empty() {
            return None;
        }
        let Json::Number(power) = entry.get("power_w")? else {
            return None;
        };
        let power = *power;
        // Reject non-finite, negative, or non-physical watt figures — never
        // fabricate a clamped or NaN power reading.
        if !power.is_finite() || power < 0.0 || power > MAX_PLAUSIBLE_POWER_W {
            return None;
        }
        packages.push(RaplPackageReading {
            name: name.to_owned(),
            power_w: power as f32,
            energy_delta_uj: u64_field(entry, "energy_delta_uj")?,
        });
    }
    Some(RaplPowerSuccess {
        schema,
        sample_ms,
        packages,
    })
}

/// Validate an ERROR-shaped object; `None` on an unknown `kind`.
fn parse_error(json: &Json) -> Option<RaplHelperError> {
    let kind_raw = json.get("kind")?.as_str()?;
    let kind = RaplHelperErrorKind::from_contract_str(kind_raw)?;
    let detail = json.get("detail")?.as_str()?.to_owned();
    Some(RaplHelperError { kind, detail })
}

/// Read a JSON number field as a non-negative integer fitting `u64`.
fn u64_field(json: &Json, key: &str) -> Option<u64> {
    let Json::Number(value) = json.get(key)? else {
        return None;
    };
    if !value.is_finite() || value.fract() != 0.0 || *value < 0.0 {
        return None;
    }
    Some(*value as u64)
}

/// Side-effect-free process seam: production runs `pkexec`; tests return a
/// canned `HelperOutput` or a synthetic `io::Error`.
pub trait RaplHelperProcess {
    /// Run the privileged helper once and capture its stdout + exit status.
    fn run(&self) -> io::Result<HelperOutput>;
}

/// Production process driver: `pkexec <helper>` via `std::process::Command`.
/// Linux-only — polkit/pkexec do not exist on macOS/Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct PkexecRaplHelper;

impl PkexecRaplHelper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl RaplHelperProcess for PkexecRaplHelper {
    fn run(&self) -> io::Result<HelperOutput> {
        use std::process::{Command, Stdio};
        let mut command = Command::new("pkexec");
        command
            .arg(RAPL_HELPER_PATH)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let output = super::bounded_runner::run_bounded(
            &mut command,
            super::bounded_runner::INTERACTIVE_PKEXEC_DEADLINE,
        )
        .map_err(|error| error.into_io_error("the pkexec rapl helper"))?;
        Ok(HelperOutput {
            status_code: output.status_code,
            stdout: output.stdout,
        })
    }
}

/// Drive one helper invocation through `process`, then map the raw reply to a
/// typed outcome. Mapping rules mirror `invoke_perf_helper_with`.
pub fn invoke_rapl_helper_with<P: RaplHelperProcess>(process: &P) -> RaplHelperOutcome {
    let output = match process.run() {
        Ok(output) => output,
        Err(error) => {
            let detail = if error.kind() == io::ErrorKind::TimedOut {
                format!("the pkexec rapl helper crossing was killed at its deadline: {error}")
            } else {
                format!("could not spawn the pkexec rapl helper: {error}")
            };
            return RaplHelperOutcome::Unavailable {
                reason: EscalationDenialReason::HelperUnavailable,
                detail,
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    match parse_helper_output(&stdout) {
        ParsedOutput::Success(success) => RaplHelperOutcome::Success(success),
        ParsedOutput::HelperError(error) => RaplHelperOutcome::HelperError(error),
        ParsedOutput::NotContract => {
            let (reason, detail) = classify_pkexec_no_contract(
                output.status_code,
                rapl_helper_path_for_detail(),
                &stdout,
            );
            RaplHelperOutcome::Unavailable { reason, detail }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn rapl_helper_path_for_detail() -> &'static str {
    "the annotated helper path"
}

#[cfg(target_os = "linux")]
fn rapl_helper_path_for_detail() -> &'static str {
    RAPL_HELPER_PATH
}

/// Run the privileged helper end-to-end via the production `pkexec` driver.
/// Non-Linux hosts fail closed as `Unavailable { Unsupported }`.
#[cfg(target_os = "linux")]
pub fn invoke_rapl_helper() -> RaplHelperOutcome {
    invoke_rapl_helper_with(&PkexecRaplHelper::new())
}

#[cfg(not(target_os = "linux"))]
pub fn invoke_rapl_helper() -> RaplHelperOutcome {
    RaplHelperOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "pkexec/polkit per-feature escalation is Linux-only".to_owned(),
    }
}

#[cfg(test)]
#[path = "../../tests/headless/escalation_polkit_rapl.rs"]
mod tests;

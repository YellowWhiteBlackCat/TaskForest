//! Escalation crossing for the MSR readout helper (ADR-023/048, Boundary 2):
//! the `pkexec` driver + the std-only contract parser for the helper's JSON.
//! Mirrors the RAPL crossing — a thin process seam, a fail-closed parser, and
//! [`invoke_msr_helper`] as the one public entry.
//!
//! Shared JSON contract (must match `taskmanager-msr-helper` exactly):
//! ```text
//! SUCCESS: {"schema":1,
//!           "packages":[{"cpu":<u32>,"bclk_mhz":<f32 finite|null>,
//!                        "temperature_c":<f32 finite|null>,
//!                        "multiplier":<f32 finite|null>,
//!                        "multiplier_min":<f32 finite|null>,
//!                        "multiplier_max":<f32 finite|null>,
//!                        "vcore_v":<f32 finite|null>}]}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_msr"|"open_failed"|"read_failed",
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
/// `polkit/io.github.YellowWhiteBlackCat.TaskForest.msr-helper.policy.in`.
#[cfg(target_os = "linux")]
pub(crate) const MSR_HELPER_PATH: &str = "/usr/libexec/taskforest-msr-helper";

/// Parser-side sanity ceilings for the readout fields. The helper already
/// gates every decode to its documented physical envelope; these bounds only
/// reject non-physical garbage so a consumer never renders an absurd
/// temperature/ratio/voltage as if it were real telemetry.
const MAX_PLAUSIBLE_TEMPERATURE_C: f64 = 200.0;
const MAX_PLAUSIBLE_RATIO: f64 = 1000.0;
const MAX_PLAUSIBLE_MHZ: f64 = 1000.0;
const MAX_PLAUSIBLE_VOLTS: f64 = 10.0;

/// One MSR readout row parsed from a SUCCESS object's `packages` array —
/// one `/dev/cpu/N/msr` node. Fields the CPU does not implement parse as
/// `None` (JSON `null`), never a fabricated zero.
#[derive(Debug, Clone, PartialEq)]
pub struct MsrPackageReading {
    /// The numeric suffix `N` of the `/dev/cpu/N` node.
    pub cpu: u32,
    /// Base clock in MHz; `null` until a verified derivation exists (ADR-048).
    pub bclk_mhz: Option<f32>,
    /// Package temperature in °C (TjMax − package digital readout).
    pub temperature_c: Option<f32>,
    /// Current performance ratio.
    pub multiplier: Option<f32>,
    /// Maximum efficiency ratio (minimum multiplier).
    pub multiplier_min: Option<f32>,
    /// Maximum 1-core turbo ratio.
    pub multiplier_max: Option<f32>,
    /// P-state core voltage in volts.
    pub vcore_v: Option<f32>,
}

/// The typed SUCCESS payload: the per-node readout rows, sorted by CPU index.
#[derive(Debug, Clone, PartialEq)]
pub struct MsrReadoutSuccess {
    /// Contract schema version; the parser requires exactly `1`.
    pub schema: u32,
    /// The per-node readouts, sorted by node index.
    pub packages: Vec<MsrPackageReading>,
}

/// The typed error `kind` the helper emits in an ERROR object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsrHelperErrorKind {
    /// `permission_denied` — the helper still lacked read permission on the
    /// 0600 `/dev/cpu/N/msr` nodes.
    PermissionDenied,
    /// `no_msr` — no `/dev/cpu` tree on this host (msr driver not loaded).
    NoMsr,
    /// `open_failed` — the `/dev/cpu` root could not be opened for a
    /// non-permission reason.
    OpenFailed,
    /// `read_failed` — an msr node read failed for a non-permission reason.
    ReadFailed,
}

impl MsrHelperErrorKind {
    /// The lowercase contract string this variant maps to.
    #[must_use]
    pub const fn as_contract_str(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::NoMsr => "no_msr",
            Self::OpenFailed => "open_failed",
            Self::ReadFailed => "read_failed",
        }
    }

    fn from_contract_str(raw: &str) -> Option<Self> {
        Some(match raw {
            "permission_denied" => Self::PermissionDenied,
            "no_msr" => Self::NoMsr,
            "open_failed" => Self::OpenFailed,
            "read_failed" => Self::ReadFailed,
            _ => return None,
        })
    }
}

/// A typed ERROR payload the helper emitted (it ran, but produced no readout
/// for a documented reason).
#[derive(Debug, Clone, PartialEq)]
pub struct MsrHelperError {
    /// The contract `kind`, typed.
    pub kind: MsrHelperErrorKind,
    /// The helper's human-readable detail string.
    pub detail: String,
}

/// The full typed outcome of one MSR escalation attempt. Mirrors
/// [`super::RaplHelperOutcome`]: no variant fabricates a reading.
#[derive(Debug, Clone, PartialEq)]
pub enum MsrHelperOutcome {
    /// The helper ran and emitted a valid SUCCESS object.
    Success(MsrReadoutSuccess),
    /// The helper ran and emitted a valid ERROR object.
    HelperError(MsrHelperError),
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
    Success(MsrReadoutSuccess),
    HelperError(MsrHelperError),
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

/// Validate a SUCCESS-shaped object; `None` on any contract violation. Every
/// readout key must be present: an absent register is JSON `null`, never a
/// missing key.
fn parse_success(json: &Json) -> Option<MsrReadoutSuccess> {
    let schema = super::integer_field(json, "schema")?;
    if schema != 1 {
        return None;
    }
    let raw_packages = json.get("packages")?.as_array()?;
    let mut packages = Vec::with_capacity(raw_packages.len());
    for entry in raw_packages {
        packages.push(MsrPackageReading {
            cpu: super::integer_field(entry, "cpu")?,
            bclk_mhz: optional_finite_field(entry, "bclk_mhz", MAX_PLAUSIBLE_MHZ)?,
            temperature_c: optional_finite_field(
                entry,
                "temperature_c",
                MAX_PLAUSIBLE_TEMPERATURE_C,
            )?,
            multiplier: optional_finite_field(entry, "multiplier", MAX_PLAUSIBLE_RATIO)?,
            multiplier_min: optional_finite_field(entry, "multiplier_min", MAX_PLAUSIBLE_RATIO)?,
            multiplier_max: optional_finite_field(entry, "multiplier_max", MAX_PLAUSIBLE_RATIO)?,
            vcore_v: optional_finite_field(entry, "vcore_v", MAX_PLAUSIBLE_VOLTS)?,
        });
    }
    Some(MsrReadoutSuccess { schema, packages })
}

/// Read one optional readout field: the key must be present, its value is
/// either JSON `null` (honest absence) or a finite non-negative number within
/// the sanity ceiling. The outer `None` is a contract violation.
fn optional_finite_field(json: &Json, key: &str, ceiling: f64) -> Option<Option<f32>> {
    match json.get(key)? {
        Json::Null => Some(None),
        Json::Number(value) => {
            let value = *value;
            if !value.is_finite() || value < 0.0 || value > ceiling {
                return None;
            }
            Some(Some(value as f32))
        }
        _ => None,
    }
}

/// Validate an ERROR-shaped object; `None` on an unknown `kind`.
fn parse_error(json: &Json) -> Option<MsrHelperError> {
    let kind_raw = json.get("kind")?.as_str()?;
    let kind = MsrHelperErrorKind::from_contract_str(kind_raw)?;
    let detail = json.get("detail")?.as_str()?.to_owned();
    Some(MsrHelperError { kind, detail })
}

/// Side-effect-free process seam: production runs `pkexec`; tests return a
/// canned `HelperOutput` or a synthetic `io::Error`.
pub trait MsrHelperProcess {
    /// Run the privileged helper once and capture its stdout + exit status.
    fn run(&self) -> io::Result<HelperOutput>;
}

/// Production process driver: `pkexec <helper>` via `std::process::Command`.
/// Linux-only — polkit/pkexec do not exist on macOS/Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct PkexecMsrHelper;

impl PkexecMsrHelper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl MsrHelperProcess for PkexecMsrHelper {
    fn run(&self) -> io::Result<HelperOutput> {
        use std::process::{Command, Stdio};
        let mut command = Command::new("pkexec");
        command
            .arg(MSR_HELPER_PATH)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let output = super::bounded_runner::run_bounded(
            &mut command,
            super::bounded_runner::INTERACTIVE_PKEXEC_DEADLINE,
        )
        .map_err(|error| error.into_io_error("the pkexec msr helper"))?;
        Ok(HelperOutput {
            status_code: output.status_code,
            stdout: output.stdout,
        })
    }
}

/// Drive one helper invocation through `process`, then map the raw reply to a
/// typed outcome. Mapping rules mirror `invoke_rapl_helper_with`.
pub fn invoke_msr_helper_with<P: MsrHelperProcess>(process: &P) -> MsrHelperOutcome {
    let output = match process.run() {
        Ok(output) => output,
        Err(error) => {
            let detail = if error.kind() == io::ErrorKind::TimedOut {
                format!("the pkexec msr helper crossing was killed at its deadline: {error}")
            } else {
                format!("could not spawn the pkexec msr helper: {error}")
            };
            return MsrHelperOutcome::Unavailable {
                reason: EscalationDenialReason::HelperUnavailable,
                detail,
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    match parse_helper_output(&stdout) {
        ParsedOutput::Success(success) => MsrHelperOutcome::Success(success),
        ParsedOutput::HelperError(error) => MsrHelperOutcome::HelperError(error),
        ParsedOutput::NotContract => {
            let (reason, detail) = classify_pkexec_no_contract(
                output.status_code,
                msr_helper_path_for_detail(),
                &stdout,
            );
            MsrHelperOutcome::Unavailable { reason, detail }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn msr_helper_path_for_detail() -> &'static str {
    "the annotated helper path"
}

#[cfg(target_os = "linux")]
fn msr_helper_path_for_detail() -> &'static str {
    MSR_HELPER_PATH
}

/// Run the privileged helper end-to-end via the production `pkexec` driver.
/// Non-Linux hosts fail closed as `Unavailable { Unsupported }`.
#[cfg(target_os = "linux")]
pub fn invoke_msr_helper() -> MsrHelperOutcome {
    invoke_msr_helper_with(&PkexecMsrHelper::new())
}

#[cfg(not(target_os = "linux"))]
pub fn invoke_msr_helper() -> MsrHelperOutcome {
    MsrHelperOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "pkexec/polkit per-feature escalation is Linux-only".to_owned(),
    }
}

#[cfg(test)]
#[path = "../../tests/headless/escalation_polkit_msr.rs"]
mod tests;

//! Concrete escalation crossing for the Intel PMU perf helper (ADR-023,
//! permission-model Boundary 2 operationalized): the real `PolkitGate` plus the
//! `invoke_perf_helper` entry point that drives `pkexec` + the privileged helper
//! and parses its shared JSON contract into typed data.
//!
//! ## Why this lives in a sibling module, not in `lib.rs`
//!
//! `lib.rs` is the abstract SEAM — the zero-dependency boundary definition
//! (`EscalationFeature`, `PrivilegeGate`, `UnprivilegedGate`). The workspace
//! architecture test `escalation_seam_crate_exists_as_a_forbid_unsafe_leaf`
//! scans `lib.rs` and asserts the seam itself never grants or invokes a
//! capability (`pkexec` / `Command::new` must stay out of it). THIS module is
//! the operational crossing recorded by ADR-023: it is the one place that
//! launches OS-native prompts and consumes helper replies. Keeping the seam pure while
//! concentrating the crossing here preserves both the CI contract on `lib.rs`
//! and the documented invariant that the escalation crate stays pure safe Rust
//! with **zero dependencies**.
//!
//! ## Zero dependencies, including for JSON
//!
//! The crate's documented invariant is "no dependencies beyond std/core" (see
//! `lib.rs` and ADR-023). Adding `serde`/`serde_json` here would break that
//! invariant, so the helper's shared JSON contract is parsed by a small,
//! fail-closed, std-only recursive-descent parser defined privately below. Any
//! byte that is not exactly the contract is reported as a typed
//! `ParsedOutput::NotContract` — never silently coerced into a fabricated
//! engine row. This fail-closed behavior IS the honesty red line.
//!
//! ## Honesty
//!
//! Every failure path returns a typed `PerfHelperOutcome` variant. Missing
//! escalation mechanism, a refused prompt, an uninstalled helper, or an
//! unparseable reply each surface as `PerfHelperOutcome::Unavailable` with a
//! typed `EscalationDenialReason` and a host-specific detail string — never as a
//! zero-valued engine row. The real privileged `perf_event_open` read cannot be
//! exercised without sudo; this module is therefore unit-tested against fixture
//! stdout strings and a mocked process seam, and the live `pkexec` + prompt is
//! verified on-box by the integrator.

#![forbid(unsafe_code)]

use std::io;

use crate::EscalationDenialReason;

/// Std-only JSON reader for the helper contract. Kept in a sibling file so this
/// module stays under the workspace file-line budget; it is private to `polkit`.
mod json_reader;
use json_reader::{Json, JsonReader};
// The bounded pkexec runner is consumed only by the Linux drivers below.
#[cfg(target_os = "linux")]
mod bounded_runner;
#[cfg(target_os = "linux")]
use bounded_runner::{INTERACTIVE_PKEXEC_DEADLINE, run_bounded};
mod setup;
pub use setup::{
    PkexecSetupScript, SetupScriptFailure, SetupScriptOperation, SetupScriptOutcome,
    SetupScriptProcess, SetupScriptProcessOutput, invoke_setup_script, invoke_setup_script_with,
};

/// The helper program the polkit `.policy` authorizes. This is the absolute
/// install path that MUST match the `org.freedesktop.policykit.exec.path`
/// annotation in `polkit/com.taskforest.perf-helper.policy.in`: polkit matches
/// the action by the exact program path passed to `pkexec`, so a bare binary
/// name would fail to resolve the action and be denied. The integrator verifies
/// the path on-box; see `polkit/README.md`.
#[cfg(target_os = "linux")]
pub(crate) const PERF_HELPER_PATH: &str = "/usr/libexec/taskmanager-privilege-helper";

/// One GPU engine reading parsed from a SUCCESS object's `engines` array.
///
/// `busy_pct` is the helper-reported busy percent in `[0.0, 100.0]`; the parser
/// rejects any value outside that range or any non-finite value as
/// `NotContract`, so a consumer never sees a fabricated or NaN utilization.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineReading {
    /// Human-readable engine name, e.g. `"Render Ring"`.
    pub name: String,
    /// Engine class, e.g. `"rcs"` / `"vcs"` / the i915 engine class string.
    pub class: String,
    /// Busy percent in `[0.0, 100.0]`.
    pub busy_pct: f32,
}

/// The typed SUCCESS payload: the driver that owns the engines, the sample
/// window the helper measured, and the per-engine busy readings.
#[derive(Debug, Clone, PartialEq)]
pub struct PerfHelperSuccess {
    /// Contract schema version. The parser requires exactly `1`; any other
    /// value is `NotContract` (a future schema needs a deliberate migration).
    pub schema: u32,
    /// Driver family the helper read (`"xe"` or `"i915"`).
    pub driver: String,
    /// The measurement window in milliseconds the helper sampled.
    pub sample_ms: u32,
    /// The per-engine busy readings.
    pub engines: Vec<EngineReading>,
}

/// The typed error `kind` the helper emits in an ERROR object, mirroring the
/// shared JSON contract exactly. One variant per contract `kind` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerfHelperErrorKind {
    /// `permission_denied` — the helper could not acquire the PMU access it
    /// needs (e.g. restrictive `perf_event_paranoid` even when escalated).
    PermissionDenied,
    /// `no_pmu` — no Intel i915/xe PMU event is registered on this host.
    NoPmu,
    /// `open_failed` — `perf_event_open` failed for a reason other than
    /// permission (e.g. ENODEV, EINVAL).
    OpenFailed,
    /// `read_failed` — the counter fd opened but a read/reset failed.
    ReadFailed,
}

impl PerfHelperErrorKind {
    /// The lowercase contract string this variant maps to.
    #[must_use]
    pub const fn as_contract_str(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::NoPmu => "no_pmu",
            Self::OpenFailed => "open_failed",
            Self::ReadFailed => "read_failed",
        }
    }

    fn from_contract_str(raw: &str) -> Option<Self> {
        Some(match raw {
            "permission_denied" => Self::PermissionDenied,
            "no_pmu" => Self::NoPmu,
            "open_failed" => Self::OpenFailed,
            "read_failed" => Self::ReadFailed,
            _ => return None,
        })
    }
}

/// A typed ERROR payload the helper emitted (it ran, but could not produce
/// engine data for a documented reason).
#[derive(Debug, Clone, PartialEq)]
pub struct PerfHelperError {
    /// The contract `kind`, typed.
    pub kind: PerfHelperErrorKind,
    /// The helper's human-readable detail string.
    pub detail: String,
}

/// The full typed outcome of one escalation attempt.
///
/// Every variant is honest data: `Success` carries real engine
/// rows; `HelperError` carries the helper's own typed
/// reason for not producing rows; `Unavailable` carries the
/// escalation layer's typed reason it could not deliver usable data (non-Linux
/// host, pkexec/polkit missing, the user declined the prompt, or the helper
/// replied with something that is not the contract). No variant fabricates a
/// row.
#[derive(Debug, Clone, PartialEq)]
pub enum PerfHelperOutcome {
    /// The helper ran and emitted a valid SUCCESS object with engine rows.
    Success(PerfHelperSuccess),
    /// The helper ran and emitted a valid ERROR object (it could not produce
    /// engine data for a documented typed reason).
    HelperError(PerfHelperError),
    /// The escalation layer could not deliver usable data. `reason` is typed so
    /// the consumer can react without parsing `detail`.
    Unavailable {
        /// Typed reason the escalation could not deliver data.
        reason: EscalationDenialReason,
        /// Host-specific detail for logs/diagnostics.
        detail: String,
    },
}

/// What `parse_helper_output` extracted from a raw stdout blob. Internal: the
/// escalation-layer process semantics in `invoke_perf_helper_with` layer on top
/// of this to produce the public `PerfHelperOutcome`.
#[derive(Debug, Clone, PartialEq)]
enum ParsedOutput {
    /// A valid SUCCESS object.
    Success(PerfHelperSuccess),
    /// A valid ERROR object.
    HelperError(PerfHelperError),
    /// Not a recognizable contract document (malformed JSON, missing required
    /// fields, unknown schema/kind, or a value out of contract range).
    NotContract,
}

// ===========================================================================
// Contract parsing: JSON value -> typed ParsedOutput.
// ===========================================================================

/// Parse a raw helper stdout blob into one of the three contract outcomes.
///
/// Distinguishes SUCCESS from ERROR by the shared contract rule: a SUCCESS
/// object carries an `engines` array (and no `status`); an ERROR object carries
/// `status: "error"` (and no `engines`). Anything else — malformed JSON, a
/// missing/wrong-type required field, an out-of-range `busy_pct`, an unknown
/// schema or `kind` — is `ParsedOutput::NotContract`, so the caller never
/// silently coerces a non-contract reply into fake rows.
fn parse_helper_output(stdout: &str) -> ParsedOutput {
    let json = match JsonReader::parse(stdout) {
        Ok(value) => value,
        Err(()) => return ParsedOutput::NotContract,
    };
    // SUCCESS is distinguished by the presence of "engines".
    if json.get("engines").is_some() {
        return match parse_success(&json) {
            Some(success) => ParsedOutput::Success(success),
            None => ParsedOutput::NotContract,
        };
    }
    // ERROR is distinguished by status == "error".
    if json.get("status").and_then(Json::as_str) == Some("error") {
        return match parse_error(&json) {
            Some(error) => ParsedOutput::HelperError(error),
            None => ParsedOutput::NotContract,
        };
    }
    ParsedOutput::NotContract
}

/// Validate a SUCCESS-shaped object. `None` if any required field is missing,
/// wrongly typed, or out of contract range.
fn parse_success(json: &Json) -> Option<PerfHelperSuccess> {
    // schema MUST be exactly the integer 1.
    let schema = integer_field(json, "schema")?;
    if schema != 1 {
        return None;
    }
    let driver = json.get("driver")?.as_str()?;
    if driver.is_empty() {
        return None;
    }
    let sample_ms = integer_field(json, "sample_ms")?;
    let raw_engines = json.get("engines")?.as_array()?;
    let mut engines = Vec::with_capacity(raw_engines.len());
    for entry in raw_engines {
        let name = entry.get("name")?.as_str()?.to_owned();
        let class = entry.get("class")?.as_str()?.to_owned();
        let busy = entry.get("busy_pct")?;
        let Json::Number(busy_pct) = busy else {
            return None;
        };
        let busy_pct = *busy_pct;
        // Reject non-finite or out-of-range busy percents — never fabricate a
        // clamped or NaN row.
        if !busy_pct.is_finite() || !(0.0..=100.0).contains(&busy_pct) {
            return None;
        }
        engines.push(EngineReading {
            name,
            class,
            busy_pct: busy_pct as f32,
        });
    }
    Some(PerfHelperSuccess {
        schema,
        driver: driver.to_owned(),
        sample_ms,
        engines,
    })
}

/// Validate an ERROR-shaped object. `None` if `kind` is unknown or `detail` is
/// not a string.
fn parse_error(json: &Json) -> Option<PerfHelperError> {
    let kind_raw = json.get("kind")?.as_str()?;
    let kind = PerfHelperErrorKind::from_contract_str(kind_raw)?;
    let detail = json.get("detail")?.as_str()?.to_owned();
    Some(PerfHelperError { kind, detail })
}

/// Read a JSON number field as a non-negative integer fitting `u32`. Returns
/// `None` if the field is absent, not a number, has a fractional part, or is
/// outside `[0, u32::MAX]`.
fn integer_field(json: &Json, key: &str) -> Option<u32> {
    let Json::Number(value) = json.get(key)? else {
        return None;
    };
    if !value.is_finite() || value.fract() != 0.0 || *value < 0.0 {
        return None;
    }
    Some(*value as u32)
}

// ===========================================================================
// Process seam: lets tests inject a canned reply so no real pkexec runs.
// ===========================================================================

/// The captured reply from one helper invocation. Carrying `status_code` as a
/// plain `Option<i32>` (rather than `std::process::ExitStatus`) keeps this
/// constructible in tests on every platform without `ExitStatusExt`.
pub struct HelperOutput {
    /// The process exit code, or `None` if the platform did not report one
    /// (e.g. terminated by signal).
    pub status_code: Option<i32>,
    /// The helper's raw stdout.
    pub stdout: Vec<u8>,
}

/// Side-effect-free process seam: production runs `pkexec`; tests return a
/// canned `HelperOutput` or a synthetic `io::Error`.
pub trait PerfHelperProcess {
    /// Run the privileged helper once and capture its stdout + exit status.
    fn run(&self) -> io::Result<HelperOutput>;
}

/// Production process driver: `pkexec <helper>` via `std::process::Command`.
/// Linux-only — polkit/pkexec do not exist on macOS/Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct PkexecPerfHelper;

impl PkexecPerfHelper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl PerfHelperProcess for PkexecPerfHelper {
    fn run(&self) -> io::Result<HelperOutput> {
        use std::process::{Command, Stdio};
        let mut command = Command::new("pkexec");
        command
            .arg(PERF_HELPER_PATH)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            // The contract travels on stdout; keep stderr out of the captured
            // payload so a noisy polkit/pkexec diagnostic cannot corrupt it.
            .stderr(Stdio::null());
        // Bounded run: an abandoned polkit dialog cannot park this thread
        // forever, and a runaway stdout cannot balloon memory — it is capped
        // and then flows through the NotContract semantics downstream.
        let output = run_bounded(&mut command, INTERACTIVE_PKEXEC_DEADLINE)
            .map_err(|error| error.into_io_error("the pkexec perf helper"))?;
        Ok(HelperOutput {
            status_code: output.status_code,
            stdout: output.stdout,
        })
    }
}

/// Drive one helper invocation through `process`, then map the raw reply to a
/// typed `PerfHelperOutcome`.
///
/// Mapping rules (all honest — no fabricated row):
/// * a spawn error (`pkexec` not on `PATH`, etc.) → `Unavailable { HelperUnavailable }`;
/// * a deadline kill from the bounded runner (`ErrorKind::TimedOut`) →
///   `Unavailable { HelperUnavailable }` with a detail naming the abandoned
///   crossing — the helper never delivered a contract message;
/// * a valid SUCCESS object → `PerfHelperOutcome::Success`;
/// * a valid ERROR object → `PerfHelperOutcome::HelperError` (honoring the
///   helper's own typed `kind`);
/// * otherwise the helper emitted no valid contract message: exit 126 is the
///   user-refusal outcome, 127 is an authorization-service failure, and every
///   other status is a helper protocol violation.
pub fn invoke_perf_helper_with<P: PerfHelperProcess>(process: &P) -> PerfHelperOutcome {
    let output = match process.run() {
        Ok(output) => output,
        Err(error) => {
            let detail = if error.kind() == io::ErrorKind::TimedOut {
                format!("the pkexec perf helper crossing was killed at its deadline: {error}")
            } else {
                format!("could not spawn the pkexec perf helper: {error}")
            };
            return PerfHelperOutcome::Unavailable {
                reason: EscalationDenialReason::HelperUnavailable,
                detail,
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    match parse_helper_output(&stdout) {
        ParsedOutput::Success(success) => PerfHelperOutcome::Success(success),
        ParsedOutput::HelperError(error) => PerfHelperOutcome::HelperError(error),
        ParsedOutput::NotContract => {
            let (reason, detail) = classify_pkexec_no_contract(
                output.status_code,
                perf_helper_path_for_detail(),
                &stdout,
            );
            PerfHelperOutcome::Unavailable { reason, detail }
        }
    }
}

fn classify_pkexec_no_contract(
    status_code: Option<i32>,
    helper_path: &str,
    stdout: &str,
) -> (EscalationDenialReason, String) {
    match status_code {
        Some(126) => (
            EscalationDenialReason::PermissionDenied,
            "the authorization prompt was dismissed or refused (pkexec exit 126)".to_owned(),
        ),
        Some(127) => (
            EscalationDenialReason::AuthorizationUnavailable,
            format!(
                "the authorization service could not authorize {helper_path} (pkexec exit 127)"
            ),
        ),
        code => (
            EscalationDenialReason::HelperProtocolViolation,
            format!(
                "helper at {helper_path} produced no valid contract message (exit {code:?}): {}",
                truncate_for_detail(stdout),
            ),
        ),
    }
}

#[cfg(not(target_os = "linux"))]
fn perf_helper_path_for_detail() -> &'static str {
    "the annotated helper path"
}

#[cfg(target_os = "linux")]
fn perf_helper_path_for_detail() -> &'static str {
    PERF_HELPER_PATH
}

/// Run the privileged helper end-to-end via the production `pkexec` driver.
///
/// On non-Linux hosts polkit/pkexec do not exist, so this returns
/// `Unavailable { Unsupported }` without attempting to spawn anything — the
/// feature is honestly unreachable off-Linux.
#[cfg(target_os = "linux")]
pub fn invoke_perf_helper() -> PerfHelperOutcome {
    invoke_perf_helper_with(&PkexecPerfHelper::new())
}

#[cfg(not(target_os = "linux"))]
pub fn invoke_perf_helper() -> PerfHelperOutcome {
    PerfHelperOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "pkexec/polkit per-feature escalation is Linux-only".to_owned(),
    }
}

// ---------------------------------------------------------------------------
// AF_PACKET net-launcher invocation (ADR-024/025) — isolated in its own
// `net_launcher` module so this file stays under the workspace file-line budget
// and the net-launcher crossing (abstraction + driver + Linux impl) is in one
// place. Re-exported here to keep the public path stable.
// ---------------------------------------------------------------------------
mod net_launcher;
pub use net_launcher::{
    NetLaunchHandle, NetLauncherOutcome, NetLauncherProcess, invoke_net_launcher,
    invoke_net_launcher_with,
};
// PkexecNetLauncher (the Linux pkexec driver) is Linux-only — re-export it only
// where it exists. NET_LAUNCHER_PATH stays private to `net_launcher`.
#[cfg(target_os = "linux")]
pub use net_launcher::PkexecNetLauncher;

// `pub(crate)`: the helper-contract reader inside (`parse_output`) is the one
// authority for "what is a valid process-control helper contract message" —
// the Windows UAC reply channel (`crate::uac`) classifies by the same rule
// (ADR-035), instead of growing a second parser.
pub(crate) mod process_control;
pub use process_control::{
    ForeignProcessControlFailure, ForeignProcessControlOperation, ForeignProcessControlOutcome,
    ForeignProcessControlProcess, ForeignProcessControlTarget, ForeignProcessSignal,
    PkexecForeignProcessControl, invoke_foreign_process_control,
    invoke_foreign_process_control_with,
};

mod gate;
pub use gate::PolkitGate;

/// Cap a non-contract stdout in a detail string so a giant/garbage blob does
/// not balloon the diagnostic. Cuts at the newest UTF-8 char boundary at or
/// before `LIMIT` bytes so a multibyte helper stdout can never panic the
/// slice (defect: byte 160 used to land mid-character). `pub(crate)` so the
/// sibling `uac` transport reuses the same bounded detail rule for reply
/// payloads.
pub(crate) fn truncate_for_detail(text: &str) -> String {
    const LIMIT: usize = 160;
    let text = text.trim();
    if text.len() <= LIMIT {
        text.to_owned()
    } else {
        let mut cut = truncate_at_char_boundary(text, LIMIT);
        cut.push('…');
        cut
    }
}

/// Cut `text` at the newest char boundary at or before `limit_bytes`. The
/// shared char-boundary fallback used by every bounded detail string in the
/// polkit module tree (this file and `polkit/setup.rs`).
pub(super) fn truncate_at_char_boundary(text: &str, limit_bytes: usize) -> String {
    if text.len() <= limit_bytes {
        return text.to_owned();
    }
    let mut end = limit_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
#[path = "../tests/headless/escalation_polkit.rs"]
mod tests;

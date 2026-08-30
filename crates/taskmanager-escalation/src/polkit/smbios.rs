//! Escalation crossing for the SMBIOS memory helper (ADR-023, Boundary 2):
//! the `pkexec` driver + the std-only contract parser for the helper's JSON.
//!
//! Mirrors the perf-helper crossing in the parent module: a thin process seam
//! (so tests inject canned stdout and never run a real `pkexec`), a
//! fail-closed parser that turns every non-contract reply into a typed
//! [`EscalationDenialReason`], and one public entry point
//! ([`invoke_smbios_helper`]) that the CLI / request lane call when the user
//! escalates the `MemorySmbios` feature.
//!
//! Shared JSON contract (must match `taskmanager-smbios-helper` exactly):
//! ```text
//! SUCCESS: {"schema":1,"slots_total":<u32>,"slots_used":<u32>,
//!           "modules":[{"slot":<u32>,"size_mb":<u32|null>,"speed_mts":<u32|null>,
//!                       "configured_speed_mts":<u32|null>,"manufacturer":<str|null>,
//!                       "serial_number":<str|null>,"part_number":<str|null>,
//!                       "form_factor":<str|null>,"memory_type":<str|null>,
//!                       "locator":<str|null>}],
//!           "identity":{"bios_vendor":<str|null>,"bios_version":<str|null>,
//!                       "bios_date":<str|null>,"board_manufacturer":<str|null>,
//!                       "board_product":<str|null>,"board_serial":<str|null>,
//!                       "board_asset_tag":<str|null>,
//!                       "system_manufacturer":<str|null>,
//!                       "system_product":<str|null>,"system_serial":<str|null>,
//!                       "system_uuid":<str|null>,"system_sku":<str|null>,
//!                       "system_family":<str|null>}|null}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_dmi"|"open_failed"|"read_failed",
//!           "detail":"<string>"}
//! ```
//! A SUCCESS object has NO `status`; an ERROR object has NO `modules`. A
//! `null` field is the honest "fact absent on this host" — never a zero. The
//! whole `identity` object is `null`/absent when the host carries no
//! type-0/1/2 DMI entries (a helper predating the field omits the key — also
//! `None`); a present object with a `null` field keeps that field `None`.

#![forbid(unsafe_code)]

use std::io;

use crate::EscalationDenialReason;

use super::json_reader::{Json, JsonReader};
use super::{HelperOutput, classify_pkexec_no_contract};

/// The helper program the polkit `.policy` authorizes; must equal the
/// `org.freedesktop.policykit.exec.path` annotation in
/// `polkit/io.github.YellowWhiteBlackCat.TaskForest.smbios-helper.policy.in`
/// (polkit resolves the action by the exact program path passed to `pkexec`).
#[cfg(target_os = "linux")]
pub(crate) const SMBIOS_HELPER_PATH: &str = "/usr/libexec/taskforest-smbios-helper";

/// One populated memory-module row parsed from a SUCCESS object's `modules`
/// array. Every optional fact is `None` when the helper reported `null`.
#[derive(Debug, Clone, PartialEq)]
pub struct SmbiosModuleReading {
    /// SMBIOS slot index (the `17-N` entry suffix the helper read).
    pub slot: u32,
    /// Module capacity in MB.
    pub size_mb: Option<u32>,
    /// Maximum speed in MT/s.
    pub speed_mts: Option<u32>,
    /// Currently configured speed in MT/s (the "live" speed CPU-X shows).
    pub configured_speed_mts: Option<u32>,
    /// Manufacturer label, e.g. `"Samsung"`.
    pub manufacturer: Option<String>,
    /// Module serial number.
    pub serial_number: Option<String>,
    /// Module part number, e.g. `"M471A2G43CB2-..."`.
    pub part_number: Option<String>,
    /// Form factor label, e.g. `"SODIMM"`.
    pub form_factor: Option<String>,
    /// Memory type label, e.g. `"DDR5"`.
    pub memory_type: Option<String>,
    /// Device locator string, e.g. `"ChannelA-DIMM0"`.
    pub locator: Option<String>,
}

/// The typed SUCCESS payload: slot inventory plus the populated-module rows.
#[derive(Debug, Clone, PartialEq)]
pub struct SmbiosMemorySuccess {
    /// Contract schema version; the parser requires exactly `1`.
    pub schema: u32,
    /// Total type-17 records seen (populated and empty slots).
    pub slots_total: u32,
    /// Records reporting a populated module.
    pub slots_used: u32,
    /// The populated-module rows, sorted by slot.
    pub modules: Vec<SmbiosModuleReading>,
    /// System/board identity facts (types 0/1/2). `None` when the helper
    /// reported a `null`/missing `identity` (no DMI identity entries on this
    /// host, or a helper predating the additive field).
    pub identity: Option<DmiIdentityFacts>,
}

/// The system/board identity facts parsed from a SUCCESS object's `identity`
/// section. Every field is `None` when the helper reported `null` — never a
/// fabricated zero or empty string. The lane's core-facing twin
/// (`taskmanager-core::DmiIdentityFacts`) is mapped from this field-by-field
/// at the provider crossing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DmiIdentityFacts {
    /// BIOS vendor string (type 0).
    pub bios_vendor: Option<String>,
    /// BIOS version string (type 0).
    pub bios_version: Option<String>,
    /// BIOS release date string (type 0).
    pub bios_date: Option<String>,
    /// Board manufacturer string (type 2).
    pub board_manufacturer: Option<String>,
    /// Board product name string (type 2).
    pub board_product: Option<String>,
    /// Board serial number string (type 2).
    pub board_serial: Option<String>,
    /// Board asset tag string (type 2).
    pub board_asset_tag: Option<String>,
    /// System manufacturer string (type 1).
    pub system_manufacturer: Option<String>,
    /// System product name string (type 1).
    pub system_product: Option<String>,
    /// System serial number string (type 1).
    pub system_serial: Option<String>,
    /// System UUID, canonical hyphenated lowercase (type 1).
    pub system_uuid: Option<String>,
    /// System SKU number string (type 1).
    pub system_sku: Option<String>,
    /// System family string (type 1).
    pub system_family: Option<String>,
}

/// The typed error `kind` the helper emits in an ERROR object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbiosHelperErrorKind {
    /// `permission_denied` — the helper still lacked read permission
    /// (e.g. invoked without the escalation actually granted).
    PermissionDenied,
    /// `no_dmi` — no `/sys/firmware/dmi/entries` tree on this host.
    NoDmi,
    /// `open_failed` — the entries directory could not be opened for a
    /// non-permission reason.
    OpenFailed,
    /// `read_failed` — a `17-*/raw` record read failed for a non-permission
    /// reason.
    ReadFailed,
}

impl SmbiosHelperErrorKind {
    /// The lowercase contract string this variant maps to.
    #[must_use]
    pub const fn as_contract_str(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::NoDmi => "no_dmi",
            Self::OpenFailed => "open_failed",
            Self::ReadFailed => "read_failed",
        }
    }

    fn from_contract_str(raw: &str) -> Option<Self> {
        Some(match raw {
            "permission_denied" => Self::PermissionDenied,
            "no_dmi" => Self::NoDmi,
            "open_failed" => Self::OpenFailed,
            "read_failed" => Self::ReadFailed,
            _ => return None,
        })
    }
}

/// A typed ERROR payload the helper emitted (it ran, but produced no memory
/// inventory for a documented reason).
#[derive(Debug, Clone, PartialEq)]
pub struct SmbiosHelperError {
    /// The contract `kind`, typed.
    pub kind: SmbiosHelperErrorKind,
    /// The helper's human-readable detail string.
    pub detail: String,
}

/// The full typed outcome of one SMBIOS escalation attempt. Mirrors
/// [`super::PerfHelperOutcome`]: no variant fabricates a module row. The
/// success payload is boxed to keep the variants close in size.
#[derive(Debug, Clone, PartialEq)]
pub enum SmbiosHelperOutcome {
    /// The helper ran and emitted a valid SUCCESS object.
    Success(Box<SmbiosMemorySuccess>),
    /// The helper ran and emitted a valid ERROR object.
    HelperError(SmbiosHelperError),
    /// The escalation layer could not deliver usable data (typed reason).
    Unavailable {
        /// Typed reason the escalation could not deliver data.
        reason: EscalationDenialReason,
        /// Host-specific detail for logs/diagnostics.
        detail: String,
    },
}

/// What `parse_helper_output` extracted from a raw stdout blob. The success
/// payload is boxed to keep the variants close in size.
#[derive(Debug, Clone, PartialEq)]
enum ParsedOutput {
    Success(Box<SmbiosMemorySuccess>),
    HelperError(SmbiosHelperError),
    NotContract,
}

/// Parse a raw helper stdout blob into one of the three contract outcomes.
/// SUCCESS is distinguished by the presence of `modules`; ERROR by
/// `status == "error"`. Anything else is `NotContract`.
fn parse_helper_output(stdout: &str) -> ParsedOutput {
    let json = match JsonReader::parse(stdout) {
        Ok(value) => value,
        Err(()) => return ParsedOutput::NotContract,
    };
    if json.get("modules").is_some() {
        return match parse_success(&json) {
            Some(success) => ParsedOutput::Success(Box::new(success)),
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
fn parse_success(json: &Json) -> Option<SmbiosMemorySuccess> {
    let schema = super::integer_field(json, "schema")?;
    if schema != 1 {
        return None;
    }
    let slots_total = super::integer_field(json, "slots_total")?;
    let slots_used = super::integer_field(json, "slots_used")?;
    let raw_modules = json.get("modules")?.as_array()?;
    let mut modules = Vec::with_capacity(raw_modules.len());
    for entry in raw_modules {
        modules.push(SmbiosModuleReading {
            slot: super::integer_field(entry, "slot")?,
            size_mb: optional_u32(entry, "size_mb")?,
            speed_mts: optional_u32(entry, "speed_mts")?,
            configured_speed_mts: optional_u32(entry, "configured_speed_mts")?,
            manufacturer: optional_string(entry, "manufacturer")?,
            serial_number: optional_string(entry, "serial_number")?,
            part_number: optional_string(entry, "part_number")?,
            form_factor: optional_string(entry, "form_factor")?,
            memory_type: optional_string(entry, "memory_type")?,
            locator: optional_string(entry, "locator")?,
        });
    }
    let identity = optional_identity(json)?;
    Some(SmbiosMemorySuccess {
        schema,
        slots_total,
        slots_used,
        modules,
        identity,
    })
}

/// Validate an ERROR-shaped object; `None` on an unknown `kind`.
fn parse_error(json: &Json) -> Option<SmbiosHelperError> {
    let kind_raw = json.get("kind")?.as_str()?;
    let kind = SmbiosHelperErrorKind::from_contract_str(kind_raw)?;
    let detail = json.get("detail")?.as_str()?.to_owned();
    Some(SmbiosHelperError { kind, detail })
}

/// A contract u32 field that may be `null` (honest absence). A missing key or
/// a non-integer / out-of-range number violates the contract.
fn optional_u32(json: &Json, key: &str) -> Option<Option<u32>> {
    match json.get(key)? {
        Json::Null => Some(None),
        Json::Number(value)
            if value.is_finite()
                && value.fract() == 0.0
                && *value >= 0.0
                && *value <= u32::MAX as f64 =>
        {
            Some(Some(*value as u32))
        }
        _ => None,
    }
}

/// A contract string field that may be `null` (honest absence).
fn optional_string(json: &Json, key: &str) -> Option<Option<String>> {
    match json.get(key) {
        Some(Json::Null) => Some(None),
        Some(Json::String(value)) => Some(Some(value.clone())),
        _ => None,
    }
}

/// The SUCCESS object's `identity` section. The field is additive under
/// schema 1: a missing key (a helper predating it) and an explicit `null`
/// (no type-0/1/2 entries on this host) are both `None`. A present object
/// must carry all 13 contract fields, each string-or-null ([`optional_string`]
/// turns a missing key or wrong type into a contract violation); any
/// non-object shape is likewise not the contract.
fn optional_identity(json: &Json) -> Option<Option<DmiIdentityFacts>> {
    let identity = match json.get("identity") {
        None | Some(Json::Null) => return Some(None),
        Some(identity @ Json::Object(_)) => identity,
        _ => return None,
    };
    Some(Some(DmiIdentityFacts {
        bios_vendor: optional_string(identity, "bios_vendor")?,
        bios_version: optional_string(identity, "bios_version")?,
        bios_date: optional_string(identity, "bios_date")?,
        board_manufacturer: optional_string(identity, "board_manufacturer")?,
        board_product: optional_string(identity, "board_product")?,
        board_serial: optional_string(identity, "board_serial")?,
        board_asset_tag: optional_string(identity, "board_asset_tag")?,
        system_manufacturer: optional_string(identity, "system_manufacturer")?,
        system_product: optional_string(identity, "system_product")?,
        system_serial: optional_string(identity, "system_serial")?,
        system_uuid: optional_string(identity, "system_uuid")?,
        system_sku: optional_string(identity, "system_sku")?,
        system_family: optional_string(identity, "system_family")?,
    }))
}

/// Side-effect-free process seam: production runs `pkexec`; tests return a
/// canned `HelperOutput` or a synthetic `io::Error`.
pub trait SmbiosHelperProcess {
    /// Run the privileged helper once and capture its stdout + exit status.
    fn run(&self) -> io::Result<HelperOutput>;
}

/// Production process driver: `pkexec <helper>` via `std::process::Command`.
/// Linux-only — polkit/pkexec do not exist on macOS/Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct PkexecSmbiosHelper;

impl PkexecSmbiosHelper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl SmbiosHelperProcess for PkexecSmbiosHelper {
    fn run(&self) -> io::Result<HelperOutput> {
        use std::process::{Command, Stdio};
        let mut command = Command::new("pkexec");
        command
            .arg(SMBIOS_HELPER_PATH)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let output = super::bounded_runner::run_bounded(
            &mut command,
            super::bounded_runner::INTERACTIVE_PKEXEC_DEADLINE,
        )
        .map_err(|error| error.into_io_error("the pkexec smbios helper"))?;
        Ok(HelperOutput {
            status_code: output.status_code,
            stdout: output.stdout,
        })
    }
}

/// Drive one helper invocation through `process`, then map the raw reply to a
/// typed outcome. Mapping rules mirror `invoke_perf_helper_with`: spawn
/// failure or deadline → `Unavailable { HelperUnavailable }`; valid SUCCESS →
/// `Success`; valid ERROR → `HelperError`; no contract message → the pkexec
/// exit-code classification (126 refusal / 127 authorization / violation).
pub fn invoke_smbios_helper_with<P: SmbiosHelperProcess>(process: &P) -> SmbiosHelperOutcome {
    let output = match process.run() {
        Ok(output) => output,
        Err(error) => {
            let detail = if error.kind() == io::ErrorKind::TimedOut {
                format!("the pkexec smbios helper crossing was killed at its deadline: {error}")
            } else {
                format!("could not spawn the pkexec smbios helper: {error}")
            };
            return SmbiosHelperOutcome::Unavailable {
                reason: EscalationDenialReason::HelperUnavailable,
                detail,
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    match parse_helper_output(&stdout) {
        ParsedOutput::Success(success) => SmbiosHelperOutcome::Success(success),
        ParsedOutput::HelperError(error) => SmbiosHelperOutcome::HelperError(error),
        ParsedOutput::NotContract => {
            let (reason, detail) = classify_pkexec_no_contract(
                output.status_code,
                smbios_helper_path_for_detail(),
                &stdout,
            );
            SmbiosHelperOutcome::Unavailable { reason, detail }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn smbios_helper_path_for_detail() -> &'static str {
    "the annotated helper path"
}

#[cfg(target_os = "linux")]
fn smbios_helper_path_for_detail() -> &'static str {
    SMBIOS_HELPER_PATH
}

/// Run the privileged helper end-to-end via the production `pkexec` driver.
/// Non-Linux hosts fail closed as `Unavailable { Unsupported }`.
#[cfg(target_os = "linux")]
pub fn invoke_smbios_helper() -> SmbiosHelperOutcome {
    invoke_smbios_helper_with(&PkexecSmbiosHelper::new())
}

#[cfg(not(target_os = "linux"))]
pub fn invoke_smbios_helper() -> SmbiosHelperOutcome {
    SmbiosHelperOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "pkexec/polkit per-feature escalation is Linux-only".to_owned(),
    }
}

#[cfg(test)]
#[path = "../../tests/headless/escalation_polkit_smbios.rs"]
mod tests;

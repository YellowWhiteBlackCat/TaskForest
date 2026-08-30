//! `taskmanager-smbios-helper` — one feature-specific privileged piece of
//! TaskForest.
//!
//! Invoked through the OS-native escalation prompt (polkit + `pkexec` on
//! Linux, per ADR-023 and `docs/PERMISSION_MODEL.md` Boundary 2), it performs
//! exactly ONE operation: a walk of `/sys/firmware/dmi/entries/17-*/raw` plus
//! the first `0-*`/`1-*`/`2-*` entries, decoded through the ONE SMBIOS format
//! authority (`taskmanager-smbios-tables`). It writes ONE JSON object to
//! stdout and exits. There are no flags and no file access beyond that
//! entries directory — minimal privileged attack surface.
//!
//! Honesty red line: a missing DMI tree, a permission denial, or any
//! open/read failure emits a typed ERROR envelope and a non-zero exit. The
//! helper NEVER emits a success object with fabricated or zeroed module
//! fields, never invents slots that the firmware did not describe, and never
//! invents serials/UUIDs the identity tables did not state.
//!
//! Shared JSON contract (the consumer keys SUCCESS off the presence of
//! `"modules"`):
//! ```text
//! SUCCESS: {"schema":1,"slots_total":<u32>,"slots_used":<u32>,
//!           "modules":[{"slot":<u32>,"size_mb":<u32>,"speed_mts":<u32>,
//!                       "configured_speed_mts":<u32>,"manufacturer":<str>,
//!                       "serial_number":<str>,"part_number":<str>,
//!                       "form_factor":<str>,"memory_type":<str>,"locator":<str>}],
//!           "identity":{...system/board facts, each <str|null>}|null}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_dmi"|"open_failed"|"read_failed",
//!           "detail":"<string>"}
//! ```
//! A SUCCESS object has NO `status`; an ERROR object has NO `modules`.

#![forbid(unsafe_code)]

// This binary is the Linux half of the ADR-023 escalation chain (pkexec +
// /sys/firmware/dmi/entries reads) and has no non-Linux build; the stub main
// keeps the workspace compiling on Windows/macOS.
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "taskmanager-smbios-helper is the Linux pkexec/SMBIOS helper; \
         there is no build of it on this platform"
    );
}

#[cfg(target_os = "linux")]
mod dmi_walk;
#[cfg(target_os = "linux")]
mod json;

#[cfg(target_os = "linux")]
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::ExitCode;

#[cfg(target_os = "linux")]
use dmi_walk::{WalkOutcome, collect_dmi_facts};
#[cfg(target_os = "linux")]
use json::{
    DmiIdentityJson, ErrorEnvelope, ErrorKindJson, MemoryModuleJson, SCHEMA_VERSION,
    SuccessEnvelope,
};

/// The ONLY path this helper reads: the kernel-exported SMBIOS entry set.
#[cfg(target_os = "linux")]
const DMI_ENTRIES_ROOT: &str = "/sys/firmware/dmi/entries";

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    emit(run())
}

/// The single pass: walk the DMI entries for Memory Device records and the
/// type-0/1/2 identity records, and produce either a success payload (slot
/// counts + populated modules + identity) or a typed failure.
#[cfg(target_os = "linux")]
fn run() -> Outcome {
    match collect_dmi_facts(Path::new(DMI_ENTRIES_ROOT)) {
        WalkOutcome::Success {
            modules,
            slots_total,
            slots_used,
            identity,
        } => Outcome::Success {
            modules,
            slots_total,
            slots_used,
            identity,
        },
        WalkOutcome::Error(error) => Outcome::Error(error.kind, error.detail),
    }
}

/// The terminal result of [`run`]: a success payload to serialize, or a typed
/// error kind + detail. The identity object stays boxed (as the walk returns
/// it) to keep the variants close in size.
#[cfg(target_os = "linux")]
enum Outcome {
    Success {
        modules: Vec<MemoryModuleJson>,
        slots_total: u32,
        slots_used: u32,
        identity: Option<Box<DmiIdentityJson>>,
    },
    Error(ErrorKindJson, String),
}

/// Serialize the outcome to stdout (flushed), and choose the process exit
/// code. Any serialization or stdout failure is itself an honest ERROR
/// (open_failed re-purposed as the closest "could not produce output" kind)
/// rather than a silent success.
#[cfg(target_os = "linux")]
fn emit(outcome: Outcome) -> ExitCode {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let (json_string, kind) = match outcome {
        Outcome::Success {
            modules,
            slots_total,
            slots_used,
            identity,
        } => {
            let envelope = SuccessEnvelope {
                schema: SCHEMA_VERSION,
                slots_total,
                slots_used,
                modules,
                identity: identity.map(|boxed| *boxed),
            };
            match serde_json::to_string(&envelope) {
                Ok(json) => (json, None),
                Err(error) => serialize_error_json(&error.to_string()),
            }
        }
        Outcome::Error(kind, detail) => {
            let envelope = ErrorEnvelope {
                status: "error",
                kind,
                detail,
            };
            match serde_json::to_string(&envelope) {
                Ok(json) => (json, Some(kind)),
                Err(error) => serialize_error_json(&error.to_string()),
            }
        }
    };
    // One JSON object then a newline; flush so the consumer sees it before exit.
    let write_result = writeln!(handle, "{json_string}").and_then(|()| handle.flush());
    let exit_kind = kind.or_else(|| {
        write_result.err().map(|error| {
            // stdout I/O failure: surface honestly rather than exit 0.
            let _ = writeln!(io::stderr(), "smbios-helper: stdout write failed: {error}");
            ErrorKindJson::OpenFailed
        })
    });
    match exit_kind {
        Some(kind) => ExitCode::from(kind.exit_code().clamp(1, 255) as u8),
        None => ExitCode::SUCCESS,
    }
}

/// Build the fallback ERROR envelope JSON for a serialization failure (and the
/// kind to exit with). Hand-rolled because the failure IS the serializer, so
/// the envelope cannot itself go through serde_json. Quotes/backslashes in
/// the detail are escaped so the result stays valid JSON.
#[cfg(target_os = "linux")]
fn serialize_error_json(detail: &str) -> (String, Option<ErrorKindJson>) {
    let escaped = detail.replace('\\', "\\\\").replace('"', "\\\"");
    (
        format!(r#"{{"status":"error","kind":"open_failed","detail":"{escaped}"}}"#),
        Some(ErrorKindJson::OpenFailed),
    )
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../tests/headless/smbios_helper_main.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;

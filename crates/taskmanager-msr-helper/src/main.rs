//! `taskmanager-msr-helper` — one feature-specific privileged piece of
//! TaskForest (ADR-023/048).
//!
//! Invoked through the OS-native escalation prompt (polkit + `pkexec` on
//! Linux), it performs exactly ONE operation: an enumeration of the existing
//! `/dev/cpu/N/msr` nodes, `pread`ing the verified register set — the five
//! Intel registers, or the AMD P-state block of ADR-049 when the CPUID
//! family gate identifies a family 0x17–0x19 CPU — plus the three CPUID
//! identity leaves from the first node's read-only `cpuid` sibling (see
//! `msr_read`) and decoding them through a pure safe layer. It writes ONE
//! JSON object to stdout and exits. There are no flags, no ioctl, no unsafe
//! code and no file access beyond those `/dev/cpu` nodes — MSR and CPUID
//! reads on Linux are plain file I/O, so the four audited boundary crates
//! stay the only unsafe trust roots (ADR-048's core argument).
//!
//! Honesty red line: a missing `/dev/cpu` tree, a permission denial, or any
//! open/read failure emits a typed ERROR envelope and a non-zero exit. A
//! register the CPU does not implement, or a value that fails its documented
//! plausibility range, becomes a `null` field — the helper NEVER emits a
//! fabricated zero, and never invents readings the silicon did not report.
//!
//! Shared JSON contract (the consumer keys SUCCESS off the presence of
//! `"packages"`):
//! ```text
//! SUCCESS: {"schema":1,"packages":[{"cpu":<u32>,"bclk_mhz":<f32>|null,
//!           "temperature_c":<f32>|null,"multiplier":<f32>|null,
//!           "multiplier_min":<f32>|null,"multiplier_max":<f32>|null,
//!           "vcore_v":<f32>|null}]}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_msr"|"open_failed"|"read_failed",
//!           "detail":"<string>"}
//! ```
//! A SUCCESS object has NO `status`; an ERROR object has NO `packages`.

#![forbid(unsafe_code)]

// This binary is the Linux half of the ADR-023/048 escalation chain (pkexec +
// /dev/cpu/*/msr reads) and has no non-Linux build; the stub main keeps the
// workspace compiling on Windows/macOS.
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "taskmanager-msr-helper is the Linux pkexec/MSR helper; \
         there is no build of it on this platform"
    );
}

#[cfg(target_os = "linux")]
mod json;
#[cfg(target_os = "linux")]
mod msr_read;

#[cfg(target_os = "linux")]
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::ExitCode;

#[cfg(target_os = "linux")]
use json::{ErrorEnvelope, ErrorKindJson, PackageReadingJson, SCHEMA_VERSION, SuccessEnvelope};
#[cfg(target_os = "linux")]
use msr_read::{ReadOutcome, collect_msr_readings};

/// The ONLY path root this helper reads: the kernel's per-CPU character
/// nodes — the MSR registers (`/dev/cpu/N/msr`) and the read-only CPUID
/// leaves (`/dev/cpu/N/cpuid`), both mode 0600 root-only.
#[cfg(target_os = "linux")]
const DEV_CPU_ROOT: &str = "/dev/cpu";

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    emit(run())
}

/// The single pass: read the verified MSR registers of every existing
/// `/dev/cpu/N/msr` node and produce either the per-node readout list or a
/// typed failure.
#[cfg(target_os = "linux")]
fn run() -> Outcome {
    match collect_msr_readings(Path::new(DEV_CPU_ROOT)) {
        ReadOutcome::Packages { packages } => Outcome::Success { packages },
        ReadOutcome::Error(error) => Outcome::Error(error.kind, error.detail),
    }
}

/// The terminal result of [`run`]: a success payload to serialize, or a typed
/// error kind + detail.
#[cfg(target_os = "linux")]
enum Outcome {
    Success { packages: Vec<PackageReadingJson> },
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
        Outcome::Success { packages } => {
            let envelope = SuccessEnvelope {
                schema: SCHEMA_VERSION,
                packages,
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
            let _ = writeln!(io::stderr(), "msr-helper: stdout write failed: {error}");
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
#[path = "../tests/headless/msr_helper_main.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;

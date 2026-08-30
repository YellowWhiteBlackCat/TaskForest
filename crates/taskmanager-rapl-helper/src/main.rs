//! `taskmanager-rapl-helper` — one feature-specific privileged piece of
//! TaskForest.
//!
//! Invoked through the OS-native escalation prompt (polkit + `pkexec` on
//! Linux, per ADR-023 and `docs/PERMISSION_MODEL.md` Boundary 2), it performs
//! exactly ONE operation: a two-sample read of
//! `/sys/class/powercap/intel-rapl:*` package energy counters (`name`,
//! `max_energy_range_uj`, `energy_uj` twice, 1000 ms apart), reduced to
//! per-package watts with wraparound handling. It writes ONE JSON object to
//! stdout and exits. There are no flags and no file access beyond that
//! powercap tree — minimal privileged attack surface (`energy_uj` is 0400
//! root-owned, which is exactly the unprivileged gap this helper fills).
//!
//! Honesty red line: a missing RAPL tree, a permission denial, or any
//! open/read failure emits a typed ERROR envelope and a non-zero exit. The
//! helper NEVER emits a success object with fabricated or zeroed power
//! figures, and drops any package whose energy delta is unknowable.
//!
//! Shared JSON contract (the consumer keys SUCCESS off the presence of
//! `"packages"`):
//! ```text
//! SUCCESS: {"schema":1,"sample_ms":<u32>,
//!           "packages":[{"name":"<string>","power_w":<f32 finite >=0.0>,
//!                        "energy_delta_uj":<u64>}]}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_rapl"|"open_failed"|"read_failed",
//!           "detail":"<string>"}
//! ```
//! A SUCCESS object has NO `status`; an ERROR object has NO `packages`.

#![forbid(unsafe_code)]

// This binary is the Linux half of the ADR-023 escalation chain (pkexec +
// /sys/class/powercap reads) and has no non-Linux build; the stub main keeps
// the workspace compiling on Windows/macOS.
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "taskmanager-rapl-helper is the Linux pkexec/RAPL helper; \
         there is no build of it on this platform"
    );
}

#[cfg(target_os = "linux")]
mod json;
#[cfg(target_os = "linux")]
mod rapl_read;

#[cfg(target_os = "linux")]
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::ExitCode;

#[cfg(target_os = "linux")]
use json::{ErrorEnvelope, ErrorKindJson, PackageJson, SCHEMA_VERSION, SuccessEnvelope};
#[cfg(target_os = "linux")]
use rapl_read::{ReadOutcome, sample_packages};

/// The ONLY path this helper reads: the kernel's powercap class tree.
#[cfg(target_os = "linux")]
const POWERCAP_ROOT: &str = "/sys/class/powercap";

/// The fixed sample window. ~1 second matches the product's other rate
/// windows; reported in the `sample_ms` field so the consumer can present it.
#[cfg(target_os = "linux")]
const SAMPLE_MS: u32 = 1000;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    emit(run())
}

/// The single pass: sample every RAPL package twice over the fixed window and
/// produce either a success payload (per-package watts) or a typed failure.
#[cfg(target_os = "linux")]
fn run() -> Outcome {
    match sample_packages(Path::new(POWERCAP_ROOT), u64::from(SAMPLE_MS)) {
        ReadOutcome::Packages { packages } => Outcome::Success {
            packages: packages
                .into_iter()
                .map(|package| PackageJson {
                    name: package.name,
                    power_w: package.power_w,
                    energy_delta_uj: package.energy_delta_uj,
                })
                .collect(),
        },
        ReadOutcome::Error(error) => Outcome::Error(error.kind, error.detail),
    }
}

/// The terminal result of [`run`]: a success payload to serialize, or a typed
/// error kind + detail.
#[cfg(target_os = "linux")]
enum Outcome {
    Success { packages: Vec<PackageJson> },
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
                sample_ms: SAMPLE_MS,
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
            let _ = writeln!(io::stderr(), "rapl-helper: stdout write failed: {error}");
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
#[path = "../tests/headless/rapl_helper_main.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;

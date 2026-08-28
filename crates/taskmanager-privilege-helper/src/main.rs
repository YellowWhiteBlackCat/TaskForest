//! `taskmanager-privilege-helper` — one feature-specific privileged piece of
//! TaskForest.
//!
//! Invoked through the OS-native escalation prompt (polkit + `pkexec` on Linux,
//! per ADR-023 and `docs/PERMISSION_MODEL.md` Boundary 2), it performs exactly
//! ONE operation: a SYSTEM-WIDE Intel i915/xe GPU per-engine utilization read
//! via the audited `perf_event_open` boundary crate ([ADR-022]
//! `taskmanager-perf-ioctl` — one of the workspace's four `unsafe` trust roots,
//! reached here ONLY through its safe API). It writes ONE JSON object to stdout
//! and
//! exits. There are no flags and no file access beyond `/sys` discovery plus the
//! perf counter read itself — minimal privileged attack surface.
//!
//! Honesty red line: a missing PMU, a permission denial, or any open/read
//! failure emits a typed ERROR envelope and a non-zero exit. The helper NEVER
//! emits a success object with fabricated or zeroed `busy_pct`.
//!
//! Shared JSON contract (must match the Track B consumer exactly):
//! ```text
//! SUCCESS: {"schema":1,"driver":"xe"|"i915","sample_ms":<u32>,
//!           "engines":[{"name":"<string>","class":"<string>","busy_pct":<f32>}]}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_pmu"|"open_failed"|"read_failed",
//!           "detail":"<string>"}
//! ```
//! A SUCCESS object has NO `status`; an ERROR object has NO `engines`.

#![forbid(unsafe_code)]

// This binary is the Linux half of the ADR-022/023 escalation chain (pkexec +
// perf_event_open on the Intel i915/xe PMU) and has no non-Linux build; the
// stub main keeps the workspace compiling on Windows/macOS without the PMU
// boundary crate's types.
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "taskmanager-privilege-helper is the Linux pkexec/Intel-PMU helper; \
         there is no build of it on this platform"
    );
}

#[cfg(target_os = "linux")]
mod discovery;
#[cfg(target_os = "linux")]
mod engine_names;
#[cfg(target_os = "linux")]
mod json;
#[cfg(target_os = "linux")]
mod sample;

#[cfg(target_os = "linux")]
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::process::ExitCode;

#[cfg(target_os = "linux")]
use discovery::{Driver, discover_intel_gpu, discover_pmu_layout};
#[cfg(target_os = "linux")]
use json::{EngineJson, ErrorEnvelope, ErrorKindJson, SCHEMA_VERSION, SuccessEnvelope};
#[cfg(target_os = "linux")]
use sample::{SampleError, sample};

/// The fixed sample window. ~1 second matches the product's other rate windows;
/// reported in the `sample_ms` field so the consumer can present it.
#[cfg(target_os = "linux")]
const SAMPLE_MS: u32 = 1000;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    let outcome = run();
    emit(outcome)
}

/// The single pass: discover the Intel GPU, resolve its PMU, sample, and either
/// a success payload (driver + engines) or a typed failure.
#[cfg(target_os = "linux")]
fn run() -> Outcome {
    let Some((device, driver)) = discover_intel_gpu() else {
        return Outcome::Error(
            ErrorKindJson::NoPmu,
            "no Intel xe/i915 GPU device under /sys/class/drm".to_string(),
        );
    };
    let Some(layout) = discover_pmu_layout(&device, driver) else {
        return Outcome::Error(
            ErrorKindJson::NoPmu,
            format!(
                "no {} perf PMU registered under /sys/bus/event_source/devices for {}",
                driver.keyword(),
                device.display()
            ),
        );
    };
    match sample(layout, SAMPLE_MS) {
        Ok(engines) => Outcome::Success { driver, engines },
        Err(error) => Outcome::Error(sample_error_kind(&error), sample_error_detail(&error)),
    }
}

/// The terminal result of [`run`]: a success payload to serialize, or a typed
/// error kind + detail.
#[cfg(target_os = "linux")]
enum Outcome {
    Success {
        driver: Driver,
        engines: Vec<EngineJson>,
    },
    Error(ErrorKindJson, String),
}

/// Map a [`SampleError`] to its contract error kind.
#[cfg(target_os = "linux")]
fn sample_error_kind(error: &SampleError) -> ErrorKindJson {
    match error {
        SampleError::PermissionDenied(_) => ErrorKindJson::PermissionDenied,
        SampleError::OpenFailed(_) => ErrorKindJson::OpenFailed,
        SampleError::NoEngines(_) => ErrorKindJson::NoPmu,
        SampleError::ReadFailed(_) => ErrorKindJson::ReadFailed,
    }
}

/// Extract the human-readable detail from a [`SampleError`].
#[cfg(target_os = "linux")]
fn sample_error_detail(error: &SampleError) -> String {
    match error {
        SampleError::PermissionDenied(detail)
        | SampleError::OpenFailed(detail)
        | SampleError::NoEngines(detail)
        | SampleError::ReadFailed(detail) => detail.clone(),
    }
}

/// Serialize the outcome to stdout (flushed), and choose the process exit code.
/// Any serialization or stdout failure is itself an honest ERROR (open_failed
/// re-purposed as the closest "could not produce output" kind) rather than a
/// silent success.
#[cfg(target_os = "linux")]
fn emit(outcome: Outcome) -> ExitCode {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let (json_string, kind) = match outcome {
        Outcome::Success { driver, engines } => {
            let envelope = SuccessEnvelope {
                schema: SCHEMA_VERSION,
                driver: driver.keyword(),
                sample_ms: SAMPLE_MS,
                engines,
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
            let _ = writeln!(
                io::stderr(),
                "privilege-helper: stdout write failed: {error}"
            );
            ErrorKindJson::OpenFailed
        })
    });
    match exit_kind {
        Some(kind) => ExitCode::from(kind.exit_code().clamp(1, 255) as u8),
        None => ExitCode::SUCCESS,
    }
}

/// Build the fallback ERROR envelope JSON for a serialization failure (and the
/// kind to exit with). Hand-rolled because the failure IS the serializer, so the
/// envelope cannot itself go through serde_json. Quotes/backslashes in the
/// detail are escaped so the result stays valid JSON.
#[cfg(target_os = "linux")]
fn serialize_error_json(detail: &str) -> (String, Option<ErrorKindJson>) {
    let escaped = detail.replace('\\', "\\\\").replace('"', "\\\"");
    (
        format!(r#"{{"status":"error","kind":"open_failed","detail":"{escaped}"}}"#),
        Some(ErrorKindJson::OpenFailed),
    )
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../tests/headless/privilege_main.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;

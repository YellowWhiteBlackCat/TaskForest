//! The shared JSON envelope contract between this helper (Track A) and the
//! main-loop consumer (Track B). Exactly ONE JSON object is written to stdout,
//! then the process exits.
//!
//! ```text
//! SUCCESS: {"schema":1,"driver":"xe"|"i915","sample_ms":<u32>,
//!           "engines":[{"name":"<string>","class":"<string>","busy_pct":<f32 0.0-100.0>}]}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_pmu"|"open_failed"|"read_failed",
//!           "detail":"<string>"}
//! ```
//!
//! A SUCCESS object carries NO `"status"` field; an ERROR object carries NO
//! `"engines"` field. The consumer distinguishes the two solely by the presence
//! of `"engines"` — so the two structs below MUST stay disjoint in their fields.
//!
//! # Field vocabulary (Track A and Track B MUST agree)
//!
//! Each engine object has:
//! * `name` — the human display label, matching the rest of TaskForest's Intel
//!   engine vocabulary: `"Render/3D"`, `"Copy"`, `"Video Decode"`,
//!   `"Video Encode"`, `"Compute"` (unknown future engines pass through
//!   upper-cased).
//! * `class` — the stable lowercase engine-class keyword, the UI uses for
//!   icon/colour mapping: `"render"`, `"copy"`, `"video"`, `"video-enhance"`,
//!   `"compute"`. Derived from the i915 UAPI `engine_class` id shared by xe.
//! * `busy_pct` — the 0.0–100.0 per-engine busy ratio over the sample window.

use serde::Serialize;

/// The envelope schema version. Bumped only on a breaking shape change; the
/// consumer gates on this before reading the rest.
pub const SCHEMA_VERSION: u32 = 1;

/// One engine's sampled utilization, ready to serialize into the `engines` array.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EngineJson {
    /// Human display label, e.g. `"Render/3D"`.
    pub name: String,
    /// Stable lowercase class keyword, e.g. `"render"`.
    pub class: String,
    /// 0.0–100.0 busy ratio over the sample window (clamped; never NaN/inf).
    pub busy_pct: f32,
}

/// The SUCCESS envelope. Serialized field order is `schema, driver, sample_ms,
/// engines` and there is deliberately NO `status` field — the consumer keys off
/// `engines`' presence.
#[derive(Debug, Clone, Serialize)]
pub struct SuccessEnvelope {
    pub schema: u32,
    pub driver: &'static str,
    pub sample_ms: u32,
    pub engines: Vec<EngineJson>,
}

/// The typed ERROR envelope. `status` is always the literal `"error"`; `kind`
/// serializes to the snake_case keyword the contract names; `detail` is a short
/// human-readable diagnostic. There is deliberately NO `engines` field.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub status: &'static str,
    pub kind: ErrorKindJson,
    pub detail: String,
}

/// The typed error category. Serializes to the exact snake_case keywords of the
/// shared contract (`permission_denied`, `no_pmu`, `open_failed`, `read_failed`).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKindJson {
    /// `perf_event_open` returned `EACCES`/`EPERM` (restrictive
    /// `perf_event_paranoid`). The user can reach the data via the OS-native
    /// escalation prompt (polkit/pkexec).
    PermissionDenied,
    /// No Intel GPU PMU device could be discovered on this host (no i915/xe
    /// card, or the card exposes no `tile*/gt*/engines/` tree).
    NoPmu,
    /// The PMU was discovered but a counter `open` failed for a non-permission
    /// reason (resource exhaustion, EINVAL config, …).
    OpenFailed,
    /// A counter was opened but `read_counter` failed.
    ReadFailed,
}

impl ErrorKindJson {
    /// Distinct non-zero exit code per kind so the polkit invocation path and
    /// the integrator's on-box verification can diagnose without parsing JSON.
    /// `0` is reserved for SUCCESS; `1` is reserved for an unexpected panic.
    pub const fn exit_code(self) -> i32 {
        match self {
            ErrorKindJson::PermissionDenied => 2,
            ErrorKindJson::NoPmu => 3,
            ErrorKindJson::OpenFailed => 4,
            ErrorKindJson::ReadFailed => 5,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/privilege_json.rs"]
mod tests;

//! The shared JSON envelope contract between this helper and the escalation
//! consumer. Exactly ONE JSON object is written to stdout, then the process
//! exits.
//!
//! ```text
//! SUCCESS: {"schema":1,"sample_ms":<u32>,
//!           "packages":[{"name":"<string>","power_w":<f32 finite >= 0.0>,
//!                        "energy_delta_uj":<u64>}]}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_rapl"|"open_failed"|"read_failed",
//!           "detail":"<string>"}
//! ```
//!
//! A SUCCESS object carries NO `"status"` field; an ERROR object carries NO
//! `"packages"` field. The consumer distinguishes the two solely by the
//! presence of `"packages"` — so the two structs below MUST stay disjoint in
//! their fields.
//!
//! # Field vocabulary
//!
//! Each package object has:
//! * `name` — the package's sysfs `name` content (trimmed);
//! * `power_w` — average watts over the sample window, finite and >= 0.0 by
//!   construction (a non-finite result drops the package instead of being
//!   emitted);
//! * `energy_delta_uj` — the microjoule energy delta the watts were derived
//!   from, so the consumer can re-derive or audit the rate.

use serde::Serialize;

/// The envelope schema version. Bumped only on a breaking shape change; the
/// consumer gates on this before reading the rest.
pub const SCHEMA_VERSION: u32 = 1;

/// One package's measured power, ready to serialize into the `packages`
/// array.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PackageJson {
    pub name: String,
    /// Average watts over the sample window (finite, >= 0.0).
    pub power_w: f32,
    /// Energy consumed during the window in microjoules.
    pub energy_delta_uj: u64,
}

/// The SUCCESS envelope. Serialized field order is `schema, sample_ms,
/// packages` and there is deliberately NO `status` field — the consumer keys
/// off `packages`' presence.
#[derive(Debug, Clone, Serialize)]
pub struct SuccessEnvelope {
    pub schema: u32,
    /// The fixed sample window in milliseconds the rates were computed over.
    pub sample_ms: u32,
    /// Packages sorted by the numeric suffix of `intel-rapl:N`.
    pub packages: Vec<PackageJson>,
}

/// The typed ERROR envelope. `status` is always the literal `"error"`; `kind`
/// serializes to the snake_case keyword the contract names; `detail` is a
/// short human-readable diagnostic. There is deliberately NO `packages` field.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub status: &'static str,
    pub kind: ErrorKindJson,
    pub detail: String,
}

/// The typed error category. Serializes to the exact snake_case keywords of
/// the shared contract (`permission_denied`, `no_rapl`, `open_failed`,
/// `read_failed`).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKindJson {
    /// `EACCES`/`EPERM` opening the powercap tree or reading a counter file
    /// (`energy_uj` is 0400 root-owned — the exact gap the OS-native
    /// escalation prompt reaches).
    PermissionDenied,
    /// The powercap root is missing, or it exists but holds no top-level
    /// `intel-rapl:*` package — there is no RAPL on this host.
    NoRapl,
    /// The powercap root exists but could not be opened for a non-permission
    /// reason.
    OpenFailed,
    /// A package counter file could not be read or parsed for a non-permission
    /// reason.
    ReadFailed,
}

impl ErrorKindJson {
    /// Distinct non-zero exit code per kind so the polkit invocation path and
    /// the integrator's on-box verification can diagnose without parsing
    /// JSON. `0` is reserved for SUCCESS; `1` is reserved for an unexpected
    /// panic.
    pub const fn exit_code(self) -> i32 {
        match self {
            ErrorKindJson::PermissionDenied => 2,
            ErrorKindJson::NoRapl => 3,
            ErrorKindJson::OpenFailed => 4,
            ErrorKindJson::ReadFailed => 5,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/rapl_helper_json.rs"]
mod tests;

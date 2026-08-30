//! The shared JSON envelope contract between this helper and the escalation
//! consumer. Exactly ONE JSON object is written to stdout, then the process
//! exits.
//!
//! ```text
//! SUCCESS: {"schema":1,"packages":[{"cpu":<u32>,"bclk_mhz":<f32>|null,
//!           "temperature_c":<f32>|null,"multiplier":<f32>|null,
//!           "multiplier_min":<f32>|null,"multiplier_max":<f32>|null,
//!           "vcore_v":<f32>|null}]}
//! ERROR:   {"status":"error","kind":"permission_denied"|"no_msr"|"open_failed"|"read_failed",
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
//! One `packages` element is one `/dev/cpu/N/msr` node ("cpu" = the numeric
//! node suffix `N`, sorted ascending). Every readout field is `None` → JSON
//! `null` when the CPU does not implement the register or the decoded value
//! failed its documented plausibility range — never a fabricated zero.
//! `bclk_mhz` is populated only when CPUID leaf 0x16 enumerates the
//! SDM-defined Bus (Reference) Frequency inside the plausibility envelope
//! (ADR-048 amendment); AMD rows carry honest nulls for the readouts that
//! have no MSR-indexed path on that vendor (ADR-049).

use serde::Serialize;

/// The envelope schema version. Bumped only on a breaking shape change; the
/// consumer gates on this before reading the rest.
pub const SCHEMA_VERSION: u32 = 1;

/// One MSR readout row (one `/dev/cpu/N/msr` node), ready to serialize into
/// the `packages` array. Every readout field is `None` → JSON `null` when the
/// register is unimplemented or the value is outside its documented physical
/// range — never a fabricated zero.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PackageReadingJson {
    /// CPU node index = the numeric directory name `N` of `/dev/cpu/N/msr`.
    pub cpu: u32,
    /// Base clock in MHz from CPUID leaf 0x16 ECX bits 15:0 (the SDM "Bus
    /// (Reference) Frequency"); `null` when the leaf is unenumerated or the
    /// value fails the plausibility envelope (ADR-048 amendment).
    pub bclk_mhz: Option<f32>,
    /// Package temperature in °C = TjMax (0x1A2 bits 23:16) minus the package
    /// digital readout (0x1B1 bits 23:16, gated by its valid bit 31). Always
    /// `null` on AMD: no MSR-indexed path exists there (ADR-049).
    pub temperature_c: Option<f32>,
    /// Current performance ratio: `0x198` bits 15:0 on Intel; on AMD family
    /// 0x17–0x19 the multiplier `(CpuFid ÷ CpuDfsId) × 2` of the P-state
    /// selected by `MSR_PSTATE_S`.
    pub multiplier: Option<f32>,
    /// Minimum multiplier: `0xCE` bits 47:40 on Intel; on AMD family
    /// 0x17–0x19 the lowest enabled P-state's multiplier.
    pub multiplier_min: Option<f32>,
    /// Maximum multiplier: `0x1AD` bits 7:0 on Intel; on AMD family 0x17–0x19
    /// the P-state-0 (Pb0) multiplier.
    pub multiplier_max: Option<f32>,
    /// P-state core voltage in volts: `0x198` bits 47:32 ÷ 2^13 on Intel
    /// (`null` when the CPU leaves the field at 0 — all modern Intel do);
    /// `1.550 − 0.00625 × CpuVid` (SVI2) on AMD family 0x17–0x19.
    pub vcore_v: Option<f32>,
}

/// The SUCCESS envelope. Serialized field order is `schema, packages` and
/// there is deliberately NO `status` field — the consumer keys off
/// `packages`' presence.
#[derive(Debug, Clone, Serialize)]
pub struct SuccessEnvelope {
    pub schema: u32,
    /// One row per existing `/dev/cpu/N/msr` node, sorted by `N`, capped at
    /// 1024 nodes. An empty list is the honest "no nodes under /dev/cpu".
    pub packages: Vec<PackageReadingJson>,
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
/// the shared contract (`permission_denied`, `no_msr`, `open_failed`,
/// `read_failed`).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKindJson {
    /// `EACCES`/`EPERM` opening `/dev/cpu` or an msr node. The user can reach
    /// the data via the OS-native escalation prompt (polkit/pkexec) — this
    /// helper is that elevated half.
    PermissionDenied,
    /// The `/dev/cpu` root is missing (`ENOENT`): no per-CPU MSR nodes exist
    /// on this host (typically the `msr` driver is not loaded).
    NoMsr,
    /// `/dev/cpu` exists but could not be opened for a non-permission reason.
    OpenFailed,
    /// An msr node read failed for a non-permission reason other than the
    /// documented "register not implemented" absence.
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
            ErrorKindJson::NoMsr => 3,
            ErrorKindJson::OpenFailed => 4,
            ErrorKindJson::ReadFailed => 5,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/msr_helper_json.rs"]
mod tests;

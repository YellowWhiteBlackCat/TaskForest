//! The shared JSON envelope contract between this helper and the escalation
//! consumer. Exactly ONE JSON object is written to stdout, then the process
//! exits.
//!
//! ```text
//! SUCCESS: {"schema":1,"slots_total":<u32>,"slots_used":<u32>,
//!           "modules":[{"slot":<u32>,"size_mb":<u32>,"speed_mts":<u32>,
//!                       "configured_speed_mts":<u32>,"manufacturer":<str>,
//!                       "serial_number":<str>,"part_number":<str>,
//!                       "form_factor":<str>,"memory_type":<str>,"locator":<str>}],
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
//!
//! A SUCCESS object carries NO `"status"` field; an ERROR object carries NO
//! `"modules"` field. The consumer distinguishes the two solely by the
//! presence of `"modules"` — so the two structs below MUST stay disjoint in
//! their fields.
//!
//! `identity` is additive under schema 1: it widens the same one-helper walk
//! (types 0/1/2 alongside 17) and consumers that predate it ignore the
//! unknown field, so the shared-contract rule keeps the schema version at 1.
//!
//! # Field vocabulary
//!
//! Each module object lists only POPULATED slots (a record whose `size_mb`
//! decoded to `None` is an empty socket, counted in `slots_total` but never
//! emitted as a module). Fields the SMBIOS record does not state serialize as
//! JSON `null` — the consumer treats `null` as absent, which is the honest
//! representation of an unknown fact. The whole `identity` object is `null`
//! only when the DMI tree carries no type-0/1/2 entries at all.

use serde::Serialize;

/// The envelope schema version. Bumped only on a breaking shape change; the
/// consumer gates on this before reading the rest.
pub const SCHEMA_VERSION: u32 = 1;

/// One populated memory module, ready to serialize into the `modules` array.
/// Every optional field is `None` → JSON `null` when the SMBIOS record does
/// not state it — never a fabricated zero or empty string.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryModuleJson {
    /// Slot index = the numeric suffix `N` of the `17-N` entries directory.
    pub slot: u32,
    pub size_mb: Option<u32>,
    pub speed_mts: Option<u32>,
    pub configured_speed_mts: Option<u32>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
    pub part_number: Option<String>,
    pub form_factor: Option<&'static str>,
    pub memory_type: Option<&'static str>,
    pub locator: Option<String>,
}

/// The system/board identity facts from the type-0/1/2 records, ready to
/// serialize into the SUCCESS envelope's `identity` object. Every field is
/// `None` → JSON `null` when the source record did not state it or the record
/// type is absent on this host — never a fabricated zero or empty string.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct DmiIdentityJson {
    /// BIOS vendor string (type 0, offset 0x04).
    pub bios_vendor: Option<String>,
    /// BIOS version string (type 0, offset 0x05).
    pub bios_version: Option<String>,
    /// BIOS release date string (type 0, offset 0x08).
    pub bios_date: Option<String>,
    /// Board manufacturer string (type 2, offset 0x04).
    pub board_manufacturer: Option<String>,
    /// Board product name string (type 2, offset 0x05).
    pub board_product: Option<String>,
    /// Board serial number string (type 2, offset 0x07).
    pub board_serial: Option<String>,
    /// Board asset tag string (type 2, offset 0x08).
    pub board_asset_tag: Option<String>,
    /// System manufacturer string (type 1, offset 0x04).
    pub system_manufacturer: Option<String>,
    /// System product name string (type 1, offset 0x05).
    pub system_product: Option<String>,
    /// System serial number string (type 1, offset 0x07).
    pub system_serial: Option<String>,
    /// System UUID, canonical hyphenated lowercase (type 1, 0x08..0x18).
    pub system_uuid: Option<String>,
    /// System SKU number string (type 1, offset 0x19).
    pub system_sku: Option<String>,
    /// System family string (type 1, offset 0x1A).
    pub system_family: Option<String>,
}

/// The SUCCESS envelope. Serialized field order is `schema, slots_total,
/// slots_used, modules, identity` and there is deliberately NO `status` field
/// — the consumer keys off `modules`' presence.
#[derive(Debug, Clone, Serialize)]
pub struct SuccessEnvelope {
    pub schema: u32,
    /// Count of type-17 records seen (populated + empty + malformed).
    pub slots_total: u32,
    /// Count of records that decoded as populated modules.
    pub slots_used: u32,
    /// Populated modules sorted by slot index.
    pub modules: Vec<MemoryModuleJson>,
    /// System/board identity facts, or `None` → JSON `null` when the DMI tree
    /// has no type-0/1/2 entries at all (an honest absence, not a failure).
    pub identity: Option<DmiIdentityJson>,
}

/// The typed ERROR envelope. `status` is always the literal `"error"`; `kind`
/// serializes to the snake_case keyword the contract names; `detail` is a
/// short human-readable diagnostic. There is deliberately NO `modules` field.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub status: &'static str,
    pub kind: ErrorKindJson,
    pub detail: String,
}

/// The typed error category. Serializes to the exact snake_case keywords of
/// the shared contract (`permission_denied`, `no_dmi`, `open_failed`,
/// `read_failed`).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKindJson {
    /// `EACCES`/`EPERM` opening the entries directory or reading a raw file.
    /// The user can reach the data via the OS-native escalation prompt
    /// (polkit/pkexec) — this helper is that elevated half.
    PermissionDenied,
    /// The DMI entries root is missing (`ENOENT`): no SMBIOS on this host.
    NoDmi,
    /// The entries directory exists but could not be opened for a
    /// non-permission reason.
    OpenFailed,
    /// A `<type>-N/raw` record read failed for a non-permission reason.
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
            ErrorKindJson::NoDmi => 3,
            ErrorKindJson::OpenFailed => 4,
            ErrorKindJson::ReadFailed => 5,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/smbios_helper_json.rs"]
mod tests;

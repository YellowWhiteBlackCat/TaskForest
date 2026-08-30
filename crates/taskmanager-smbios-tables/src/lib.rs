//! `taskmanager-smbios-tables` — ONE authority for SMBIOS record parsing.
//!
//! Both SMBIOS consumers in TaskForest decode DMI records through this crate:
//! the unprivileged Linux adapter's raw-DMI probe and the pkexec-escalated
//! `taskmanager-smbios-helper` (ADR-023, Boundary 2 of
//! `docs/PERMISSION_MODEL.md`). One format, one parser — the helper and the
//! adapter can never drift apart on field offsets or sentinel semantics.
//!
//! The input is the raw structure bytes exactly as exported by the kernel at
//! `/sys/firmware/dmi/entries/<type>-N/raw`: the formatted area
//! (`raw[0..length]`) followed by the NUL-separated, double-NUL-terminated
//! string set. The layouts below match dmidecode (`dmidecode.c` cases 0/1/2/17
//! of `dmi_decode`) and the working raw-DMI probe in
//! `crates/taskmanager-platform-linux` (engine/collector/compute/
//! memory_sources/dmi.rs).
//!
//! Honesty red line: absent or unknown data is `None` — never a zero, never a
//! fabricated string. Every parser is total: a too-short record yields the
//! fields that fit and never panics; each `parse_*` returns `None` only for a
//! wrong type byte, a declared length below the record's minimum, or a
//! declared length beyond the raw slice.
//!
//! Type-17 offsets (into the raw structure file):
//! * `raw[0]` type (must be 17), `raw[1]` length, `raw[2..4]` handle (ignored);
//! * `raw[12..14]` size in MB (bit 15 set: value is in KB units);
//! * `raw[14]` form factor, `raw[18]` memory type (enum bytes);
//! * `raw[15]` device locator, `raw[23]` manufacturer, `raw[24]` serial
//!   number, `raw[26]` part number (string-set indexes);
//! * `raw[21..23]` maximum speed (present only when length >= 23);
//! * `raw[32..34]` configured speed (present only when length >= 34, i.e.
//!   SMBIOS 2.6+).
//!
//! Identity-record offsets (dmidecode `dmi_decode` cases 0/1/2; each string
//! field exists only when its offset lies inside the declared length):
//! * type 0 (BIOS Information, minimum length 0x12): vendor str@`0x04`,
//!   version str@`0x05`, release date str@`0x08`;
//! * type 1 (System Information, minimum length 0x08): manufacturer
//!   str@`0x04`, product str@`0x05`, version str@`0x06`, serial str@`0x07`,
//!   UUID bytes `0x08..0x18` (present only when length >= 0x19), SKU
//!   str@`0x19`, family str@`0x1A`;
//! * type 2 (Base Board Information, minimum length 0x08): manufacturer
//!   str@`0x04`, product str@`0x05`, version str@`0x06`, serial str@`0x07`,
//!   asset tag str@`0x08`, location in chassis str@`0x0A`.

#![forbid(unsafe_code)]

/// SMBIOS structure type of a Memory Device record.
const MEMORY_DEVICE_TYPE: u8 = 17;
/// Minimum type-17 formatted-area length (SMBIOS 2.1). Shorter is malformed.
const MIN_MEMORY_DEVICE_LENGTH: usize = 21;
/// SMBIOS structure type of a BIOS Information record.
const BIOS_INFORMATION_TYPE: u8 = 0;
/// Minimum type-0 formatted-area length (dmidecode's `h->length < 0x12` gate;
/// every spec revision carrying the vendor/version/date strings is 0x12+).
const MIN_BIOS_INFORMATION_LENGTH: usize = 0x12;
/// SMBIOS structure type of a System Information record.
const SYSTEM_INFORMATION_TYPE: u8 = 1;
/// Minimum type-1 formatted-area length (dmidecode's `h->length < 0x08` gate:
/// the header alone; every string field is gated per-offset).
const MIN_SYSTEM_INFORMATION_LENGTH: usize = 0x08;
/// Declared length at which the 16-byte UUID (`raw[0x08..0x18]`) exists
/// (dmidecode reads it only after its `h->length < 0x19` gate).
const UUID_PRESENT_LENGTH: usize = 0x19;
/// SMBIOS structure type of a Base Board Information record.
const BASEBOARD_INFORMATION_TYPE: u8 = 2;
/// Minimum type-2 formatted-area length (dmidecode's `h->length < 0x08` gate).
const MIN_BASEBOARD_INFORMATION_LENGTH: usize = 0x08;
/// Length at which the maximum-speed field (`raw[21..23]`) exists.
const SPEED_PRESENT_LENGTH: usize = 23;
/// Length at which the configured-speed field (`raw[32..34]`) exists
/// (SMBIOS 2.6).
const CONFIGURED_SPEED_PRESENT_LENGTH: usize = 34;
/// Size-word sentinel meaning "unknown" (0x7FFF per the SMBIOS spec: with bit
/// 15 reserved for the KB-units mark, 0x7FFF is the largest MB magnitude and
/// means unknown).
const SIZE_UNKNOWN: u16 = 0x7FFF;
/// Speed-word sentinel meaning "unknown" (0xFFFF per the SMBIOS spec).
const SPEED_UNKNOWN: u16 = 0xFFFF;
/// Size-word bit marking the value as KB units instead of MB.
const SIZE_KB_UNITS_BIT: u16 = 0x8000;
/// Mask extracting the magnitude from a KB-units size word.
const SIZE_KB_MASK: u16 = 0x7FFF;

/// One parsed Memory Device (SMBIOS type 17) record.
///
/// Every field is `Option`: `None` means the fact is absent, unknown
/// (sentinel value), not defined for the record's SMBIOS version, or the
/// string index points outside the record's string set — never a fabricated
/// zero or empty string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryDeviceRecord {
    /// Module size in megabytes. `None` when absent/unknown (`0`/`0x7FFF`).
    pub size_mb: Option<u64>,
    /// Maximum speed in MT/s. `None` when `0`/`0xFFFF` or the record predates
    /// the field (length < 23).
    pub speed_mts: Option<u32>,
    /// Configured speed in MT/s. `None` when `0`/`0xFFFF` or the record
    /// predates the field (length < 34, i.e. SMBIOS < 2.6).
    pub configured_speed_mts: Option<u32>,
    /// Form factor label (`"SODIMM"`, `"DIMM"`, …). `None` when 0/unmapped.
    pub form_factor: Option<&'static str>,
    /// Memory type label (`"DDR5"`, `"LPDDR4"`, …). `None` when 0/unmapped.
    pub memory_type: Option<&'static str>,
    /// Manufacturer string from the record's string set.
    pub manufacturer: Option<String>,
    /// Serial number string from the record's string set.
    pub serial_number: Option<String>,
    /// Part number string from the record's string set.
    pub part_number: Option<String>,
    /// Device locator string (e.g. `"ChannelA-DIMM0"`).
    pub device_locator: Option<String>,
}

/// Parse one raw SMBIOS Memory Device (type 17) structure.
///
/// `raw` is the whole structure file content (formatted area + string set).
/// Returns `None` only when the record is not a type-17 structure at all:
/// wrong type byte, declared length below 21, or declared length beyond the
/// raw slice. A well-typed but truncated or partial record returns `Some`
/// with every field that fits; everything unverifiable is `None`.
pub fn parse_memory_device(raw: &[u8]) -> Option<MemoryDeviceRecord> {
    if byte(raw, 0)? != MEMORY_DEVICE_TYPE {
        return None;
    }
    let length = usize::from(byte(raw, 1)?);
    if length < MIN_MEMORY_DEVICE_LENGTH || length > raw.len() {
        return None;
    }
    Some(MemoryDeviceRecord {
        size_mb: size_mb(raw),
        speed_mts: speed_field(raw, length, 21, SPEED_PRESENT_LENGTH),
        configured_speed_mts: speed_field(raw, length, 32, CONFIGURED_SPEED_PRESENT_LENGTH),
        form_factor: byte(raw, 14).and_then(form_factor_label),
        memory_type: byte(raw, 18).and_then(memory_type_label),
        manufacturer: string_field(raw, length, 23),
        serial_number: string_field(raw, length, 24),
        part_number: string_field(raw, length, 26),
        device_locator: string_field(raw, length, 15),
    })
}

/// One parsed BIOS Information (SMBIOS type 0) record. Every field is `None`
/// when the record's string index is 0, points outside the string set, or the
/// indexed string is empty — never a fabricated label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiosInformationRecord {
    /// Firmware vendor string (e.g. `"American Megatrends Inc."`).
    pub vendor: Option<String>,
    /// Firmware version string (e.g. `"1.27.0"`).
    pub version: Option<String>,
    /// Firmware release date string (mm/dd/yyyy per the spec).
    pub release_date: Option<String>,
}

/// One parsed System Information (SMBIOS type 1) record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemInformationRecord {
    /// System manufacturer string.
    pub manufacturer: Option<String>,
    /// Product name string.
    pub product_name: Option<String>,
    /// Product version string.
    pub version: Option<String>,
    /// System serial number string.
    pub serial_number: Option<String>,
    /// System UUID in the canonical hyphenated lowercase form (SMBIOS 2.6+
    /// mixed-endian first-3-fields byte order, as dmidecode renders it).
    /// `None` when the record predates the field (length < 0x19) or holds the
    /// all-zeros/all-ones pattern.
    pub uuid: Option<String>,
    /// SKU number string.
    pub sku: Option<String>,
    /// Product family string.
    pub family: Option<String>,
}

/// One parsed Base Board Information (SMBIOS type 2) record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseboardInformationRecord {
    /// Board manufacturer string.
    pub manufacturer: Option<String>,
    /// Board product name string.
    pub product_name: Option<String>,
    /// Board version string.
    pub version: Option<String>,
    /// Board serial number string.
    pub serial_number: Option<String>,
    /// Board asset tag string.
    pub asset_tag: Option<String>,
    /// Location-in-chassis string (e.g. `"Base Board"`).
    pub location_in_chassis: Option<String>,
}

/// Parse one raw SMBIOS BIOS Information (type 0) structure. `None` only for a
/// wrong type byte, a declared length below 0x12, or a declared length beyond
/// the raw slice.
pub fn parse_bios_information(raw: &[u8]) -> Option<BiosInformationRecord> {
    if byte(raw, 0)? != BIOS_INFORMATION_TYPE {
        return None;
    }
    let length = usize::from(byte(raw, 1)?);
    if length < MIN_BIOS_INFORMATION_LENGTH || length > raw.len() {
        return None;
    }
    Some(BiosInformationRecord {
        vendor: string_field(raw, length, 0x04),
        version: string_field(raw, length, 0x05),
        release_date: string_field(raw, length, 0x08),
    })
}

/// Parse one raw SMBIOS System Information (type 1) structure. `None` only for
/// a wrong type byte, a declared length below 0x08, or a declared length
/// beyond the raw slice; each string field is additionally gated on its own
/// offset (SKU exists from length 0x1A, family from 0x1B) exactly as dmidecode
/// gates them.
pub fn parse_system_information(raw: &[u8]) -> Option<SystemInformationRecord> {
    if byte(raw, 0)? != SYSTEM_INFORMATION_TYPE {
        return None;
    }
    let length = usize::from(byte(raw, 1)?);
    if length < MIN_SYSTEM_INFORMATION_LENGTH || length > raw.len() {
        return None;
    }
    Some(SystemInformationRecord {
        manufacturer: string_field(raw, length, 0x04),
        product_name: string_field(raw, length, 0x05),
        version: string_field(raw, length, 0x06),
        serial_number: string_field(raw, length, 0x07),
        uuid: uuid_field(raw, length),
        sku: string_field(raw, length, 0x19),
        family: string_field(raw, length, 0x1A),
    })
}

/// Parse one raw SMBIOS Base Board Information (type 2) structure. `None` only
/// for a wrong type byte, a declared length below 0x08, or a declared length
/// beyond the raw slice; asset tag exists from length 0x09 and location from
/// 0x0B (per-offset gates, matching dmidecode's own incremental gates).
pub fn parse_baseboard_information(raw: &[u8]) -> Option<BaseboardInformationRecord> {
    if byte(raw, 0)? != BASEBOARD_INFORMATION_TYPE {
        return None;
    }
    let length = usize::from(byte(raw, 1)?);
    if length < MIN_BASEBOARD_INFORMATION_LENGTH || length > raw.len() {
        return None;
    }
    Some(BaseboardInformationRecord {
        manufacturer: string_field(raw, length, 0x04),
        product_name: string_field(raw, length, 0x05),
        version: string_field(raw, length, 0x06),
        serial_number: string_field(raw, length, 0x07),
        asset_tag: string_field(raw, length, 0x08),
        location_in_chassis: string_field(raw, length, 0x0A),
    })
}

/// Decode the system UUID word at `raw[0x08..0x18]` into the canonical
/// hyphenated lowercase string.
///
/// Present only when the declared length covers all 16 bytes (>= 0x19), the
/// same gate dmidecode applies. Byte order follows the SMBIOS 2.6+ rule
/// (dmidecode's `dmi_system_uuid` for `ver >= 0x0206`): the first three
/// fields are little-endian on the wire, so the bytes `b0..b15` render as
/// `b3b2b1b0-b5b4-b7b6-b8b9-b10b11b12b13b14b15`. A single record file does not
/// carry the table's spec version, but every kernel exporting
/// `/sys/firmware/dmi/entries` is SMBIOS 2.6+ era, so the swapped rendering is
/// the only one this path needs. The spec's all-0xFF ("not present") and
/// all-0x00 ("not settable") patterns are honest `None`s — never a fake
/// `00000000-0000-0000-0000-000000000000`.
fn uuid_field(raw: &[u8], length: usize) -> Option<String> {
    if length < UUID_PRESENT_LENGTH {
        return None;
    }
    let bytes = raw.get(0x08..0x18)?;
    if bytes.iter().all(|&value| value == 0x00) || bytes.iter().all(|&value| value == 0xFF) {
        return None;
    }
    Some(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-\
         {:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[3],
        bytes[2],
        bytes[1],
        bytes[0],
        bytes[5],
        bytes[4],
        bytes[7],
        bytes[6],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    ))
}

/// Decode the size word (`raw[12..14]`): `0`/`0x7FFF` mean absent/unknown;
/// bit 15 set marks KB units (rounded up to whole MB); otherwise MB directly.
fn size_mb(raw: &[u8]) -> Option<u64> {
    let value = word(raw, 12)?;
    if value == 0 || value == SIZE_UNKNOWN {
        return None;
    }
    if value & SIZE_KB_UNITS_BIT != 0 {
        let size_kb = u64::from(value & SIZE_KB_MASK);
        Some(size_kb.div_ceil(1024))
    } else {
        Some(u64::from(value))
    }
}

/// Decode a speed word at `offset`, present only when the record's declared
/// length reaches `present_length`. `0`/`0xFFFF` mean unknown.
fn speed_field(raw: &[u8], length: usize, offset: usize, present_length: usize) -> Option<u32> {
    if length < present_length {
        return None;
    }
    let value = word(raw, offset)?;
    if value == 0 || value == SPEED_UNKNOWN {
        return None;
    }
    Some(u32::from(value))
}

/// Read a string-set index byte at `offset` and decode the indexed string.
/// `None` when the field is not part of this record's formatted area (the
/// offset lies at or beyond the declared length), when the index is 0 or
/// beyond the string set, or when the indexed string is empty.
fn string_field(raw: &[u8], length: usize, offset: usize) -> Option<String> {
    if offset >= length {
        return None;
    }
    string_set_at(raw, length, byte(raw, offset)?)
}

/// Decode string number `index` (1-based) from the record's string set: the
/// bytes after the formatted area, NUL-separated, double-NUL terminated.
/// The raw slice is the whole universe — a file truncated right after (or
/// even inside) the string set simply yields no further strings, never a
/// panic. Bytes after the double-NUL terminator are not part of the set.
fn string_set_at(raw: &[u8], length: usize, index: u8) -> Option<String> {
    let wanted = usize::from(index);
    if wanted == 0 {
        return None;
    }
    let set = raw.get(length..).unwrap_or(&[]);
    let mut current = 1usize;
    let mut start = 0usize;
    for (position, &byte_value) in set.iter().enumerate() {
        if byte_value != 0 {
            continue;
        }
        if current == wanted {
            let sequence = &set[start..position];
            if sequence.is_empty() {
                return None;
            }
            // Firmware pads string facts with trailing spaces (a captured
            // on-box part number carried 12 of them); the padding is
            // transport noise, not part of the fact, so it is trimmed
            // here — once, at the one authority for this format. A string
            // that is whitespace-only after trimming carries no fact and
            // stays absent.
            let decoded = String::from_utf8_lossy(sequence).trim_end().to_owned();
            return if decoded.is_empty() {
                None
            } else {
                Some(decoded)
            };
        }
        // A NUL immediately followed by another NUL terminates the set (the
        // first NUL closes the previous string, the second closes the set).
        if set.get(position + 1) == Some(&0) {
            return None;
        }
        current += 1;
        start = position + 1;
    }
    None
}

/// Bounds-guarded single-byte read; `None` when `offset` is beyond the slice.
fn byte(raw: &[u8], offset: usize) -> Option<u8> {
    raw.get(offset).copied()
}

/// Bounds-guarded little-endian `u16` read; `None` when it does not fit.
fn word(raw: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        byte(raw, offset)?,
        byte(raw, offset + 1)?,
    ]))
}

/// Map the type-17 form-factor enum byte to its label. 0 and unmapped codes
/// are `None` (never fabricated as "Unknown" — that is enum value 0x02).
fn form_factor_label(code: u8) -> Option<&'static str> {
    match code {
        0x01 => Some("Other"),
        0x02 => Some("Unknown"),
        0x03 => Some("SIP"),
        0x04 => Some("DIP"),
        0x05 => Some("ZIP"),
        0x06 => Some("Proprietary Card"),
        0x07 => Some("SIMM"),
        0x08 => Some("DIMM"),
        0x09 => Some("TSOP"),
        0x0A => Some("Row Of Chips"),
        0x0B => Some("RIMM"),
        0x0D => Some("SODIMM"),
        0x0E => Some("SRIMM"),
        0x0F => Some("FB-DIMM"),
        _ => None,
    }
}

/// Map the type-17 memory-type enum byte to its label. 0 and unmapped codes
/// are `None`.
fn memory_type_label(code: u8) -> Option<&'static str> {
    match code {
        0x03 => Some("DRAM"),
        0x0F => Some("SDRAM"),
        0x11 => Some("RDRAM"),
        0x12 => Some("DDR"),
        0x13 => Some("DDR2"),
        0x18 => Some("DDR3"),
        0x1A => Some("DDR4"),
        0x1B => Some("LPDDR"),
        0x1C => Some("LPDDR2"),
        0x1D => Some("LPDDR3"),
        0x1E => Some("LPDDR4"),
        0x20 => Some("HBM"),
        0x21 => Some("HBM2"),
        0x22 => Some("DDR5"),
        0x23 => Some("LPDDR5"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/headless/smbios_tables_lib.rs"]
mod tests;

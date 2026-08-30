//! The DMI-entries walk — pure safe `/sys` reads, parameterized by the
//! entries root so tests run against fixture trees instead of the live host.
//!
//! The walk enumerates `/sys/firmware/dmi/entries/17-*/raw` (Memory Device
//! records) plus the FIRST `0-*`, `1-*`, `2-*` entries (BIOS / System /
//! Base Board identity), decodes each record through the ONE SMBIOS format
//! authority (`taskmanager-smbios-tables`), and folds the result into the
//! module list + slot counts + identity object of the shared JSON contract.
//!
//! Honesty rules:
//! * a malformed record (wrong type byte / bad declared length) is SKIPPED
//!   but still counted in `slots_total` — it is a slot the firmware
//!   described, just not one we can read;
//! * a record with no module installed (`size_mb` is `None`) counts in
//!   `slots_total` only;
//! * an empty-but-present entries directory is an honest SUCCESS with zero
//!   slots and a `null` identity — never a fabricated error;
//! * `identity` is `Some` exactly when at least one type-0/1/2 entry was
//!   read; per-field facts inside stay `None` when the record is malformed
//!   or does not state them;
//! * ANY I/O failure (permission, unreadable raw file) is a typed ERROR for
//!   the whole walk — a partial module list or a silently missing identity
//!   would understate the machine.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use taskmanager_smbios_tables::{
    BaseboardInformationRecord, BiosInformationRecord, MemoryDeviceRecord, SystemInformationRecord,
    parse_baseboard_information, parse_bios_information, parse_memory_device,
    parse_system_information,
};

use crate::json::{DmiIdentityJson, ErrorKindJson, MemoryModuleJson};

/// Directory-name prefix of Memory Device entries (`17-N`).
const TYPE17_PREFIX: &str = "17-";
/// Directory-name prefixes of the identity entries (`0-N`, `1-N`, `2-N`),
/// indexed by SMBIOS type byte.
const IDENTITY_PREFIXES: [&str; 3] = ["0-", "1-", "2-"];

/// The walk's terminal result: populated modules + slot counts + identity, or
/// a typed error. `modules` lists ONLY populated slots, sorted by slot index.
/// The 13-field identity object is boxed to keep the success variant close to
/// the error variant in size.
pub enum WalkOutcome {
    Success {
        modules: Vec<MemoryModuleJson>,
        slots_total: u32,
        slots_used: u32,
        identity: Option<Box<DmiIdentityJson>>,
    },
    Error(WalkError),
}

/// A typed walk failure, already carrying the contract error kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkError {
    pub kind: ErrorKindJson,
    pub detail: String,
}

/// The lowest-suffix entry directory seen so far for each of types 0/1/2,
/// indexed by SMBIOS type byte (the kernel exports one directory per
/// instance; the first is the record the spec designates as THE
/// bios/system/board description).
#[derive(Default)]
struct IdentitySources {
    entries: [Option<(u32, PathBuf)>; 3],
}

/// Collect the DMI facts under `entries_root` (the `/sys/firmware/dmi/entries`
/// tree): the type-17 module inventory plus the type-0/1/2 identity facts.
/// See the module docs for the honesty rules.
pub fn collect_dmi_facts(entries_root: &Path) -> WalkOutcome {
    let entries = match fs::read_dir(entries_root) {
        Ok(entries) => entries,
        Err(error) => return WalkOutcome::Error(classify_entries_error(&error, entries_root)),
    };
    let mut modules: Vec<MemoryModuleJson> = Vec::new();
    let mut slots_total = 0u32;
    let mut slots_used = 0u32;
    let mut identity_sources = IdentitySources::default();
    for entry in entries {
        let Ok(entry) = entry else {
            return WalkOutcome::Error(WalkError {
                kind: ErrorKindJson::OpenFailed,
                detail: format!("iterating {} failed", entries_root.display()),
            });
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if let Some(slot) = slot_index(&name) {
            slots_total = slots_total.saturating_add(1);
            let raw_path = entry.path().join("raw");
            let bytes = match fs::read(&raw_path) {
                Ok(bytes) => bytes,
                Err(error) => return WalkOutcome::Error(classify_raw_error(&error, &raw_path)),
            };
            // Malformed record: still a firmware-described slot (counted
            // above), but not a module we can honestly report.
            let Some(record) = parse_memory_device(&bytes) else {
                continue;
            };
            // Empty socket: no module installed.
            if record.size_mb.is_none() {
                continue;
            }
            slots_used = slots_used.saturating_add(1);
            modules.push(module_json(slot, &record));
        } else if let Some((kind, suffix)) = identity_entry(&name) {
            // Keep the lowest-numbered entry of each identity type: the spec
            // designates instance 0 as THE bios/system/board description.
            let is_lower = identity_sources.entries[kind]
                .as_ref()
                .is_none_or(|(lowest, _)| suffix < *lowest);
            if is_lower {
                identity_sources.entries[kind] = Some((suffix, entry.path()));
            }
        }
    }
    modules.sort_by_key(|module| module.slot);
    let identity = match identity_facts(&identity_sources) {
        Ok(identity) => identity.map(Box::new),
        Err(error) => return WalkOutcome::Error(error),
    };
    WalkOutcome::Success {
        modules,
        slots_total,
        slots_used,
        identity,
    }
}

/// Resolve a `<prefix>-N` identity directory name to its SMBIOS type (`kind`,
/// the index into [`IDENTITY_PREFIXES`]: 0/1/2) and numeric suffix; `None`
/// for anything else.
fn identity_entry(name: &str) -> Option<(usize, u32)> {
    let kind = IDENTITY_PREFIXES
        .iter()
        .position(|prefix| name.starts_with(prefix))?;
    let suffix = name.strip_prefix(IDENTITY_PREFIXES[kind])?.parse().ok()?;
    Some((kind, suffix))
}

/// The slot index of a `17-N` directory name; `None` for anything else
/// (other SMBIOS types, unparsable suffixes).
fn slot_index(name: &str) -> Option<u32> {
    name.strip_prefix(TYPE17_PREFIX)?.parse().ok()
}

/// Read + decode the recorded identity entries into the contract's identity
/// object. Returns `None` when no type-0/1/2 entry exists at all; a read
/// failure of an existing entry is a typed walk error (same rule as the
/// type-17 raws — under-reading would understate the machine).
fn identity_facts(sources: &IdentitySources) -> Result<Option<DmiIdentityJson>, WalkError> {
    if sources.entries.iter().all(Option::is_none) {
        return Ok(None);
    }
    let mut identity = DmiIdentityJson::default();
    for (kind, entry) in sources.entries.iter().enumerate() {
        let Some((_, path)) = entry else {
            continue;
        };
        let raw_path = path.join("raw");
        let bytes = match fs::read(&raw_path) {
            Ok(bytes) => bytes,
            Err(error) => return Err(classify_raw_error(&error, &raw_path)),
        };
        match kind {
            0 => fill_bios(&mut identity, &parse_bios_information(&bytes)),
            1 => fill_system(&mut identity, &parse_system_information(&bytes)),
            _ => fill_baseboard(&mut identity, &parse_baseboard_information(&bytes)),
        }
    }
    Ok(Some(identity))
}

/// Fold a parsed type-0 record into the identity object; a malformed record
/// (`None`) contributes nothing — its fields stay honest nulls.
fn fill_bios(identity: &mut DmiIdentityJson, record: &Option<BiosInformationRecord>) {
    if let Some(record) = record {
        identity.bios_vendor.clone_from(&record.vendor);
        identity.bios_version.clone_from(&record.version);
        identity.bios_date.clone_from(&record.release_date);
    }
}

/// Fold a parsed type-1 record into the identity object.
fn fill_system(identity: &mut DmiIdentityJson, record: &Option<SystemInformationRecord>) {
    if let Some(record) = record {
        identity
            .system_manufacturer
            .clone_from(&record.manufacturer);
        identity.system_product.clone_from(&record.product_name);
        identity.system_serial.clone_from(&record.serial_number);
        identity.system_uuid.clone_from(&record.uuid);
        identity.system_sku.clone_from(&record.sku);
        identity.system_family.clone_from(&record.family);
    }
}

/// Fold a parsed type-2 record into the identity object.
fn fill_baseboard(identity: &mut DmiIdentityJson, record: &Option<BaseboardInformationRecord>) {
    if let Some(record) = record {
        identity.board_manufacturer.clone_from(&record.manufacturer);
        identity.board_product.clone_from(&record.product_name);
        identity.board_serial.clone_from(&record.serial_number);
        identity.board_asset_tag.clone_from(&record.asset_tag);
    }
}

/// Fold a parsed record into the contract module object. Optional fields
/// whose SMBIOS magnitude cannot fit the contract's `u32` become `None` (the
/// format's 15-bit size word cannot actually exceed it, so this is a guard,
/// not a reachable path).
fn module_json(slot: u32, record: &MemoryDeviceRecord) -> MemoryModuleJson {
    MemoryModuleJson {
        slot,
        size_mb: record.size_mb.and_then(|mb| u32::try_from(mb).ok()),
        speed_mts: record.speed_mts,
        configured_speed_mts: record.configured_speed_mts,
        manufacturer: record.manufacturer.clone(),
        serial_number: record.serial_number.clone(),
        part_number: record.part_number.clone(),
        form_factor: record.form_factor,
        memory_type: record.memory_type,
        locator: record.device_locator.clone(),
    }
}

/// Classify an entries-directory open failure: missing root → `no_dmi`;
/// `EACCES`/`EPERM` → `permission_denied`; anything else → `open_failed`.
fn classify_entries_error(error: &io::Error, root: &Path) -> WalkError {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => ErrorKindJson::NoDmi,
        io::ErrorKind::PermissionDenied => ErrorKindJson::PermissionDenied,
        _ => ErrorKindJson::OpenFailed,
    };
    WalkError {
        kind,
        detail: format!("open {}: {error}", root.display()),
    }
}

/// Classify a `17-N/raw` read failure: `EACCES`/`EPERM` →
/// `permission_denied` (the escalatable denial); anything else (missing file,
/// I/O error) → `read_failed`.
fn classify_raw_error(error: &io::Error, raw_path: &Path) -> WalkError {
    let kind = if error.kind() == io::ErrorKind::PermissionDenied {
        ErrorKindJson::PermissionDenied
    } else {
        ErrorKindJson::ReadFailed
    };
    WalkError {
        kind,
        detail: format!("read {}: {error}", raw_path.display()),
    }
}

#[cfg(test)]
#[path = "../tests/headless/smbios_helper_dmi_walk.rs"]
mod tests;

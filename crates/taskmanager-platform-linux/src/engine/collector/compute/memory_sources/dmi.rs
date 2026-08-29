//! Raw DMI and EDAC memory-module probing.

use std::fs;
use std::io;
use std::path::Path;

use taskmanager_core::core::failure::FailureKind;

use super::{
    DMI_DIMM_SIZE_FIELD, DMI_DIMM_SPEED_FIELD, DMI_EDAC_SIZE_FIELD, DMI_PROVIDER,
    DMI_SLOTS_TOTAL_FIELD, DMI_SLOTS_USED_FIELD, DMI_SPEED_FIELD, DmiMemoryObservation,
    FailureSummary, classify_io, classify_smbios_io, read_optional_u64, source_status,
};

/// Merge the udev-database facts into a raw-DMI observation: udev values win
/// (configured speed over maximum, exact slots), raw-DMI fills gaps only.
/// Provided fields lose their raw-DMI failure receipts, and a non-empty udev
/// contribution upgrades the source status (the udev database is
/// privilege-free and authoritative — an empty raw-DMI result must not mask
/// it). The provider identity stays `dmi` — udev is the mechanism by which the
/// DMI facts are obtained, not a separate capability. Pure so the precedence
/// rules are unit-testable without udevadm.
pub(super) fn observe_dmi_memory_from_paths(
    dmi_id_roots: [&Path; 2],
    dmi_entries_root: &Path,
    edac_root: &Path,
) -> DmiMemoryObservation {
    let mut failures = FailureSummary::default();
    let mut source_reached = false;
    let mut observed = 0usize;
    let mut speed_mhz = None;
    let mut slots_used = None;
    let mut slots_total = None;

    for root in dmi_id_roots {
        match fs::read_dir(root) {
            Ok(_) => {
                source_reached = true;
                for (speed_name, used_name, total_name) in [
                    (
                        "memory_speed_mhz",
                        "memory_slots_used",
                        "memory_slots_total",
                    ),
                    ("memory_speed", "slots_used", "slots_total"),
                ] {
                    if speed_mhz.is_none()
                        && let Some(value) = read_optional_u64(
                            &root.join(speed_name),
                            &mut failures,
                            DMI_SPEED_FIELD,
                        )
                    {
                        match u32::try_from(value) {
                            Ok(value) if value > 0 => {
                                speed_mhz = Some(value);
                                observed = observed.saturating_add(1);
                            }
                            Ok(_) => {}
                            Err(_) => {
                                failures.record_field(DMI_SPEED_FIELD, FailureKind::ProviderFault)
                            }
                        }
                    }
                    if slots_used.is_none()
                        && let Some(value) = read_optional_u64(
                            &root.join(used_name),
                            &mut failures,
                            DMI_SLOTS_USED_FIELD,
                        )
                    {
                        match usize::try_from(value) {
                            Ok(value) => {
                                slots_used = Some(value);
                                observed = observed.saturating_add(1);
                            }
                            Err(_) => failures
                                .record_field(DMI_SLOTS_USED_FIELD, FailureKind::ProviderFault),
                        }
                    }
                    if slots_total.is_none()
                        && let Some(value) = read_optional_u64(
                            &root.join(total_name),
                            &mut failures,
                            DMI_SLOTS_TOTAL_FIELD,
                        )
                    {
                        match usize::try_from(value) {
                            Ok(value) => {
                                slots_total = Some(value);
                                observed = observed.saturating_add(1);
                            }
                            Err(_) => failures
                                .record_field(DMI_SLOTS_TOTAL_FIELD, FailureKind::ProviderFault),
                        }
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => failures.record_field(DMI_SPEED_FIELD, classify_io(&error)),
        }
    }

    match fs::read_dir(dmi_entries_root) {
        Ok(entries) => {
            source_reached = true;
            let mut type17_count = 0usize;
            let mut type17_used = 0usize;
            let mut max_speed = 0u32;
            for entry in entries {
                let Ok(entry) = entry else {
                    failures.record_field(DMI_DIMM_SIZE_FIELD, FailureKind::ProviderFault);
                    failures.record_field(DMI_DIMM_SPEED_FIELD, FailureKind::ProviderFault);
                    continue;
                };
                if !entry.file_name().to_string_lossy().starts_with("17-") {
                    continue;
                }
                match fs::read(entry.path().join("raw")) {
                    Ok(bytes) if bytes.len() >= 23 => {
                        observed = observed.saturating_add(1);
                        type17_count = type17_count.saturating_add(1);
                        let size = u16::from_le_bytes([bytes[12], bytes[13]]);
                        if size > 0 && size != u16::MAX {
                            type17_used = type17_used.saturating_add(1);
                        }
                        let mut module_speed =
                            u32::from(u16::from_le_bytes([bytes[21], bytes[22]]));
                        if bytes.len() >= 34 {
                            let configured = u32::from(u16::from_le_bytes([bytes[32], bytes[33]]));
                            if configured > 0 && configured != u32::from(u16::MAX) {
                                module_speed = configured;
                            }
                        }
                        if module_speed > 0 && module_speed != u32::from(u16::MAX) {
                            max_speed = max_speed.max(module_speed);
                        }
                    }
                    Ok(_) => {
                        failures.record_field(DMI_DIMM_SIZE_FIELD, FailureKind::ProviderFault);
                        failures.record_field(DMI_DIMM_SPEED_FIELD, FailureKind::ProviderFault);
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        failures.record_field(DMI_DIMM_SIZE_FIELD, classify_smbios_io(&error));
                        failures.record_field(DMI_DIMM_SPEED_FIELD, classify_smbios_io(&error));
                    }
                }
            }
            if type17_count > 0 {
                slots_total.get_or_insert(type17_count);
                slots_used.get_or_insert(type17_used);
                if max_speed > 0 {
                    speed_mhz.get_or_insert(max_speed);
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            failures.record_field(DMI_DIMM_SIZE_FIELD, classify_io(&error));
            failures.record_field(DMI_DIMM_SPEED_FIELD, classify_io(&error));
        }
    }

    if slots_total.is_none() || slots_used.is_none() {
        match fs::read_dir(edac_root) {
            Ok(controllers) => {
                source_reached = true;
                let mut edac_total = 0usize;
                let mut edac_used = 0usize;
                for controller in controllers.flatten() {
                    if !controller.file_name().to_string_lossy().starts_with("mc") {
                        continue;
                    }
                    let dimms = match fs::read_dir(controller.path()) {
                        Ok(dimms) => dimms,
                        Err(error) => {
                            failures.record_field(DMI_EDAC_SIZE_FIELD, classify_io(&error));
                            continue;
                        }
                    };
                    for dimm in dimms.flatten() {
                        if !dimm.file_name().to_string_lossy().starts_with("dimm") {
                            continue;
                        }
                        edac_total = edac_total.saturating_add(1);
                        if let Some(size) = read_optional_u64(
                            &dimm.path().join("size"),
                            &mut failures,
                            DMI_EDAC_SIZE_FIELD,
                        ) {
                            observed = observed.saturating_add(1);
                            if size > 0 {
                                edac_used = edac_used.saturating_add(1);
                            }
                        }
                    }
                }
                if edac_total > 0 {
                    slots_total.get_or_insert(edac_total);
                    slots_used.get_or_insert(edac_used);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => failures.record_field(DMI_EDAC_SIZE_FIELD, classify_io(&error)),
        }
    }

    DmiMemoryObservation {
        speed_mhz,
        slots_used,
        slots_total,
        module_types: Vec::new(),
        module_manufacturers: Vec::new(),
        module_form_factors: Vec::new(),
        status: source_status(DMI_PROVIDER, observed, source_reached, &failures),
        receipts: failures.fields,
    }
}

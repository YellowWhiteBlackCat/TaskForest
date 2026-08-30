//! Udev-database memory facts (privilege-free DMI successor; ADR udev design).

use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

use super::super::memory_sources::{
    DMI_PROVIDER, DMI_SLOTS_TOTAL_FIELD, DMI_SLOTS_USED_FIELD, DMI_SPEED_FIELD,
    DmiMemoryObservation, UDEV_COMMAND_TIMEOUT, UdevMemoryModule,
};
use std::process::Command;
use taskmanager_platform_portable::run_with_timeout;

pub(super) fn merge_udev_into_dmi(
    observation: &mut DmiMemoryObservation,
    devices: &UdevMemoryDevices,
) {
    let mut provided_any = false;
    if observation.speed_mhz.is_none()
        && let Some(speed) = devices
            .modules
            .iter()
            .filter_map(|module| module.configured_speed_mts.or(module.speed_mts))
            .max()
    {
        observation.speed_mhz = Some(speed);
        provided_any = true;
        observation.receipts.remove(DMI_SPEED_FIELD);
    }
    if observation.slots_total.is_none() {
        observation.slots_total = Some(devices.slots_total);
        provided_any = true;
        observation.receipts.remove(DMI_SLOTS_TOTAL_FIELD);
    }
    let used = devices.modules.len();
    if observation.slots_used.is_none() && used > 0 {
        observation.slots_used = Some(used);
        provided_any = true;
        observation.receipts.remove(DMI_SLOTS_USED_FIELD);
    }
    let mut types = devices
        .modules
        .iter()
        .filter_map(|module| module.module_type.clone())
        .collect::<Vec<_>>();
    types.sort();
    types.dedup();
    observation.module_types = types;
    let mut manufacturers = devices
        .modules
        .iter()
        .filter_map(|module| module.manufacturer.clone())
        .collect::<Vec<_>>();
    manufacturers.sort();
    manufacturers.dedup();
    observation.module_manufacturers = manufacturers;
    let mut form_factors = devices
        .modules
        .iter()
        .filter_map(|module| module.form_factor.clone())
        .collect::<Vec<_>>();
    form_factors.sort();
    form_factors.dedup();
    observation.module_form_factors = form_factors;
    let mut part_numbers = devices
        .modules
        .iter()
        .filter_map(|module| module.part_number.clone())
        .collect::<Vec<_>>();
    part_numbers.sort();
    part_numbers.dedup();
    observation.module_part_numbers = part_numbers;
    let mut serials = devices
        .modules
        .iter()
        .filter_map(|module| module.serial_number.clone())
        .collect::<Vec<_>>();
    serials.sort();
    serials.dedup();
    observation.module_serials = serials;
    if provided_any {
        observation.status = SourceStatus {
            provider: ProviderId::borrowed(DMI_PROVIDER),
            outcome: SourceOutcome::Available,
            item_count: devices.modules.len(),
        };
    }
}
/// Query the udev database for the DMI memory-device properties. Returns
/// `None` when the command is missing, times out, or reports no `MEMORY_DEVICE`
/// properties (non-systemd udev, older builtin, or a system without DMI).
pub(super) fn observe_udev_memory_devices() -> Option<UdevMemoryDevices> {
    let output = run_with_timeout(
        Command::new("udevadm").args([
            "info",
            "-q",
            "property",
            "-p",
            "/sys/devices/virtual/dmi/id",
        ]),
        UDEV_COMMAND_TIMEOUT,
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_udev_memory_properties(&String::from_utf8_lossy(&output.stdout))
}

/// Parsed result of one `udevadm info` query.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct UdevMemoryDevices {
    pub modules: Vec<UdevMemoryModule>,
    /// `MEMORY_ARRAY_NUM_DEVICES`; falls back to the highest module index + 1.
    pub slots_total: usize,
}

/// A udev module label is programmed (worth surfacing) only when it carries
/// real content: the DMI "not specified" sentinels and empty strings are the
/// unprogrammed state, not a fact.
fn is_programmed_module_label(value: &str) -> bool {
    !value.is_empty() && value != "<OUT OF SPEC>" && value != "Not Specified" && value != "None"
}

/// An all-zero serial is the standard unprogrammed SPD state (never a real
/// module identity), so it stays an honest absence.
fn is_all_zero_serial(value: &str) -> bool {
    value.len() >= 4 && value.chars().all(|ch| ch == '0')
}

/// Parse `MEMORY_DEVICE_<n>_<PROP>=<value>` lines (the udev DMI database
/// properties). Present modules are `PRESENT=1`; absent slots are skipped
/// entirely. Returns `None` when no memory-device properties exist at all.
pub(crate) fn parse_udev_memory_properties(text: &str) -> Option<UdevMemoryDevices> {
    let mut devices = UdevMemoryDevices::default();
    let mut present = false;
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let Some(rest) = key.strip_prefix("MEMORY_DEVICE_") else {
            if key == "MEMORY_ARRAY_NUM_DEVICES"
                && let Ok(total) = value.trim().parse::<usize>()
            {
                devices.slots_total = total;
            }
            continue;
        };
        let (index, property) = rest.split_once('_')?;
        let index = index.parse::<usize>().ok()?;
        while devices.modules.len() <= index {
            devices.modules.push(UdevMemoryModule {
                present: true,
                size_mib: None,
                module_type: None,
                manufacturer: None,
                form_factor: None,
                part_number: None,
                serial_number: None,
                speed_mts: None,
                configured_speed_mts: None,
                rank: None,
                locator: None,
            });
        }
        present = true;
        let module = &mut devices.modules[index];
        let value = value.trim();
        match property {
            "PRESENT" => module.present = value == "1",
            "SIZE" => module.size_mib = value.parse().ok(),
            "TYPE" if !value.is_empty() && value != "<OUT OF SPEC>" => {
                module.module_type = Some(value.to_owned())
            }
            "MANUFACTURER" if !value.is_empty() && value != "Not Specified" => {
                module.manufacturer = Some(value.to_owned())
            }
            "FORM_FACTOR" if !value.is_empty() && value != "<OUT OF SPEC>" => {
                module.form_factor = Some(value.to_owned())
            }
            "PART_NUMBER" if is_programmed_module_label(value) => {
                module.part_number = Some(value.to_owned())
            }
            "SERIAL_NUMBER" if is_programmed_module_label(value) && !is_all_zero_serial(value) => {
                module.serial_number = Some(value.to_owned())
            }
            "SPEED_MTS" => module.speed_mts = value.parse().ok(),
            "CONFIGURED_SPEED_MTS" => module.configured_speed_mts = value.parse().ok(),
            "RANK" => module.rank = value.parse().ok(),
            "LOCATOR" => module.locator = Some(value.to_owned()),
            _ => {}
        }
    }
    if !present {
        return None;
    }
    devices.modules.retain(|module| module.present);
    if devices.slots_total == 0 {
        devices.slots_total = devices.modules.len();
    }
    Some(devices)
}

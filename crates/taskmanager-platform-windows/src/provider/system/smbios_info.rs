//! SMBIOS firmware and system hardware facts parser.
//!
//! Uses `smbioslib` to safely parse the raw DMTF SMBIOS table bytes provided
//! by `taskmanager_windows_api::raw_smbios_table`.

#[cfg(windows)]
use smbioslib::{
    SMBiosData, SMBiosInformation, SMBiosMemoryDevice, SMBiosPhysicalMemoryArray,
    SMBiosSystemInformation,
};
use taskmanager_core::FirmwareInfo;

/// Query and parse SMBIOS firmware and system facts.
#[cfg(windows)]
pub(super) fn query_firmware_info() -> Option<FirmwareInfo> {
    let raw_bytes = taskmanager_windows_api::raw_smbios_table().ok()?;
    let table_data = if raw_bytes.len() > 8 {
        raw_bytes[8..].to_vec()
    } else {
        raw_bytes
    };
    let smbios_data = SMBiosData::from_vec_and_version(table_data, None);

    let mut firmware_vendor = None;
    let mut firmware_version = None;
    let mut product_name = None;
    let mut product_version = None;
    let mut manufacturer = None;

    // Type 0: BIOS Information
    if let Some(bios_info) = smbios_data.find_map(|item: SMBiosInformation| Some(item)) {
        let vendor = bios_info.vendor().to_string().trim().to_string();
        if !vendor.is_empty() {
            firmware_vendor = Some(vendor);
        }
        let version = bios_info.version().to_string().trim().to_string();
        if !version.is_empty() {
            firmware_version = Some(version);
        }
    }

    // Type 1: System Information
    if let Some(sys_info) = smbios_data.find_map(|item: SMBiosSystemInformation| Some(item)) {
        let raw_manufacturer = sys_info.manufacturer().to_string();
        let trimmed = raw_manufacturer.trim();
        if !trimmed.is_empty() {
            manufacturer = Some(trimmed.to_string());
        }
        let product = sys_info.product_name().to_string().trim().to_string();
        if !product.is_empty() {
            product_name = Some(product);
        }
        let version = sys_info.version().to_string().trim().to_string();
        if !version.is_empty() {
            product_version = Some(version);
        }
    }

    let virtualization = super::virtualization::detect_virtualization(
        manufacturer.as_deref(),
        product_name.as_deref(),
    );

    if firmware_vendor.is_some()
        || firmware_version.is_some()
        || product_name.is_some()
        || product_version.is_some()
        || virtualization.is_some()
    {
        Some(FirmwareInfo {
            virtualization,
            product_name,
            product_version,
            firmware_vendor,
            firmware_version,
            ..FirmwareInfo::default()
        })
    } else {
        None
    }
}

/// Static physical memory hardware facts from SMBIOS Type 16 and Type 17.
#[derive(Debug, Clone, Default)]
pub(super) struct SmbiosMemoryFacts {
    pub speed_mhz: Option<u32>,
    pub slots_used: Option<usize>,
    pub slots_total: Option<usize>,
    pub module_type: Option<String>,
    pub module_manufacturer: Option<String>,
    pub module_form_factor: Option<String>,
    pub total_installed_bytes: Option<u64>,
}

/// Query and parse memory slots, speed, and module information from SMBIOS.
#[cfg(windows)]
pub(super) fn query_memory_hardware_info() -> Option<SmbiosMemoryFacts> {
    let raw_bytes = taskmanager_windows_api::raw_smbios_table().ok()?;
    let table_data = if raw_bytes.len() > 8 {
        raw_bytes[8..].to_vec()
    } else {
        raw_bytes
    };
    let smbios_data = SMBiosData::from_vec_and_version(table_data, None);

    let mut facts = SmbiosMemoryFacts::default();

    // Type 16: Physical Memory Array (slot totals)
    if let Some(array) = smbios_data.find_map(|item: SMBiosPhysicalMemoryArray| Some(item))
        && let Some(slots) = array.number_of_memory_devices()
        && slots > 0
    {
        facts.slots_total = Some(slots as usize);
    }

    // Type 17: Memory Device (individual memory sticks/slots)
    let mut slots_used = 0usize;
    let mut total_installed_bytes = 0u64;
    for device in smbios_data.collect::<SMBiosMemoryDevice>() {
        let size = device.size();
        let has_memory = match size {
            Some(smbioslib::MemorySize::Kilobytes(k)) if k > 0 => {
                total_installed_bytes = total_installed_bytes.saturating_add(u64::from(k) * 1024);
                true
            }
            Some(smbioslib::MemorySize::Megabytes(m)) if m > 0 => {
                total_installed_bytes =
                    total_installed_bytes.saturating_add(u64::from(m) * 1024 * 1024);
                true
            }
            Some(smbioslib::MemorySize::SeeExtendedSize) => match device.extended_size() {
                Some(smbioslib::MemorySizeExtended::Megabytes(m)) if m > 0 => {
                    total_installed_bytes =
                        total_installed_bytes.saturating_add(u64::from(m) * 1024 * 1024);
                    true
                }
                _ => false,
            },
            _ => false,
        };

        if has_memory {
            slots_used = slots_used.saturating_add(1);

            if facts.speed_mhz.is_none()
                && let Some(speed) = device.configured_memory_speed().or_else(|| device.speed())
                && let smbioslib::MemorySpeed::MTs(mts) = speed
                && mts > 0
            {
                facts.speed_mhz = Some(u32::from(mts));
            }

            if facts.module_form_factor.is_none()
                && let Some(form) = device.form_factor()
            {
                let form_str = format!("{:?}", form.value);
                if !form_str.is_empty() && form_str != "Unknown" {
                    facts.module_form_factor = Some(form_str);
                }
            }

            if facts.module_type.is_none()
                && let Some(mem_type) = device.memory_type()
            {
                let type_str = format!("{:?}", mem_type.value);
                if !type_str.is_empty() && type_str != "Unknown" {
                    facts.module_type = Some(type_str);
                }
            }

            if facts.module_manufacturer.is_none() {
                let mfg = device.manufacturer().to_string().trim().to_string();
                if !mfg.is_empty() && mfg != "Unknown" && mfg != "None" {
                    facts.module_manufacturer = Some(mfg);
                }
            }
        }
    }

    if slots_used > 0 {
        facts.slots_used = Some(slots_used);
        if facts.slots_total.is_none() || facts.slots_total < Some(slots_used) {
            facts.slots_total = Some(slots_used);
        }
    }
    if total_installed_bytes > 0 {
        facts.total_installed_bytes = Some(total_installed_bytes);
    }

    Some(facts)
}

#[cfg(not(windows))]
pub(super) fn query_firmware_info() -> Option<FirmwareInfo> {
    None
}

#[cfg(not(windows))]
pub(super) fn query_memory_hardware_info() -> Option<SmbiosMemoryFacts> {
    None
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_smbios_info.rs"]
mod tests;

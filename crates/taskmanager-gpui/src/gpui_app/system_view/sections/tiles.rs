//! Static hardware summary tiles for the System page.

use taskmanager_application::i18n;
use taskmanager_application::truncate_text;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_ui_contract::IconId;

/// One static hardware-parameter tile's data (the row beneath the hero card).
pub(crate) struct SystemTile {
    pub(crate) icon: IconId,
    pub(crate) title: String,
    pub(crate) value: String,
    pub(crate) note: String,
}

fn compact_cpu_label(value: Option<&str>) -> String {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return crate::gpui_app::formatting::missing_value();
    };
    let compact = value
        .replace("(R)", "")
        .replace("(TM)", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate_text(&compact, 28)
}

pub(crate) fn build_tiles(
    hw: &HardwareInfo,
    snap: &SystemSnapshot,
    units: UnitPreferences,
) -> Vec<SystemTile> {
    use crate::gpui_app::formatting;
    let cpu_value = compact_cpu_label(hw.cpu_brand.as_deref());
    let mut cpu_note = Vec::new();
    if let Some(physical) = snap.cpu.physical_cores {
        cpu_note.push(format!("{physical} {}", i18n::t("common.physical")));
    }
    if let Some(logical) = hw.cpu_cores.or(snap.cpu.logical_cores) {
        cpu_note.push(format!("{logical} {}", i18n::t("common.logical")));
    }
    if let Some(base) = hw.base_freq_mhz {
        cpu_note.push(formatting::format_ghz(base));
    }

    let memory_value = hw
        .total_memory_mb
        .and_then(|mb| mb.checked_mul(1024 * 1024))
        .map(|bytes| units.format_quantity(bytes, QuantityFamily::Memory, false))
        .unwrap_or_else(formatting::missing_value);
    let memory_note = snap
        .memory
        .current_module_type()
        .map(str::to_owned)
        .or_else(|| {
            snap.memory
                .current_speed_mhz()
                .filter(|mhz| *mhz > 0)
                .map(|mhz| format!("{mhz} MT/s"))
        })
        .unwrap_or_else(formatting::missing_value);

    let storage_count = snap.disks.len();
    let storage_value = match storage_count {
        0 => formatting::missing_value(),
        1 => snap.disks[0]
            .model
            .trim()
            .is_empty()
            .then(|| snap.disks[0].name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(formatting::missing_value),
        count => format!("{count} {}", i18n::t("common.disk")),
    };
    let storage_total = snap
        .disks
        .iter()
        .filter_map(|disk| disk.current_capacity_bytes())
        .fold(0_u64, u64::saturating_add);
    let storage_note = if storage_total > 0 {
        units.format_quantity(storage_total, QuantityFamily::Drive, false)
    } else {
        formatting::missing_value()
    };

    let graphics_count = snap.gpu.len();
    let graphics_value = snap
        .gpu
        .first()
        .map(|gpu| {
            gpu.marketing_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .or_else(|| (!gpu.brand.trim().is_empty()).then_some(gpu.brand.as_str()))
                .map(str::to_string)
                .unwrap_or_else(formatting::missing_value)
        })
        .unwrap_or_else(formatting::missing_value);
    let graphics_note = if graphics_count == 0 {
        formatting::missing_value()
    } else {
        let memory = snap.gpu.first().and_then(|gpu| {
            gpu.current_dedicated_vram_total_bytes()
                .or_else(|| gpu.current_memory_total_bytes())
        });
        match memory {
            Some(memory) => format!(
                "{graphics_count} {} · {}",
                i18n::t("common.gpu"),
                units.format_quantity(memory, QuantityFamily::Memory, false)
            ),
            None => format!("{graphics_count} {}", i18n::t("common.gpu")),
        }
    };
    vec![
        SystemTile {
            icon: IconId::Cpu,
            title: i18n::t("common.cpu").to_string(),
            value: cpu_value,
            note: if cpu_note.is_empty() {
                formatting::missing_value()
            } else {
                cpu_note.join(" · ")
            },
        },
        SystemTile {
            icon: IconId::Memory,
            title: i18n::t("common.memory").to_string(),
            value: memory_value,
            note: memory_note,
        },
        SystemTile {
            icon: IconId::Disk,
            title: i18n::t("system.storage").to_string(),
            value: storage_value,
            note: storage_note,
        },
        SystemTile {
            icon: IconId::Gpu,
            title: i18n::t("common.gpu").to_string(),
            value: graphics_value,
            note: graphics_note,
        },
    ]
}

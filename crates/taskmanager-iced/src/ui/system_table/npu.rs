//! Pure projection of discovered NPU facts for the System page.

use taskforest_core::core::npu::{NpuDevice, NpuInventorySnapshot};
use taskmanager_application::i18n;
use taskmanager_application::i18n::t;

use taskmanager_shell::presentation::observed_percentage;
use taskmanager_shell::presentation::{bytes, missing_value};

use super::SystemInfoRow;

/// Render-neutral facts for one discovered accelerator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NpuDeviceViewModel {
    pub(crate) title: String,
    pub(crate) rows: Vec<SystemInfoRow>,
}

impl NpuDeviceViewModel {
    fn from_device(device: &NpuDevice) -> Self {
        let title = format!("{} {}", t("npu.title"), device.device_id.as_str());
        let brand = device
            .brand
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map_or_else(missing_value, str::to_owned);
        let mut rows = vec![
            fact_row(t("npu.device_title"), brand),
            fact_row(
                t("common.driver"),
                device
                    .driver
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map_or_else(missing_value, str::to_owned),
            ),
            fact_row(
                t("common.utilization"),
                observed_percentage(device.utilization_pct.current_value().copied()),
            ),
        ];
        rows.extend(device.engines.iter().map(|engine| {
            fact_row(
                i18n::npu_engine_kind_label(engine.kind),
                observed_percentage(engine.utilization_pct.current_value().copied()),
            )
        }));
        rows.extend([
            fact_row(
                t("npu.dedicated_memory"),
                byte_count(device.memory.dedicated_total_bytes.current_value().copied()),
            ),
            fact_row(
                t("npu.shared_memory"),
                byte_count(device.memory.shared_total_bytes.current_value().copied()),
            ),
            fact_row(
                t("npu.sram"),
                byte_count(device.memory.sram_total_bytes.current_value().copied()),
            ),
        ]);
        Self { title, rows }
    }
}

/// Project only successful, real inventory devices. A failed request cannot
/// manufacture a display device; unavailable facts on a discovered device
/// remain visible as an honest missing-value marker.
pub(crate) fn npu_device_view_models(
    inventory: Option<&NpuInventorySnapshot>,
) -> Vec<NpuDeviceViewModel> {
    let Some(inventory) = inventory.filter(|inventory| inventory.is_success()) else {
        return Vec::new();
    };
    inventory
        .devices
        .iter()
        .map(NpuDeviceViewModel::from_device)
        .collect()
}

fn fact_row(label: &str, value: String) -> SystemInfoRow {
    SystemInfoRow {
        label: label.to_owned(),
        value,
    }
}

fn byte_count(value: Option<u64>) -> String {
    value.map_or_else(missing_value, bytes)
}

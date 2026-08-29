//! Pure projection of discovered NPU facts for the System page.

use taskmanager_application::i18n::t;
use taskmanager_core::core::npu::{NpuDevice, NpuEngineKind, NpuInventorySnapshot};

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
                percentage(device.utilization_pct.current_value().copied()),
            ),
        ];
        rows.extend(device.engines.iter().map(|engine| {
            fact_row(
                t(engine_label_key(engine.kind)),
                percentage(engine.utilization_pct.current_value().copied()),
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

const fn engine_label_key(kind: NpuEngineKind) -> &'static str {
    match kind {
        NpuEngineKind::Compute => "npu.engine_compute",
        NpuEngineKind::Matrix => "npu.engine_matrix",
        NpuEngineKind::Vector => "npu.engine_vector",
        NpuEngineKind::Video => "npu.engine_video",
        NpuEngineKind::Copy => "npu.engine_copy",
        NpuEngineKind::Unknown => "npu.engine_unknown",
    }
}

fn fact_row(label: &str, value: String) -> SystemInfoRow {
    SystemInfoRow {
        label: label.to_owned(),
        value,
    }
}

fn percentage(value: Option<f32>) -> String {
    value.map_or_else(missing_value, |value| format!("{:.0}%", value.round()))
}

fn byte_count(value: Option<u64>) -> String {
    value.map_or_else(missing_value, bytes)
}

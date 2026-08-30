//! Fixed System Hardware projection for discovered NPU devices.
//!
//! The inventory already owns identity, aggregate utilization, every reported
//! engine and split memory facts. This module materializes that complete set
//! once; the renderer has no selected engine or metric state.

use taskmanager_core::core::npu::{NpuDevice, NpuEngineKind, NpuInventorySnapshot};

use crate::gpui_app::formatting;
use taskmanager_application::i18n;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};

/// Render-neutral rows for one discovered accelerator.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NpuDeviceViewModel {
    identity: (String, String),
    utilization: (String, String),
    engines: Vec<(String, String)>,
    memory: Vec<(String, String)>,
}

impl NpuDeviceViewModel {
    fn from_device(device: &NpuDevice, units: UnitPreferences) -> Self {
        let prefix = format!("{} {}", i18n::t("npu.title"), device.device_id.as_str());
        let brand = device
            .brand
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| i18n::t("npu.device_title"));
        let driver = device
            .driver
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map_or_else(String::new, |driver| format!(" ({driver})"));
        let identity = (prefix.clone(), format!("{brand}{driver}"));
        let utilization = (
            qualified_label(&prefix, i18n::t("common.utilization")),
            percentage(device.utilization_pct.current_value().copied()),
        );
        let engines = device
            .engines
            .iter()
            .map(|engine| {
                (
                    qualified_label(&prefix, engine_label(engine.kind)),
                    percentage(engine.utilization_pct.current_value().copied()),
                )
            })
            .collect();
        let memory = [
            (
                "npu.dedicated_memory",
                device.memory.dedicated_total_bytes.current_value().copied(),
            ),
            (
                "npu.shared_memory",
                device.memory.shared_total_bytes.current_value().copied(),
            ),
        ]
        .into_iter()
        .filter_map(|(label_key, bytes)| {
            bytes.map(|bytes| {
                (
                    qualified_label(&prefix, i18n::t(label_key)),
                    units.format_quantity(bytes, QuantityFamily::Memory, false),
                )
            })
        })
        .collect();

        Self {
            identity,
            utilization,
            engines,
            memory,
        }
    }

    fn into_rows(self) -> impl Iterator<Item = (String, String)> {
        std::iter::once(self.identity)
            .chain(std::iter::once(self.utilization))
            .chain(self.engines)
            .chain(self.memory)
    }
}

pub(super) fn inventory_rows(
    inventory: Option<&NpuInventorySnapshot>,
    units: UnitPreferences,
) -> Vec<(String, String)> {
    let Some(inventory) = inventory.filter(|inventory| inventory.is_success()) else {
        return Vec::new();
    };
    inventory
        .devices
        .iter()
        .flat_map(|device| NpuDeviceViewModel::from_device(device, units).into_rows())
        .collect()
}

fn qualified_label(device: &str, fact: &str) -> String {
    format!("{device} · {fact}")
}

fn percentage(value: Option<f32>) -> String {
    value.map_or_else(formatting::missing_value, |value| {
        format!("{:.0}%", value.round())
    })
}

fn engine_label(kind: NpuEngineKind) -> &'static str {
    match kind {
        NpuEngineKind::Compute => i18n::t("npu.engine_compute"),
        NpuEngineKind::Matrix => i18n::t("npu.engine_matrix"),
        NpuEngineKind::Vector => i18n::t("npu.engine_vector"),
        NpuEngineKind::Video => i18n::t("npu.engine_video"),
        NpuEngineKind::Copy => i18n::t("npu.engine_copy"),
        NpuEngineKind::Unknown => i18n::t("npu.engine_unknown"),
    }
}

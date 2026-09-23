//! The per-NPU Performance-page detail panel for the Iced frontend.

use super::*;
use taskmanager_core::core::npu::{NpuDevice, NpuEngineKind};
use taskmanager_shell::presentation::{bytes, missing_value};
use taskmanager_shell::viewmodel::StatRow;

use super::super::responsive::PerformancePageBudget;
use taskmanager_theme::Theme;

pub(crate) fn npu_section(
    app: &crate::IcedApp,
    index: usize,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let inventory = app.shell.projection().npu_inventory.as_ref();
    let theme_snapshot = app.theme();

    let rows = match inventory {
        None => vec![tables::message_panel(
            theme_snapshot,
            t("common.collecting_telemetry"),
        )],
        Some(inv) if !inv.is_success() || inv.devices.is_empty() => vec![tables::message_panel(
            theme_snapshot,
            t("npu.engine_unknown"),
        )],
        Some(inv) => match inv.devices.get(index) {
            Some(device) => vec![npu_block(app, device, theme_snapshot, budget)],
            None => vec![tables::message_panel(
                theme_snapshot,
                t("npu.engine_unknown"),
            )],
        },
    };
    device_rows_panel(rows, theme_snapshot)
}

fn npu_block<'a>(
    _app: &'a crate::IcedApp,
    device: &'a NpuDevice,
    theme_snapshot: &'a Theme,
    budget: PerformancePageBudget,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let title = format!("{} {}", t("npu.title"), device.device_id.as_str());
    let brand = device
        .brand
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map_or_else(missing_value, str::to_owned);

    let mut stats = vec![
        StatRow::text(t("npu.device_title"), Some(brand)),
        StatRow::text(
            t("common.driver"),
            device.driver.as_deref().map(str::to_owned),
        ),
        StatRow::text(
            t("common.utilization"),
            device
                .utilization_pct
                .current_value()
                .map(|pct| format!("{pct:>5.1}%")),
        ),
        StatRow::text(
            t("npu.dedicated_memory"),
            device
                .memory
                .dedicated_total_bytes
                .current_value()
                .copied()
                .map(bytes),
        ),
        StatRow::text(
            t("npu.shared_memory"),
            device
                .memory
                .shared_total_bytes
                .current_value()
                .copied()
                .map(bytes),
        ),
        StatRow::text(
            t("npu.sram"),
            device
                .memory
                .sram_total_bytes
                .current_value()
                .copied()
                .map(bytes),
        ),
    ];

    for engine in &device.engines {
        let label = match engine.kind {
            NpuEngineKind::Compute => t("npu.engine_compute"),
            NpuEngineKind::Matrix => t("npu.engine_matrix"),
            NpuEngineKind::Vector => t("npu.engine_vector"),
            NpuEngineKind::Video => t("npu.engine_video"),
            NpuEngineKind::Copy => t("npu.engine_copy"),
            NpuEngineKind::Unknown => t("npu.engine_unknown"),
        };
        stats.push(StatRow::text(
            label,
            engine
                .utilization_pct
                .current_value()
                .map(|pct| format!("{pct:>5.1}%")),
        ));
    }

    perf_layout::main_with_stats(
        theme_snapshot,
        perf_layout::DetailHeader {
            title,
            subtitle: t("npu.device_title").to_string(),
            vital_line: None,
        },
        perf_layout::DetailBody {
            left: Vec::new(),
            stats,
            footer: None,
        },
        budget,
        perf_layout::DetailExtent::for_scroll_parent(budget.device_navigation),
    )
}

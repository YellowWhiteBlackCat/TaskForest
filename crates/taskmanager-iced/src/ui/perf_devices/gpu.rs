//! Performance page GPU detail block and series graph projection.

use super::*;
use iced::Element;
use taskmanager_core::core::metrics::{GpuMetrics, SystemSnapshot};

use taskmanager_shell::presentation::{
    bytes, device_status_i18n_key, gpu_display_identity, missing_value,
};
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_theme::tokens;

use super::super::responsive::{DeviceNavigationPresentation, PerformancePageBudget};

/// The Performance-page GPU panel readiness.
#[must_use]
pub(crate) fn gpu_section_state(snapshot: Option<&SystemSnapshot>) -> tables::ListState {
    match snapshot {
        None => tables::ListState::Loading,
        Some(snapshot) if snapshot.gpu.is_empty() => tables::ListState::Empty,
        Some(_) => tables::ListState::Ready,
    }
}

/// One GPU's display identity (GPUI `gpu_identity_text` parity): the
/// resolved product headline leads, else the neutral "GPU {index}" — never a
/// family prefix.
#[must_use]
pub(crate) fn gpu_title(gpu: &GpuMetrics, index: usize) -> String {
    gpu_display_identity(gpu)
        .headline
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{} {index}", t("common.gpu")))
}

/// The GPU page subtitle (GPUI parity): the adapter brand that qualifies the
/// resolved product; the kernel driver stays in its dedicated stats row.
#[must_use]
pub(crate) fn gpu_subtitle(gpu: &GpuMetrics) -> String {
    gpu_display_identity(gpu)
        .qualifier
        .unwrap_or_default()
        .to_owned()
}

/// The GPU page's undroppable one-line VRAM fact (GPUI `gpu_vram_vital_line`
/// parity): dedicated then shared used/total pairs, honest dashes for
/// uncollected halves.
#[must_use]
pub(crate) fn gpu_vram_vital_line(gpu: &GpuMetrics, units: UnitPrefs) -> String {
    let observed = super::projection::GpuObservation::from(gpu);
    let pair = |used: Option<u64>, total: Option<u64>| match (used, total) {
        (Some(used), Some(total)) => format!(
            "{} / {}",
            quantity_text_pref(used, units.use_bytes, units.use_base2),
            quantity_text_pref(total, units.use_bytes, units.use_base2),
        ),
        _ => missing_value(),
    };
    format!(
        "{} · {}",
        pair(
            observed.dedicated_vram_used_bytes,
            observed.dedicated_vram_total_bytes
        ),
        pair(
            observed.shared_vram_used_bytes,
            observed.shared_vram_total_bytes
        ),
    )
}

/// Project one GPU's honest scalar readouts as pre-folded shell [`StatRow`]s
/// (GPUI `gpu_stats` parity: one fold, three renderers). Headline facts whose
/// absence is a sampling gap (utilization) keep their row and render the
/// shared dash; facts that simply do not exist on this GPU family (power
/// draw, temperature, VRAM pairs, throttle reason) omit their rows entirely.
#[must_use]
pub(crate) fn gpu_summary_lines(gpu: &GpuMetrics) -> Vec<StatRow> {
    let observed = super::projection::GpuObservation::from(gpu);
    let mut rows = vec![
        StatRow::text(
            t("device.status"),
            Some(t(device_status_i18n_key(gpu.device_state.status)).to_string()),
        ),
        StatRow::text(
            t("common.utilization"),
            observed
                .utilization_pct
                .map(|value| format!("{:.0}%", value.round())),
        ),
    ];
    if let Some(name) = gpu
        .marketing_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        rows.push(StatRow::text(
            t("gpu.marketing_name"),
            Some(name.to_owned()),
        ));
    }
    // Graphics-API versions (GPUI parity).
    if let Some(api) = &gpu.graphics_api {
        if let Some(version) = api
            .opengl_version
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            rows.push(StatRow::text(
                t("gpu.opengl_version"),
                Some(version.to_owned()),
            ));
        }
        if let Some(version) = api
            .vulkan_version
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            rows.push(StatRow::text(
                t("gpu.vulkan_version"),
                Some(version.to_owned()),
            ));
        }
    }

    for (label, used, total) in [
        (
            t("gpu.dedicated_vram"),
            observed.dedicated_vram_used_bytes,
            observed.dedicated_vram_total_bytes,
        ),
        (
            t("gpu.shared_vram"),
            observed.shared_vram_used_bytes,
            observed.shared_vram_total_bytes,
        ),
    ] {
        if let Some(used) = used
            && let Some(total) = total
            && total > 0
        {
            rows.push(StatRow::pair(
                label,
                Some(format!("{} / {}", bytes(used), bytes(total))),
            ));
        }
    }
    if let Some(used) = observed.memory_used_bytes
        && let Some(total) = observed.memory_total_bytes
    {
        rows.push(StatRow::pair(
            t("gpu.vram"),
            Some(format!("{} / {}", bytes(used), bytes(total))),
        ));
    }

    if let Some(mhz) = observed.frequency_mhz {
        rows.push(StatRow::text(t("common.clock"), Some(format!("{mhz} MHz"))));
    }
    if let Some(mhz) = observed.max_frequency_mhz {
        rows.push(StatRow::text(
            t("gpu.max_clock"),
            Some(format!("{mhz} MHz")),
        ));
    }
    if let Some(value) = observed.idle_residency_pct {
        rows.push(StatRow::text(
            t("gpu.idle_residency"),
            Some(format!("{:.0}%", value.round())),
        ));
    }
    if let Some(value) = observed.temperature_c {
        rows.push(StatRow::text(
            t("common.temperature"),
            Some(format!("{:.0} \u{b0}C", value.round())),
        ));
    }
    if let Some(watts) = observed.power_w {
        rows.push(StatRow::text(
            t("common.power"),
            Some(format!("{watts:.1} W")),
        ));
    }
    if let Some(driver) = gpu.driver.as_deref() {
        rows.push(StatRow::text(t("common.driver"), Some(driver.to_string())));
    }
    // Driver version (registry DriverVersion / NVML sys version). Absent on
    // drivers that expose no versioned release — omit the row (GPUI parity).
    if let Some(version) = gpu
        .driver_version
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        rows.push(StatRow::text(
            t("gpu.driver_version"),
            Some(version.to_owned()),
        ));
    }

    for engine in &gpu.engines {
        if !engine.name.trim().is_empty() && engine.usage_pct.is_finite() {
            rows.push(StatRow::text(
                engine.name.clone(),
                Some(format!("{:.0}%", engine.usage_pct.round())),
            ));
        }
    }
    if let Some(reason) = observed.throttle_reason.filter(|reason| !reason.is_empty()) {
        rows.push(StatRow::text(t("gpu.throttling"), Some(reason)));
    }
    // PCI slot (GPUI parity).
    if let Some(slot) = gpu
        .pci_slot
        .as_deref()
        .filter(|slot| !slot.trim().is_empty())
    {
        rows.push(StatRow::text(t("gpu.pci_slot"), Some(slot.to_owned())));
    }
    rows
}

pub(crate) fn gpu_percent_readout(value: Option<f32>) -> String {
    value.map_or_else(missing_value, |value| format!("{:.0}%", value.round()))
}

pub(super) fn gpu_headline_label_value(
    metric: super::projection::GpuHeadlineMetric,
) -> (String, String) {
    use super::projection::{GpuHeadlineKind, GpuHeadlineValue};
    let missing = || missing_value();
    match metric.kind {
        GpuHeadlineKind::Utilization => (
            t("common.utilization").to_owned(),
            match metric.value {
                Some(GpuHeadlineValue::UtilizationPercent(value)) => {
                    gpu_percent_readout(Some(value))
                }
                _ => missing(),
            },
        ),
        GpuHeadlineKind::Temperature => (
            t("common.temperature").to_owned(),
            match metric.value {
                Some(GpuHeadlineValue::TemperatureC(value)) => {
                    format!("{:.0} °C", value.round())
                }
                _ => missing(),
            },
        ),
        GpuHeadlineKind::Frequency => (
            t("common.clock").to_owned(),
            match metric.value {
                Some(GpuHeadlineValue::FrequencyMhz(value)) => format!("{value} MHz"),
                _ => missing(),
            },
        ),
        GpuHeadlineKind::Power => (
            t("common.power").to_owned(),
            match metric.value {
                Some(GpuHeadlineValue::PowerW(value)) => format!("{value:.1} W"),
                _ => missing(),
            },
        ),
    }
}

fn gpu_headline_readouts(
    gpu: &GpuMetrics,
    theme_snapshot: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    perf_layout::headline_readouts(
        theme_snapshot,
        super::projection::gpu_headline_metrics(gpu)
            .into_iter()
            .map(gpu_headline_label_value),
    )
}

/// The Performance-page GPU panel: one block per GPU in the shared snapshot.
pub(crate) fn gpu_section(
    app: &crate::IcedApp,
    index: usize,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let snapshot = app.shell.projection().snapshot.as_ref();
    let theme_snapshot = app.theme();
    let color = crate::theme_binding::color(theme_snapshot.gpu);
    let compact = budget.device_navigation == DeviceNavigationPresentation::Strip;
    let rows = match (gpu_section_state(snapshot), snapshot) {
        (tables::ListState::Loading, _) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
        (tables::ListState::Empty, _) => {
            vec![tables::message_panel(theme_snapshot, t("gpu.empty"))]
        }
        (tables::ListState::Ready, Some(snapshot)) => match snapshot.gpu.get(index) {
            Some(gpu) => vec![gpu_block(GpuBlockProps {
                app,
                gpu,
                index,
                color,
                theme: theme_snapshot,
                compact,
                engine_rows: engine_rows_presentation(app, gpu),
                budget,
            })],
            None => vec![tables::message_panel(theme_snapshot, t("gpu.empty"))],
        },
        (tables::ListState::Ready, None) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
    };
    device_rows_panel(rows, theme_snapshot)
}

mod engine_graph;
use engine_graph::{GpuBlockProps, engine_rows_presentation, gpu_block};

/// Dedicated visual progress bars and percentage meters for Dedicated VRAM and Shared VRAM.
///
/// Each meter's bar renders through the shared `progress` component: a
/// measured ratio arrives as `Some(clamped)` and paints the accent tone; an
/// unobserved pair never reaches a bar at all (the meter stays absent —
/// unavailable data must not masquerade as a measured 0%).
pub(crate) fn gpu_vram_meters_panel<'a>(
    gpu: &'a GpuMetrics,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let observed = super::projection::GpuObservation::from(gpu);
    let mut bars: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();

    for (label, used_opt, total_opt) in [
        (
            t("gpu.dedicated_vram"),
            observed.dedicated_vram_used_bytes,
            observed.dedicated_vram_total_bytes,
        ),
        (
            t("gpu.shared_vram"),
            observed.shared_vram_used_bytes,
            observed.shared_vram_total_bytes,
        ),
    ] {
        if let (Some(used), Some(total)) = (used_opt, total_opt)
            && total > 0
        {
            let pct = (used as f32 / total as f32).clamp(0.0, 1.0);
            let used_str = bytes(used);
            let total_str = bytes(total);

            let header = iced::widget::row![
                iced::widget::text(label).size(f32::from(tokens::FONT_12)),
                iced::widget::text(format!("{used_str} / {total_str} ({:.1}%)", pct * 100.0))
                    .size(f32::from(tokens::FONT_11))
                    .style(move |_| iced::widget::text::Style {
                        color: Some(theme::muted_text_color(theme_snapshot)),
                    }),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);

            let progress_bar =
                components::progress(theme_snapshot, Some(pct), components::BadgeTone::Accent);

            bars.push(
                iced::widget::column![header, progress_bar]
                    .spacing(4)
                    .into(),
            );
        }
    }

    if bars.is_empty() {
        None
    } else {
        Some(
            iced::widget::container(iced::widget::column(bars).spacing(6))
                .padding(8)
                .style(move |_| theme::panel_style(theme_snapshot))
                .into(),
        )
    }
}

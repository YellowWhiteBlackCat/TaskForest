//! Performance page GPU detail block and series graph projection.

use super::*;
use iced::Element;
use taskmanager_application::{CapabilityId, GpuMetrics, SystemSnapshot};
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsAction, GpuEngineRowsPresentation, present_gpu_engine_rows,
};
use taskmanager_shell::presentation::{
    device_status_i18n_key, gpu_display_identity, missing_value,
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
    let color = theme::color(theme_snapshot.gpu);
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

struct GpuBlockProps<'a> {
    app: &'a crate::IcedApp,
    gpu: &'a GpuMetrics,
    index: usize,
    color: iced::Color,
    theme: &'a taskmanager_theme::Theme,
    compact: bool,
    engine_rows: GpuEngineRowsPresentation<'a>,
    budget: PerformancePageBudget,
}

fn gpu_block<'a>(props: GpuBlockProps<'a>) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let GpuBlockProps {
        app,
        gpu,
        index,
        color,
        theme: theme_snapshot,
        compact,
        engine_rows,
        budget,
    } = props;
    // The chart inventory comes from the typed frame budget (both axes); no
    // local compact-flag derivation remains.
    let chart_layout = super::projection::GpuChartLayout::for_inventory(budget.chart_inventory);
    // GPUI parity: no metric selector. The Full inventory REPLACES the
    // aggregate utilization headline with the complete engine inventory when
    // the GPU reports more than one engine; otherwise the utilization family
    // keeps the headline contract. Every other measured scalar family renders
    // simultaneously as a secondary chart below (also Full-gated).
    let engines: Vec<&taskmanager_application::GpuEngine> =
        chart_layout.engine_charts(gpu).collect();
    let mut graphs: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
    if chart_layout.shows_secondary_regions() && engines.len() > 1 {
        graphs.extend(gpu_engine_inventory(
            app,
            gpu,
            color,
            theme_snapshot,
            compact,
        ));
    } else {
        graphs.push(gpu_chart_metric_graph(
            app,
            gpu,
            taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric::Utilization,
            color,
            theme_snapshot,
            compact,
            true,
        ));
    }
    if compact {
        graphs.insert(0, gpu_headline_readouts(gpu, theme_snapshot));
    }
    // Secondary scalar families (Full inventory + availability + ≥1 finite
    // sample — GPUI `render_gpu_metric_graphs` parity).
    for metric in taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric::ALL {
        if metric == taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric::Utilization
        {
            continue;
        }
        let samples =
            app.cached_gpu_chart_metric_series(&gpu.device_id, gpu.device_generation.get(), metric);
        if !chart_layout.shows_secondary_regions()
            || !gpu_metric_available(gpu, metric)
            || !samples.iter().any(|value| value.is_finite())
        {
            continue;
        }
        graphs.push(gpu_chart_metric_graph(
            app,
            gpu,
            metric,
            color,
            theme_snapshot,
            false,
            false,
        ));
    }
    // The VRAM meters render whenever a pair is observed (GPUI never
    // budget-gates the VRAM facts — placement is the registered Iced 异).
    if let Some(vram_panel) = gpu_vram_meters_panel(gpu, theme_snapshot) {
        graphs.push(vram_panel);
    }
    let mut stats = gpu_summary_lines(gpu);
    match &engine_rows {
        GpuEngineRowsPresentation::Active(engines) => {
            if engines.is_empty() {
                stats.push(StatRow::text(
                    t("gpu.per_engine_title"),
                    Some(t("gpu.engines_none_reported").to_string()),
                ));
            }
            for engine in *engines {
                stats.push(StatRow::text(
                    engine.name.clone(),
                    Some(format!("{:.0}%", engine.utilization_pct.round())),
                ));
            }
        }
        presentation => {
            if let Some(message_key) = presentation.message_key()
                && !matches!(presentation, GpuEngineRowsPresentation::PermissionRequired)
            {
                stats.push(StatRow::text(
                    t("gpu.per_engine_title"),
                    Some(t(message_key).to_string()),
                ));
                if matches!(presentation, GpuEngineRowsPresentation::MissingDependency) {
                    stats.push(StatRow::text(
                        "",
                        Some(t("gpu.engines_install_hint").to_string()),
                    ));
                }
            }
        }
    }
    let stats_footer = super::device_status_footer(theme_snapshot, gpu.device_state.status);
    let block = perf_layout::main_with_stats(
        theme_snapshot,
        gpu_title(gpu, index),
        gpu_subtitle(gpu),
        // The undroppable one-line VRAM fact renders at every vertical rung
        // (GPUI `gpu_vram_vital_line` parity).
        Some(gpu_vram_vital_line(gpu, app.drive_units())),
        graphs,
        stats,
        stats_footer,
        budget,
        perf_layout::DetailExtent::Fill,
    );
    match engine_rows_toggle_section(theme_snapshot, engine_rows.action()) {
        Some(toggle) => iced::widget::column![block, toggle].spacing(8).into(),
        None => block,
    }
}

/// Whether one chartable scalar family is available for this GPU (the
/// shared shell availability projection — never a renderer-local guess).
fn gpu_metric_available(
    gpu: &GpuMetrics,
    metric: taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric,
) -> bool {
    taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetricAvailability::for_viewed_gpu(
        Some(gpu),
    )
    .is_available(metric)
}

/// One family's y-ceiling (GPUI `gpu_chart_metric_max` parity): percent
/// families keep the fixed 0–100 ladder; scalar families scale to their
/// window's finite peak with a sane floor so a lone sample still reads.
fn gpu_chart_metric_max(
    metric: taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric,
    samples: &[f32],
) -> f32 {
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetricUnit;
    match metric.unit() {
        GpuChartMetricUnit::Percent => 100.0,
        GpuChartMetricUnit::Watts => samples
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(10.0, f32::max),
        GpuChartMetricUnit::Celsius => samples
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(50.0, f32::max),
        GpuChartMetricUnit::Megahertz => samples
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(100.0, f32::max),
    }
}

/// The complete engine inventory that REPLACES the aggregate utilization
/// headline under the Full chart inventory (GPUI `render_gpu_engine_grid`
/// parity): the dominant engine — the first name containing "3d", else the
/// first reported — wears the full headline contract with a caption carrying
/// its live per-engine readout; every remaining engine renders as a compact
/// cell in a ≤3-column grid so a multi-engine card never clips its last row.
fn gpu_engine_inventory<'a>(
    app: &'a crate::IcedApp,
    gpu: &'a GpuMetrics,
    color: iced::Color,
    theme_snapshot: &'a taskmanager_theme::Theme,
    compact: bool,
) -> Vec<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let mut engines: Vec<&taskmanager_application::GpuEngine> =
        super::projection::GpuChartLayout::for_inventory(
            crate::ui::responsive::PerformanceChartInventory::Full,
        )
        .engine_charts(gpu)
        .collect();
    let Some(primary_index) = engines
        .iter()
        .position(|engine| engine.name.to_ascii_lowercase().contains("3d"))
        .or(if engines.is_empty() { None } else { Some(0) })
    else {
        return Vec::new();
    };
    let primary = engines.remove(primary_index);
    let primary_usage = primary
        .usage_pct
        .is_finite()
        .then(|| gpu_percent_readout(Some(primary.usage_pct)));
    let primary_samples =
        app.cached_gpu_engine_series(&gpu.device_id, gpu.device_generation.get(), &primary.name);
    let mut cards = vec![device_chart::device_mini_graph_fill(
        primary_samples,
        device_chart::DeviceMetricScale::Percent,
        color,
        format!("{}  {}", primary.name, primary_usage.unwrap_or_default()),
        theme_snapshot,
        compact,
        device_chart::GraphPrefs {
            smooth: true,
            max_override: None,
            hover: true,
        },
    )];
    if !engines.is_empty() {
        let cells: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = engines
            .iter()
            .map(|engine| {
                let usage = engine
                    .usage_pct
                    .is_finite()
                    .then(|| gpu_percent_readout(Some(engine.usage_pct)));
                device_chart::device_mini_graph_with_height(
                    app.cached_gpu_engine_series(
                        &gpu.device_id,
                        gpu.device_generation.get(),
                        &engine.name,
                    ),
                    device_chart::DeviceMetricScale::Percent,
                    color,
                    format!("{}  {}", engine.name, usage.unwrap_or_default()),
                    theme_snapshot,
                    device_chart::ENGINE_DEVICE_CHART_HEIGHT,
                    device_chart::GraphPrefs {
                        smooth: true,
                        max_override: None,
                        hover: true,
                    },
                )
            })
            .collect();
        // ≤3-column rows keep a multi-engine card readable (GPUI grid parity).
        let columns = cells.len().min(3);
        cards.push(crate::ui::chunked_rows(cells, columns));
    }
    cards
}

/// One chartable family's graph: the shared shell dispatch over the device's
/// live windows, with the scale of the family's unit — an unavailable family
/// yields its gaps (never a fabricated zero line). `headline` carries the
/// tier contract: the headline family fills the column's remaining height,
/// secondary families keep the shared secondary floor.
#[allow(clippy::too_many_arguments)]
fn gpu_chart_metric_graph<'a>(
    app: &'a crate::IcedApp,
    gpu: &GpuMetrics,
    metric: taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric,
    color: iced::Color,
    theme_snapshot: &taskmanager_theme::Theme,
    compact: bool,
    headline: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetricUnit;
    let scale = match metric.unit() {
        GpuChartMetricUnit::Percent => device_chart::DeviceMetricScale::Percent,
        GpuChartMetricUnit::Watts => device_chart::DeviceMetricScale::Watts,
        GpuChartMetricUnit::Celsius => device_chart::DeviceMetricScale::Celsius,
        GpuChartMetricUnit::Megahertz => device_chart::DeviceMetricScale::Megahertz,
    };
    let samples =
        app.cached_gpu_chart_metric_series(&gpu.device_id, gpu.device_generation.get(), metric);
    let prefs = device_chart::GraphPrefs {
        smooth: true,
        max_override: Some(gpu_chart_metric_max(metric, &samples)),
        hover: true,
    };
    let caption = t(metric.label_key()).to_string();
    if headline {
        device_chart::device_mini_graph_fill(
            samples,
            scale,
            color,
            caption,
            theme_snapshot,
            compact,
            prefs,
        )
    } else {
        device_chart::device_mini_graph_with_height(
            samples,
            scale,
            color,
            caption,
            theme_snapshot,
            device_chart::SECONDARY_DEVICE_CHART_HEIGHT,
            prefs,
        )
    }
}

fn engine_rows_presentation<'a>(
    app: &'a crate::IcedApp,
    gpu: &'a GpuMetrics,
) -> GpuEngineRowsPresentation<'a> {
    present_gpu_engine_rows(
        app.shell.gpu_engine_rows_state(),
        &taskmanager_application::DeviceId::new(gpu.device_id.clone()),
        app.shell
            .projection()
            .capability_status(&CapabilityId::TELEMETRY_GPU_ENGINES),
    )
}

fn engine_rows_toggle_section<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    action: GpuEngineRowsAction,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    match action {
        GpuEngineRowsAction::Disable => Some(crate::focus::button(
            theme_snapshot,
            crate::app::FocusTarget::GpuEngineRowsToggle,
            t("common.disable"),
            Message::ToggleGpuEngines,
            true,
        )),
        GpuEngineRowsAction::Enable => Some(
            iced::widget::row![
                crate::focus::ghost_button(
                    theme_snapshot,
                    crate::app::FocusTarget::GpuEngineRowsToggle,
                    t("gpu.enable_per_engine"),
                    Message::ToggleGpuEngines,
                ),
                iced::widget::text(t("gpu.engines_requires_auth")).size(f32::from(tokens::FONT_11)),
            ]
            .spacing(8)
            .into(),
        ),
        GpuEngineRowsAction::Reauthorize | GpuEngineRowsAction::Recheck => {
            Some(crate::focus::ghost_button(
                theme_snapshot,
                crate::app::FocusTarget::GpuEngineRowsToggle,
                t(match action {
                    GpuEngineRowsAction::Reauthorize => "gpu.engines_reauthorize",
                    _ => "gpu.engines_recheck",
                }),
                Message::ToggleGpuEngines,
            ))
        }
        GpuEngineRowsAction::None => None,
    }
}

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

//! GPU chart, engine inventory, and engine-toggle presentation.

use super::*;
use crate::IcedApp;
use crate::app::{FocusTarget, Message};
use crate::focus::{button, ghost_button};
use crate::ui::chunked_rows;
use crate::ui::device_chart::{
    DeviceMetricScale, ENGINE_DEVICE_CHART_HEIGHT, GraphPrefs, SECONDARY_DEVICE_CHART_HEIGHT,
    device_mini_graph_fill, device_mini_graph_with_height,
};
use crate::ui::perf_layout::{DetailExtent, main_with_stats};
use crate::ui::responsive::{PerformanceChartInventory, PerformancePageBudget};
use iced::widget::{column, row, text};
use iced::{Color, Element, Renderer};
use taskmanager_application::i18n::t;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{GpuEngine, GpuMetrics};
use taskmanager_platform_contract::CapabilityId;

use taskmanager_shell::presentation::gpu_chart_metric::{
    GpuChartMetric, GpuChartMetricAvailability, GpuChartMetricUnit,
};
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsAction, GpuEngineRowsPresentation, present_gpu_engine_rows,
};
use taskmanager_theme::{Theme, tokens};

use super::projection::GpuChartLayout;

type IcedTheme = iced::Theme;

pub(super) struct GpuBlockProps<'a> {
    pub(super) app: &'a IcedApp,
    pub(super) gpu: &'a GpuMetrics,
    pub(super) index: usize,
    pub(super) color: Color,
    pub(super) theme: &'a Theme,
    pub(super) compact: bool,
    pub(super) engine_rows: GpuEngineRowsPresentation<'a>,
    pub(super) budget: PerformancePageBudget,
}

pub(super) fn gpu_block<'a>(props: GpuBlockProps<'a>) -> Element<'a, Message, IcedTheme, Renderer> {
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
    let chart_layout = GpuChartLayout::for_inventory(budget.chart_inventory);
    // GPUI parity: no metric selector. The Full inventory REPLACES the
    // aggregate utilization headline with the complete engine inventory when
    // the GPU reports more than one engine; otherwise the utilization family
    // keeps the headline contract. Every other measured scalar family renders
    // simultaneously as a secondary chart below (also Full-gated).
    let engines: Vec<&GpuEngine> = chart_layout.engine_charts(gpu).collect();
    let mut graphs: Vec<Element<'a, Message, IcedTheme, Renderer>> = Vec::new();
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
            GpuChartMetric::Utilization,
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
    for metric in GpuChartMetric::ALL {
        if metric == GpuChartMetric::Utilization {
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
    if let Some(engines_panel) = gpu_engines_panel(app, gpu, &engine_rows, theme_snapshot) {
        graphs.push(engines_panel);
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
    let block = main_with_stats(
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
        DetailExtent::Fill,
    );
    match engine_rows_toggle_section(theme_snapshot, engine_rows.action()) {
        Some(toggle) => column![block, toggle].spacing(8).into(),
        None => block,
    }
}

/// Whether one chartable scalar family is available for this GPU (the
/// shared shell availability projection — never a renderer-local guess).
fn gpu_metric_available(gpu: &GpuMetrics, metric: GpuChartMetric) -> bool {
    GpuChartMetricAvailability::for_viewed_gpu(Some(gpu)).is_available(metric)
}

/// One family's y-ceiling (GPUI `gpu_chart_metric_max` parity): percent
/// families keep the fixed 0–100 ladder; scalar families scale to their
/// window's finite peak with a sane floor so a lone sample still reads.
fn gpu_chart_metric_max(metric: GpuChartMetric, samples: &[f32]) -> f32 {
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
    app: &'a IcedApp,
    gpu: &'a GpuMetrics,
    color: Color,
    theme_snapshot: &'a Theme,
    compact: bool,
) -> Vec<Element<'a, Message, IcedTheme, Renderer>> {
    let mut engines: Vec<&GpuEngine> =
        GpuChartLayout::for_inventory(PerformanceChartInventory::Full)
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
    let mut cards = vec![device_mini_graph_fill(
        primary_samples,
        DeviceMetricScale::Percent,
        color,
        format!("{}  {}", primary.name, primary_usage.unwrap_or_default()),
        theme_snapshot,
        compact,
        GraphPrefs {
            smooth: true,
            max_override: None,
            hover: true,
        },
    )];
    if !engines.is_empty() {
        let cells: Vec<Element<'a, Message, IcedTheme, Renderer>> = engines
            .iter()
            .map(|engine| {
                let usage = engine
                    .usage_pct
                    .is_finite()
                    .then(|| gpu_percent_readout(Some(engine.usage_pct)));
                device_mini_graph_with_height(
                    app.cached_gpu_engine_series(
                        &gpu.device_id,
                        gpu.device_generation.get(),
                        &engine.name,
                    ),
                    DeviceMetricScale::Percent,
                    color,
                    format!("{}  {}", engine.name, usage.unwrap_or_default()),
                    theme_snapshot,
                    ENGINE_DEVICE_CHART_HEIGHT,
                    GraphPrefs {
                        smooth: true,
                        max_override: None,
                        hover: true,
                    },
                )
            })
            .collect();
        // ≤3-column rows keep a multi-engine card readable (GPUI grid parity).
        let columns = cells.len().min(3);
        cards.push(chunked_rows(cells, columns));
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
    app: &'a IcedApp,
    gpu: &GpuMetrics,
    metric: GpuChartMetric,
    color: Color,
    theme_snapshot: &Theme,
    compact: bool,
    headline: bool,
) -> Element<'a, Message, IcedTheme, Renderer> {
    let scale = match metric.unit() {
        GpuChartMetricUnit::Percent => DeviceMetricScale::Percent,
        GpuChartMetricUnit::Watts => DeviceMetricScale::Watts,
        GpuChartMetricUnit::Celsius => DeviceMetricScale::Celsius,
        GpuChartMetricUnit::Megahertz => DeviceMetricScale::Megahertz,
    };
    let samples =
        app.cached_gpu_chart_metric_series(&gpu.device_id, gpu.device_generation.get(), metric);
    let prefs = GraphPrefs {
        smooth: true,
        max_override: Some(gpu_chart_metric_max(metric, &samples)),
        hover: true,
    };
    let caption = t(metric.label_key()).to_string();
    if headline {
        device_mini_graph_fill(
            samples,
            scale,
            color,
            caption,
            theme_snapshot,
            compact,
            prefs,
        )
    } else {
        device_mini_graph_with_height(
            samples,
            scale,
            color,
            caption,
            theme_snapshot,
            SECONDARY_DEVICE_CHART_HEIGHT,
            prefs,
        )
    }
}

pub(crate) fn engine_rows_presentation<'a>(
    app: &'a IcedApp,
    gpu: &'a GpuMetrics,
) -> GpuEngineRowsPresentation<'a> {
    present_gpu_engine_rows(
        app.shell.gpu_engine_rows_state(),
        &DeviceId::new(gpu.device_id.clone()),
        app.shell
            .projection()
            .capability_status(&CapabilityId::TELEMETRY_GPU_ENGINES),
    )
}

pub(super) fn engine_rows_toggle_section<'a>(
    theme_snapshot: &'a Theme,
    action: GpuEngineRowsAction,
) -> Option<Element<'a, Message, IcedTheme, Renderer>> {
    match action {
        GpuEngineRowsAction::Disable => Some(button(
            theme_snapshot,
            FocusTarget::GpuEngineRowsToggle,
            t("common.disable"),
            Message::ToggleGpuEngines,
            true,
        )),
        GpuEngineRowsAction::Enable => Some(
            row![
                ghost_button(
                    theme_snapshot,
                    FocusTarget::GpuEngineRowsToggle,
                    t("gpu.enable_per_engine"),
                    Message::ToggleGpuEngines,
                ),
                text(t("gpu.engines_requires_auth")).size(f32::from(tokens::FONT_11)),
            ]
            .spacing(8)
            .into(),
        ),
        GpuEngineRowsAction::Reauthorize | GpuEngineRowsAction::Recheck => Some(ghost_button(
            theme_snapshot,
            FocusTarget::GpuEngineRowsToggle,
            t(match action {
                GpuEngineRowsAction::Reauthorize => "gpu.engines_reauthorize",
                _ => "gpu.engines_recheck",
            }),
            Message::ToggleGpuEngines,
        )),
        GpuEngineRowsAction::None => None,
    }
}

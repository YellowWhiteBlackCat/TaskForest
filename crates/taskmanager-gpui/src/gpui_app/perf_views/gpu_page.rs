//! GPU performance-page composition: one aggregate headline, a fine engine
//! inventory, and a compact memory strip.

use std::cell::RefCell;
use std::rc::Rc;

#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{AnyElement, Div, ElementId, IntoElement, ParentElement, Styled, div, px};
use taskmanager_application::GpuEngineRowsState;
use taskmanager_telemetry_store::CorrelatedSystemTelemetryHistory;

use crate::gpui_app::elements;
use crate::gpui_app::formatting::{
    GraphUnit, PerformanceSettings, gpu_identity_text, missing_value,
};
use crate::gpui_app::graph::{GraphCacheHandle, GraphHover, GraphSettings};
use crate::gpui_app::history_samples::{gpu_engine_samples, gpu_engine_series_names};
use crate::gpui_app::perf_views::gpu_stats::{
    VramCompositionData, gpu_stats, vram_composition_data,
};
use crate::gpui_app::perf_views::layout::{
    ChartSpec, HeadlineSurface, PerfPageProps, perf_page, render_chart, stats_panel,
};
use crate::gpui_app::perf_views::smart_status::status_footer;
use crate::gpui_app::root::responsive::PerformanceChartInventory;
use taskmanager_application::i18n;
use taskmanager_core::core::metrics::{GpuMetrics, SystemSnapshot};
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_shell::presentation::gpu_chart_metric::{
    GpuChartMetric, GpuChartMetricAvailability, gpu_chart_metric_history,
};
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsPresentation, present_gpu_engine_rows,
};
use taskmanager_telemetry_store::live_graph::LiveGraphHistory;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

/// Fixed geometry used by the GPU lower-band fit check. These are allocation
/// bounds, not paint-time clipping dimensions: a group is admitted only when
/// its complete rows fit beside the aggregate headline and memory strip.
const GPU_TOP_CHROME_FLOOR: f32 = 72.0;
const GPU_HEADLINE_SECTION_FLOOR: f32 = 186.0;
const GPU_ENGINE_HEADER_HEIGHT: f32 = 20.0;
const GPU_ENGINE_ROW_HEIGHT: f32 = 68.0;
const GPU_ENGINE_ROW_GAP: f32 = 4.0;
const GPU_MEMORY_SECTION_HEIGHT: f32 = 94.0;
const GPU_LOWER_BAND_GAP: f32 = 8.0;
const GPU_BOTTOM_SAFETY: f32 = 32.0;
const MAX_GPU_ENGINE_ROWS: usize = 64;

/// Root-owned GPU UI state projected into one render call. The state remains
/// per-window; this props boundary only prevents the stateless renderer from
/// growing another independent argument for every GPU control family.
pub(crate) struct GpuRenderState<'a> {
    pub(crate) engine_session: &'a taskmanager_application::GpuEngineRowsState,
    pub(crate) engine_capability_status: Option<taskmanager_platform_contract::CapabilityStatus>,
    pub(crate) engine_device_id: taskmanager_core::core::identity::DeviceId,
    pub(crate) chart_layout: GpuChartLayout,
    pub(crate) performance: PerformanceSettings,
    pub(crate) budget: crate::gpui_app::root::responsive::PerformancePageBudget,
    pub(crate) graph_cache: GraphCacheHandle,
}

/// The complete GPU chart inventory selected by responsive layout alone.
///
/// This is deliberately not interaction state: a standard surface admits the
/// complete reported engine inventory subject to the shared height budget,
/// while a compact surface keeps one readable aggregate graph instead of
/// compressing engine cards below their minimum useful height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuChartLayout {
    AggregateOnly,
    EngineInventory,
}

impl GpuChartLayout {
    pub(crate) const fn for_chart_inventory(inventory: PerformanceChartInventory) -> Self {
        match inventory {
            PerformanceChartInventory::AggregateOnly => Self::AggregateOnly,
            PerformanceChartInventory::Full => Self::EngineInventory,
        }
    }

    const fn shows_engine_inventory(self) -> bool {
        matches!(self, Self::EngineInventory)
    }
}

pub(crate) fn gpu_percentage_readout(value: Option<f32>) -> String {
    value.map_or_else(missing_value, |percentage| {
        format!("{:.0}%", percentage.round())
    })
}

struct GpuEngineMiniGridProps<'a> {
    theme: &'a Theme,
    history: &'a CorrelatedSystemTelemetryHistory,
    metrics: &'a GpuMetrics,
    engine_session: &'a GpuEngineRowsState,
    engine_device_id: &'a taskmanager_core::core::identity::DeviceId,
    engine_capability_status: Option<taskmanager_platform_contract::CapabilityStatus>,
    graph_settings: GraphSettings,
    graph_cache: GraphCacheHandle,
    max_rows: Option<usize>,
}

fn render_gpu_engine_mini_grid(props: GpuEngineMiniGridProps<'_>) -> Option<AnyElement> {
    let GpuEngineMiniGridProps {
        theme,
        history,
        metrics,
        engine_session,
        engine_device_id,
        engine_capability_status,
        graph_settings,
        graph_cache,
        max_rows,
    } = props;
    let mut engine_names = gpu_engine_series_names(history, metrics);
    if let GpuEngineRowsPresentation::Active(engines) =
        present_gpu_engine_rows(engine_session, engine_device_id, engine_capability_status)
    {
        for engine in engines {
            if !engine_names.iter().any(|name| name == &engine.name) {
                engine_names.push(engine.name.clone());
            }
        }
        engine_names.sort_unstable();
    }
    if engine_names.is_empty() {
        return None;
    }

    let active_engines =
        match present_gpu_engine_rows(engine_session, engine_device_id, engine_capability_status) {
            GpuEngineRowsPresentation::Active(engines) => Some(engines),
            _ => None,
        };
    let columns = engine_names.len().clamp(1, 4);
    let visible_count = max_rows.map_or(engine_names.len(), |rows| {
        engine_names.len().min(rows.saturating_mul(columns))
    });
    if visible_count == 0 {
        return None;
    }
    let row_count = visible_count.div_ceil(columns);
    let mut grid = div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .w_full()
        .min_h(px(0.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_baseline()
                .justify_between()
                .child(
                    div()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                        .font_weight(taskmanager_ui::theme_binding::font_weight(
                            tokens::FONT_WEIGHT_BOLD,
                        ))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                        .child(i18n::t("gpu.per_engine_title")),
                )
                .child(
                    div()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                        .child(if visible_count == engine_names.len() {
                            visible_count.to_string()
                        } else {
                            format!("{visible_count} / {}", engine_names.len())
                        }),
                ),
        );
    for row_index in 0..row_count {
        let mut row = div()
            .flex()
            .flex_row()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_4,
            ))
            .flex_none()
            .h(px(GPU_ENGINE_ROW_HEIGHT))
            .w_full();
        for column_index in 0..columns {
            let index = row_index * columns + column_index;
            if index < visible_count {
                let name = &engine_names[index];
                let current = active_engines
                    .and_then(|engines| engines.iter().find(|engine| engine.name == *name))
                    .map(|engine| engine.utilization_pct)
                    .or_else(|| {
                        metrics
                            .engines
                            .iter()
                            .find(|engine| engine.name == *name)
                            .map(|engine| engine.usage_pct)
                    })
                    .filter(|value| value.is_finite());
                let samples = gpu_engine_samples(
                    &graph_cache,
                    history,
                    &metrics.device_id,
                    metrics.device_generation,
                    name,
                );
                let cell_label = format!("{name}  {}", gpu_percentage_readout(current));
                let cell = elements::mini_graph_cell(
                    theme,
                    (
                        ElementId::from("tm-gpu-engine-graph"),
                        format!("{}:{}", metrics.device_id, name),
                    ),
                    samples,
                    theme.gpu,
                    &cell_label,
                    graph_settings,
                    graph_cache.clone(),
                )
                .size_full();
                #[cfg(any(test, feature = "test-support"))]
                let cell = {
                    let debug_name = name.clone();
                    cell.debug_selector(move || format!("tm-perf-gpu-engine:{debug_name}"))
                };
                row = row.child(div().flex_1().min_w(px(0.0)).child(cell));
            } else {
                row = row.child(div().flex_1().min_w(px(0.0)));
            }
        }
        grid = grid.child(row);
    }
    #[cfg(any(test, feature = "test-support"))]
    let grid = grid.debug_selector(|| "tm-perf-gpu-engine-grid".to_string());

    Some(grid.into_any_element())
}

/// The GPU page's undroppable one-line VRAM fact: dedicated and shared
/// totals when the device reports them. Mirrors the VRAM composition
/// block's numbers so the two can never disagree.
fn gpu_vram_vital_line(
    vram: Option<&VramCompositionData>,
    units: UnitPreferences,
) -> Option<String> {
    let vram = vram?;
    let mut segments = vec![format!(
        "{} / {}",
        units.format_quantity(vram.dedicated_used, QuantityFamily::Memory, false),
        units.format_quantity(vram.dedicated_total, QuantityFamily::Memory, false),
    )];
    if vram.shared_total > 0 {
        segments.push(format!(
            "{} / {}",
            units.format_quantity(vram.shared_used, QuantityFamily::Memory, false),
            units.format_quantity(vram.shared_total, QuantityFamily::Memory, false),
        ));
    }
    Some(segments.join(" · "))
}

pub(crate) fn render_gpu(
    theme: &Theme,
    snap: &SystemSnapshot,
    live_graph: &LiveGraphHistory,
    i: usize,
    gpu_state: GpuRenderState<'_>,
    hover_slot: &Rc<RefCell<Option<GraphHover>>>,
) -> Div {
    // Keep the page's three visual groups in a fixed order: one large
    // utilization chart, the fine engine inventory, and the flat memory strip.
    let telemetry = live_graph.store();
    let graph_settings = gpu_state.performance.graph;
    let Some(g) = snap.gpu.get(i) else {
        return div();
    };
    let samples = gpu_chart_metric_history(
        live_graph,
        &g.device_id,
        g.device_generation.get(),
        GpuChartMetric::Utilization,
    );
    let availability = GpuChartMetricAvailability::for_viewed_gpu(Some(g));

    // Hardware identity and driver identity are distinct facts. A resolved
    // product such as "Arc B390" leads; the generic adapter brand qualifies
    // it, while the kernel driver remains in the dedicated stats row.
    let (title, subtitle) = gpu_identity_text(g, i);

    let stats = gpu_stats(g, gpu_state.performance.units);
    let vram_data = vram_composition_data(g);
    let lower_capacity = gpu_lower_band_capacity(gpu_state.budget.content_height);
    let memory_metric = gpu_memory_metric(&availability);
    let memory_graph =
        if gpu_state.budget.vertical.carries_below() && gpu_memory_fits(lower_capacity) {
            memory_metric.and_then(|metric| {
                render_gpu_memory_graph(
                    theme,
                    live_graph,
                    g,
                    metric,
                    graph_settings,
                    gpu_state.budget.vertical,
                    hover_slot,
                    gpu_state.graph_cache.clone(),
                )
            })
        } else {
            None
        };
    let engine_group = gpu_state
        .chart_layout
        .shows_engine_inventory()
        .then(|| {
            render_gpu_engine_mini_grid(GpuEngineMiniGridProps {
                theme,
                history: &telemetry.system_history,
                metrics: g,
                engine_session: gpu_state.engine_session,
                engine_device_id: &gpu_state.engine_device_id,
                engine_capability_status: gpu_state.engine_capability_status,
                graph_settings,
                graph_cache: gpu_state.graph_cache.clone(),
                max_rows: gpu_engine_row_budget(lower_capacity, memory_graph.is_some()),
            })
        })
        .flatten();
    let below = match (engine_group, memory_graph) {
        (None, None) => None,
        (Some(group), None) | (None, Some(group)) => Some(group),
        (Some(group), Some(memory)) => Some(
            div()
                .flex()
                .flex_col()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .child(group)
                .child(memory)
                .into_any_element(),
        ),
    };
    let headline = HeadlineSurface::Charts(vec![ChartSpec::headline(
        "main-graph",
        (ElementId::from("tm-perf-main-graph"), g.device_id.clone()),
        Rc::from(samples),
        theme.gpu,
        GraphUnit::Percent,
    )]);
    let footer = status_footer(theme, g.device_state.status);
    let main = perf_page(PerfPageProps {
        theme,
        title,
        subtitle,
        vital_line: gpu_vram_vital_line(vram_data.as_ref(), gpu_state.performance.units),
        header_extra: None,
        headline,
        below,
        stats: stats_panel(
            theme,
            stats,
            gpu_state.budget.details,
            gpu_state.budget.content_height,
        ),
        stats_footer: footer,
        hover_slot,
        graph_cache: gpu_state.graph_cache,
        graph_settings,
        budget: gpu_state.budget,
    });
    div().size_full().child(main)
}

/// Return the lower-band capacity after the title, vital line, and aggregate
/// headline have kept their minimum useful geometry. `None` is the legacy
/// no-frame budget, where the bounded inventory may use all available space.
fn gpu_lower_band_capacity(content_height: f32) -> Option<f32> {
    if content_height <= 0.0 {
        return None;
    }
    Some(
        (content_height - GPU_TOP_CHROME_FLOOR - GPU_HEADLINE_SECTION_FLOOR - GPU_BOTTOM_SAFETY)
            .max(0.0),
    )
}

fn gpu_memory_fits(lower_capacity: Option<f32>) -> bool {
    lower_capacity.is_none_or(|capacity| capacity >= GPU_MEMORY_SECTION_HEIGHT)
}

/// Prefer the aggregate GPU-memory series, then use a typed VRAM family when
/// a provider exposes only dedicated/shared memory counters. This keeps the
/// compact third group present for real GPU adapters such as the capture
/// fixture without inventing a total-memory value from unrelated facts.
fn gpu_memory_metric(availability: &GpuChartMetricAvailability) -> Option<GpuChartMetric> {
    [
        GpuChartMetric::Memory,
        GpuChartMetric::DedicatedMemory,
        GpuChartMetric::SharedMemory,
    ]
    .into_iter()
    .find(|metric| availability.is_available(*metric))
}

/// Derive the number of complete engine-grid rows that fit beside the memory
/// strip. A bounded loop avoids converting a potentially invalid floating
/// measurement directly into an integer; every iteration adds one complete
/// row, so a partial row is never admitted.
fn gpu_engine_row_budget(lower_capacity: Option<f32>, memory_visible: bool) -> Option<usize> {
    let mut remaining = lower_capacity?;
    if memory_visible {
        remaining -= GPU_MEMORY_SECTION_HEIGHT + GPU_LOWER_BAND_GAP;
    }
    let mut rows = 0_usize;
    let mut used = GPU_ENGINE_HEADER_HEIGHT;
    while rows < MAX_GPU_ENGINE_ROWS {
        let gap = if rows == 0 { 0.0 } else { GPU_ENGINE_ROW_GAP };
        let next = gap + GPU_ENGINE_ROW_HEIGHT;
        if used + next > remaining {
            break;
        }
        used += next;
        rows += 1;
    }
    Some(rows)
}

/// Render the one compact GPU-memory chart. It is deliberately independent of
/// the engine group: the current used/total pair controls whether this card
/// exists, while an empty history produces the shared collecting state rather
/// than a fabricated flat zero line.
#[allow(clippy::too_many_arguments)]
fn render_gpu_memory_graph(
    theme: &Theme,
    live_graph: &LiveGraphHistory,
    gpu: &GpuMetrics,
    metric: GpuChartMetric,
    graph_settings: GraphSettings,
    vertical: crate::gpui_app::root::responsive::PerformanceVerticalRunway,
    hover_slot: &Rc<RefCell<Option<GraphHover>>>,
    graph_cache: GraphCacheHandle,
) -> Option<AnyElement> {
    let samples = gpu_chart_metric_history(
        live_graph,
        &gpu.device_id,
        gpu.device_generation.get(),
        metric,
    );
    Some(
        render_chart(
            theme,
            ChartSpec::compact(
                "gpu-memory-graph",
                (
                    ElementId::from("tm-perf-compact-graph"),
                    gpu.device_id.clone(),
                ),
                i18n::t("gpu.graph_memory").to_string(),
                Rc::from(samples),
                theme.gpu,
                GraphUnit::Percent,
            ),
            graph_settings,
            vertical,
            hover_slot,
            graph_cache,
        )
        .into_any_element(),
    )
}

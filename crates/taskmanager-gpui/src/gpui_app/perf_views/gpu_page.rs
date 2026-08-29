//! GPU performance-page composition: headline graph, per-engine grid and the
//! dedicated/shared/total VRAM composition block.

use std::cell::RefCell;
use std::rc::Rc;

#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{
    AnyElement, Context, Div, ElementId, IntoElement, ParentElement, ScrollHandle, Styled, div, px,
};
use taskmanager_telemetry_store::CorrelatedSystemTelemetryHistory;

use crate::gpui_app::elements;
use crate::gpui_app::formatting::{
    DisplayUnits, GraphUnit, PerformanceSettings, UnitKind, gpu_identity_text, missing_value,
};
use crate::gpui_app::graph::{GraphHover, GraphSettings};
use crate::gpui_app::history_samples::{gpu_engine_samples, gpu_engine_series_names};
use crate::gpui_app::perf_views::gpu_engines_panel;
use crate::gpui_app::perf_views::gpu_stats::{
    VramCompositionData, gpu_stats, vram_composition_data,
};
use crate::gpui_app::perf_views::layout::{
    ChartSpec, HeadlineSurface, PerfPageProps, perf_page, render_chart, stats_panel,
};
use crate::gpui_app::perf_views::smart_status::status_footer;
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::responsive::PerformanceChartInventory;
use taskmanager_application::i18n;
use taskmanager_core::core::metrics::{GpuMetrics, SystemSnapshot};
use taskmanager_shell::presentation::gpu_chart_metric::{
    GpuChartMetric, GpuChartMetricAvailability, GpuChartMetricUnit, gpu_chart_metric_history,
};
use taskmanager_telemetry_store::live_graph::LiveGraphHistory;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

const VRAM_SUMMARY_LABEL_WIDTH: taskmanager_theme::Length = taskmanager_theme::Length(82.0);

/// Root-owned GPU UI state projected into one render call. The state remains
/// per-window; this props boundary only prevents the stateless renderer from
/// growing another independent argument for every GPU control family.
pub(crate) struct GpuRenderState<'a> {
    pub(crate) engine_session: &'a taskmanager_application::GpuEngineRowsState,
    pub(crate) engine_capability_status: Option<taskmanager_platform_contract::CapabilityStatus>,
    pub(crate) engine_device_id: taskmanager_core::core::identity::DeviceId,
    pub(crate) chart_layout: GpuChartLayout,
    pub(crate) performance: PerformanceSettings,
    pub(crate) stats_scroll: ScrollHandle,
    pub(crate) budget: crate::gpui_app::root::responsive::PerformancePageBudget,
}

/// The complete GPU chart inventory selected by responsive layout alone.
///
/// This is deliberately not interaction state: a standard surface always
/// renders every reported engine, while a compact surface keeps one readable
/// aggregate graph instead of compressing engine cards below their minimum
/// useful height.
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

    const fn shows_engine_inventory(self, engine_count: usize) -> bool {
        matches!(self, Self::EngineInventory) && engine_count > 1
    }
}

pub(crate) fn gpu_percentage_readout(value: Option<f32>) -> String {
    value.map_or_else(missing_value, |percentage| {
        format!("{:.0}%", percentage.round())
    })
}

fn vram_composition_block(
    theme: &Theme,
    vram: Option<&VramCompositionData>,
    units: DisplayUnits,
) -> Div {
    let Some(vram) = vram else {
        return div();
    };
    let dedicated_total = vram.dedicated_total;
    let dedicated_used = vram.dedicated_used;
    let shared_total = vram.shared_total;
    let shared_used = vram.shared_used;
    let total_capacity = vram.total_capacity;
    let total_used = vram.total_used;

    let mut segments = Vec::new();
    if dedicated_used > 0 {
        let pct = (dedicated_used as f32 / total_capacity as f32) * 100.0;
        segments.push((theme.gpu, pct.max(1.0)));
    }
    if shared_used > 0 {
        let pct = (shared_used as f32 / total_capacity as f32) * 100.0;
        segments.push((theme.accent, pct.max(1.0)));
    }
    let free_bytes = total_capacity.saturating_sub(total_used);
    if free_bytes > 0 {
        let pct = (free_bytes as f32 / total_capacity as f32) * 100.0;
        segments.push((theme.border, pct.max(1.0)));
    }

    let mut summary = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_4)
        .text_size(tokens::FONT_11)
        .child(vram_summary_row(
            theme,
            theme.gpu,
            "dedicated",
            i18n::t("gpu.dedicated_vram"),
            units.format(dedicated_used, UnitKind::Memory, false),
            units.format(dedicated_total, UnitKind::Memory, false),
        ));
    summary = summary.child(vram_summary_row(
        theme,
        theme.accent,
        "shared",
        i18n::t("gpu.shared_vram"),
        units.format(shared_used, UnitKind::Memory, false),
        units.format(shared_total, UnitKind::Memory, false),
    ));
    summary = summary.child(vram_summary_row(
        theme,
        theme.border,
        "total",
        i18n::t("gpu.vram_total"),
        units.format(total_used, UnitKind::Memory, false),
        units.format(total_capacity, UnitKind::Memory, false),
    ));

    let block =
        div()
            .flex()
            .flex_col()
            .gap(tokens::SPACE_6)
            .p(tokens::SPACE_8)
            .rounded(tokens::control_radius(theme))
            .border_1()
            .border_color(theme.border)
            .bg(theme.sidebar_card_bg)
            .child(
                // Horizontal segmented bar
                div()
                    .flex()
                    .h(px(8.0))
                    .w_full()
                    .rounded(tokens::control_radius(theme))
                    .overflow_hidden()
                    .children(segments.iter().map(|(color, pct)| {
                        div().h_full().w(gpui::relative(*pct / 100.0)).bg(*color)
                    })),
            )
            .child(summary);
    #[cfg(any(test, feature = "test-support"))]
    let block = block.debug_selector(|| "tm-gpu-vram-composition".to_string());
    block
}

fn vram_summary_row(
    theme: &Theme,
    color: taskmanager_theme::Color,
    _debug_name: &'static str,
    label: &str,
    used: String,
    total: String,
) -> Div {
    let row = div()
        .flex()
        .items_center()
        .gap(tokens::SPACE_4)
        .w_full()
        .min_w(px(0.0))
        .child(div().flex_none().size(px(6.0)).rounded_full().bg(color))
        .child(
            div()
                .flex_none()
                .w(VRAM_SUMMARY_LABEL_WIDTH)
                .min_w(px(0.0))
                .truncate()
                .text_color(theme.fg_dim)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_right()
                .text_color(theme.fg)
                .child(format!("{used} / {total}")),
        );
    #[cfg(any(test, feature = "test-support"))]
    let row = row.debug_selector(move || format!("tm-gpu-vram-row:{_debug_name}"));
    row
}

fn render_gpu_engine_grid(
    theme: &Theme,
    history: &CorrelatedSystemTelemetryHistory,
    metrics: &GpuMetrics,
    graph_settings: GraphSettings,
    vertical: crate::gpui_app::root::responsive::PerformanceVerticalRunway,
    hover_slot: &Rc<RefCell<Option<GraphHover>>>,
) -> Div {
    let mut engine_names = gpu_engine_series_names(history, metrics);
    if engine_names.is_empty() {
        return div();
    }

    let primary_index = engine_names
        .iter()
        .position(|name| name.to_ascii_lowercase().contains("3d"))
        .unwrap_or(0);
    let primary_name = engine_names.remove(primary_index);
    let primary_usage = metrics
        .engines
        .iter()
        .find(|engine| engine.name == primary_name)
        .map(|engine| engine.usage_pct)
        .filter(|value| value.is_finite());
    let primary_samples = gpu_engine_samples(
        history,
        &metrics.device_id,
        metrics.device_generation,
        &primary_name,
    );
    // The dominant engine wears the full headline contract — hover, first
    // frame state, value pill, summary row — identified by a caption that
    // carries the live per-engine readout.
    let primary_card = render_chart(
        theme,
        ChartSpec::headline(
            (
                ElementId::from("tm-gpu-main-engine-graph"),
                format!("{}:{primary_name}", metrics.device_id),
            ),
            (
                ElementId::from("tm-gpu-main-engine-graph"),
                format!("{}:{primary_name}", metrics.device_id),
            ),
            primary_samples,
            theme.gpu,
            GraphUnit::Percent,
        )
        .with_title(format!(
            "{primary_name}  {}",
            gpu_percentage_readout(primary_usage)
        )),
        graph_settings,
        vertical,
        hover_slot,
    );
    #[cfg(any(test, feature = "test-support"))]
    let primary_card = {
        let debug_name = primary_name.clone();
        primary_card.debug_selector(move || format!("tm-perf-gpu-engine:{debug_name}"))
    };
    let mut container = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .w_full()
        .h_full()
        .min_h(px(0.0))
        .child(primary_card);

    // Remaining engines retain readable fixed-height cells below the primary.
    if !engine_names.is_empty() {
        let cnt = engine_names.len();
        let cols = if cnt <= 3 { cnt } else { 3 };
        let g_rows = cnt.div_ceil(cols);
        let mut subgrid = div()
            .flex()
            .flex_col()
            .gap(tokens::SPACE_6)
            .w_full()
            .min_h(px(0.0));

        for r in 0..g_rows {
            let mut row = div().flex().gap(tokens::SPACE_6).h(px(90.0)).w_full();
            for c in 0..cols {
                let gi = r * cols + c;
                if gi >= cnt {
                    row = row.child(div().flex_1().min_h(px(0.0)));
                } else {
                    let name = &engine_names[gi];
                    let cur_usage = metrics
                        .engines
                        .iter()
                        .find(|e| e.name == *name)
                        .map(|e| e.usage_pct)
                        .filter(|value| value.is_finite());
                    let samples = gpu_engine_samples(
                        history,
                        &metrics.device_id,
                        metrics.device_generation,
                        name,
                    );
                    let cell_label = format!("{name}  {}", gpu_percentage_readout(cur_usage));
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
                    )
                    .size_full();
                    #[cfg(any(test, feature = "test-support"))]
                    let cell = {
                        let debug_name = name.clone();
                        cell.debug_selector(move || format!("tm-perf-gpu-engine:{debug_name}"))
                    };
                    row = row.child(div().flex_1().min_h(px(0.0)).child(cell));
                }
            }
            subgrid = subgrid.child(row);
        }
        container = container.child(subgrid);
    }
    #[cfg(any(test, feature = "test-support"))]
    let container = container.debug_selector(|| "tm-perf-gpu-engine-grid".to_string());

    container
}

/// The GPU page's undroppable one-line VRAM fact: dedicated and shared
/// totals when the device reports them. Mirrors the VRAM composition
/// block's numbers so the two can never disagree.
fn gpu_vram_vital_line(vram: Option<&VramCompositionData>, units: DisplayUnits) -> Option<String> {
    let vram = vram?;
    let mut segments = vec![format!(
        "{} / {}",
        units.format(vram.dedicated_used, UnitKind::Memory, false),
        units.format(vram.dedicated_total, UnitKind::Memory, false),
    )];
    if vram.shared_total > 0 {
        segments.push(format!(
            "{} / {}",
            units.format(vram.shared_used, UnitKind::Memory, false),
            units.format(vram.shared_total, UnitKind::Memory, false),
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
    cx: &mut Context<RootView>,
    hover_slot: &Rc<RefCell<Option<GraphHover>>>,
) -> Div {
    // The engine grid keeps reading the typed system history directly; the
    // chartable scalar families ride the same shell dispatch the Iced/TUI
    // shells consume.
    let telemetry = live_graph.store();
    let graph_settings = gpu_state.performance.graph;
    let Some(g) = snap.gpu.get(i) else {
        return div();
    };
    // The headline chart is the utilization family — no selector, no hidden
    // families. Every chartable family the device actually reports renders
    // as its own graph below the headline; a family the platform cannot
    // measure renders nothing at all (never a fabricated zero).
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
    // Stats footer: the device-status footer (when not Healthy) PLUS the
    // VRAM composition card and the per-engine GPU utilization card (when
    // supported).
    let mut footer_children: Vec<AnyElement> = Vec::new();
    if let Some(f) = status_footer(theme, g.device_state.status) {
        footer_children.push(f);
    }
    if vram_data.is_some() {
        footer_children.push(
            vram_composition_block(theme, vram_data.as_ref(), gpu_state.performance.units)
                .into_any_element(),
        );
    }
    if gpu_engines_panel::panel_is_visible(
        gpu_state.engine_session,
        &gpu_state.engine_device_id,
        gpu_state.engine_capability_status,
    ) {
        footer_children.push(
            gpu_engines_panel::render_gpu_engines_panel(
                theme,
                i,
                gpu_state.engine_session,
                &gpu_state.engine_device_id,
                gpu_state.engine_capability_status,
                cx,
            )
            .into_any_element(),
        );
    }
    let footer = if footer_children.is_empty() {
        None
    } else {
        Some(
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_6)
                .children(footer_children)
                .into_any_element(),
        )
    };
    let engine_names = gpu_engine_series_names(&telemetry.system_history, g);
    let headline = if gpu_state
        .chart_layout
        .shows_engine_inventory(engine_names.len())
    {
        HeadlineSurface::Custom(
            render_gpu_engine_grid(
                theme,
                &telemetry.system_history,
                g,
                graph_settings,
                gpu_state.budget.vertical,
                hover_slot,
            )
            .into_any_element(),
        )
    } else {
        HeadlineSurface::Charts(vec![ChartSpec::headline(
            "main-graph",
            (ElementId::from("tm-perf-main-graph"), g.device_id.clone()),
            // The utilization family's generation-scoped window arrives as an
            // owned Vec; moving it into `Rc` hands the buffer to the graph
            // without a second copy.
            Rc::from(samples),
            theme.gpu,
            GraphUnit::Percent,
        )])
    };
    let main = perf_page(PerfPageProps {
        theme,
        stats_scroll: gpu_state.stats_scroll,
        title,
        subtitle,
        vital_line: gpu_vram_vital_line(vram_data.as_ref(), gpu_state.performance.units),
        header_extra: None,
        headline,
        below: render_gpu_metric_graphs(
            theme,
            live_graph,
            g,
            &availability,
            graph_settings,
            gpu_state.budget,
            hover_slot,
        ),
        stats: stats_panel(theme, stats),
        stats_footer: footer,
        hover_slot,
        graph_settings,
        budget: gpu_state.budget,
    });
    div().size_full().child(main)
}

/// Every chartable family the viewed GPU reports, as its own secondary chart
/// beneath the headline (no selector — the product contract is "show all
/// measured content, hide what is not read"). Utilization is the headline's
/// own family and never repeats here; a family absent from the device's
/// latest typed point, whose window holds no finite sample, or when the
/// chart-inventory budget keeps only the aggregate, renders nothing.
fn render_gpu_metric_graphs(
    theme: &Theme,
    live_graph: &LiveGraphHistory,
    gpu: &GpuMetrics,
    availability: &GpuChartMetricAvailability,
    graph_settings: GraphSettings,
    budget: crate::gpui_app::root::responsive::PerformancePageBudget,
    hover_slot: &Rc<RefCell<Option<GraphHover>>>,
) -> Option<AnyElement> {
    use crate::gpui_app::root::responsive::PerformanceChartInventory;
    if budget.chart_inventory != PerformanceChartInventory::Full {
        return None;
    }
    let mut cards: Vec<AnyElement> = Vec::new();
    for metric in GpuChartMetric::ALL {
        if metric == GpuChartMetric::Utilization || !availability.is_available(metric) {
            continue;
        }
        let samples = gpu_chart_metric_history(
            live_graph,
            &gpu.device_id,
            gpu.device_generation.get(),
            metric,
        );
        if !samples.iter().any(|value| value.is_finite()) {
            continue;
        }
        let unit = match metric.unit() {
            GpuChartMetricUnit::Percent => crate::gpui_app::formatting::GraphUnit::Percent,
            GpuChartMetricUnit::Watts => crate::gpui_app::formatting::GraphUnit::Watts,
            GpuChartMetricUnit::Celsius => crate::gpui_app::formatting::GraphUnit::Temperature,
            GpuChartMetricUnit::Megahertz => crate::gpui_app::formatting::GraphUnit::Megahertz,
        };
        let max = gpu_chart_metric_max(metric.unit(), &samples);
        let stem = metric.id_stem();
        cards.push(
            render_chart(
                theme,
                ChartSpec::secondary(
                    ElementId::Name(format!("gpu-{stem}-graph").into()),
                    (
                        ElementId::from("tm-perf-secondary-graph"),
                        format!("{}:{stem}", gpu.device_id),
                    ),
                    i18n::t(metric.label_key()).to_string(),
                    Rc::from(samples),
                    theme.gpu,
                    unit,
                )
                .with_max(max),
                graph_settings,
                budget.vertical,
                hover_slot,
            )
            .into_any_element(),
        );
    }
    (!cards.is_empty()).then(|| {
        div()
            .flex()
            .flex_col()
            .gap(tokens::SPACE_8)
            .children(cards)
            .into_any_element()
    })
}

/// The headline graph's y-ceiling for one scalar family: percent families
/// keep the fixed 0–100 ladder, scalar families scale to their window's
/// finite peak with a sane floor so a lone sample still reads.
fn gpu_chart_metric_max(unit: GpuChartMetricUnit, samples: &[f32]) -> f32 {
    match unit {
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

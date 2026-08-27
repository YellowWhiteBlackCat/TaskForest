//! GPU performance-page composition: headline graph, per-engine grid and the
//! dedicated/shared/total VRAM composition block.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, Div, ElementId, InteractiveElement, IntoElement, ParentElement,
    ScrollHandle, Styled, div, px,
};
use taskmanager_telemetry_store::CorrelatedSystemTelemetryHistory;

use crate::core::metrics::{GpuMetrics, SystemSnapshot};
use crate::gpui_app::elements;
use crate::gpui_app::formatting::{
    DisplayUnits, GraphUnit, PerformanceSettings, UnitKind, gpu_identity_text, missing_value,
};
use crate::gpui_app::graph::{
    GraphHover, GraphOpts, GraphSettings, graph_element, latest_samples_rc,
    latest_samples_rc_for_slide,
};
use crate::gpui_app::history_samples::{gpu_engine_samples, gpu_engine_series_names};
use crate::gpui_app::perf_views::gpu_engines_panel;
use crate::gpui_app::perf_views::gpu_stats::{
    VramCompositionData, gpu_stats, vram_composition_data,
};
use crate::gpui_app::perf_views::graph_summary_row;
use crate::gpui_app::perf_views::layout::{
    MainColumnLayout, MainContent, MainWithStatsProps, main_with_stats,
};
use crate::gpui_app::perf_views::smart_status::status_footer;
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::responsive::PerformanceChartInventory;
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;
use taskmanager_shell::history::LiveGraphHistory;
use taskmanager_shell::presentation::gpu_chart_metric::{
    GpuChartMetric, GpuChartMetricChoiceState, GpuChartMetricProjection, GpuChartMetricUnit,
    gpu_chart_metric_history,
};

const VRAM_SUMMARY_LABEL_WIDTH: crate::gpui_app::theme::Length =
    crate::gpui_app::theme::Length(82.0);

/// Root-owned GPU UI state projected into one render call. The state remains
/// per-window; this props boundary only prevents the stateless renderer from
/// growing another independent argument for every GPU control family.
pub(crate) struct GpuRenderState<'a> {
    pub(crate) engine_session: &'a taskmanager_application::GpuEngineRowsState,
    pub(crate) engine_capability_status: Option<taskmanager_application::CapabilityStatus>,
    pub(crate) engine_device_id: taskmanager_application::DeviceId,
    pub(crate) chart_layout: GpuChartLayout,
    pub(crate) performance: PerformanceSettings,
    pub(crate) left_scroll: ScrollHandle,
    pub(crate) stats_scroll: ScrollHandle,
    /// The shared chart-metric selector projection (ADR-034 stage 2): the
    /// pill row and the headline graph consume exactly this projection.
    pub(crate) chart_metric: GpuChartMetricProjection,
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
    color: crate::gpui_app::theme::Color,
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
    let primary_samples = if graph_settings.sliding_graphs {
        latest_samples_rc_for_slide(primary_samples, graph_settings.data_points)
    } else {
        latest_samples_rc(primary_samples, graph_settings.data_points)
    };
    let primary_summary = graph_summary_row(theme, &primary_samples, &|value| {
        format!("{:.0}%", value.round())
    });
    let primary_card = elements::graph_card(
        theme,
        graph_element(
            (
                ElementId::from("tm-gpu-main-engine-graph"),
                format!("{}:{primary_name}", metrics.device_id),
            ),
            Rc::clone(&primary_samples),
            theme.gpu.into(),
            GraphOpts {
                gradient_fill: true,
                ref_lines: true,
                value_badge: true,
                badge_fmt: Some(crate::gpui_app::perf_views::badge_pct),
                ..GraphOpts::default()
            }
            .with_settings(graph_settings),
        ),
    )
    .flex_1()
    .min_h(px(140.0))
    .child(
        div()
            .absolute()
            .top(px(6.0))
            .left(px(8.0))
            .flex()
            .items_baseline()
            .gap(tokens::SPACE_6)
            .child(
                div()
                    .text_size(tokens::FONT_13)
                    .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                    .text_color(theme.fg)
                    .child(primary_name.clone()),
            )
            .child(
                div()
                    .text_size(tokens::FONT_14)
                    .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                    .text_color(theme.fg_dim)
                    .child(gpu_percentage_readout(primary_usage)),
            ),
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
    if let Some(summary) = primary_summary {
        #[cfg(any(test, feature = "test-support"))]
        let summary = summary.debug_selector(|| "tm-perf-gpu-primary-engine-summary".to_string());
        container = container.child(summary);
    }

    // Remaining engines retain readable fixed-height cards below the primary.
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
                    let graph_samples = if graph_settings.sliding_graphs {
                        latest_samples_rc_for_slide(samples, graph_settings.data_points)
                    } else {
                        latest_samples_rc(samples, graph_settings.data_points)
                    };
                    let cell_label = format!("{name}  {}", gpu_percentage_readout(cur_usage));
                    let cell = elements::graph_card(
                        theme,
                        graph_element(
                            (
                                ElementId::from("tm-gpu-engine-graph"),
                                format!("{}:{}", metrics.device_id, name),
                            ),
                            graph_samples,
                            theme.gpu.into(),
                            GraphOpts {
                                gradient_fill: true,
                                ..GraphOpts::default()
                            }
                            .with_settings(graph_settings),
                        ),
                    )
                    .size_full()
                    .child(
                        div()
                            .absolute()
                            .top(px(4.0))
                            .left(px(6.0))
                            .text_size(tokens::FONT_10)
                            .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                            .text_color(theme.fg_dim)
                            .child(cell_label),
                    );
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

pub(crate) fn render_gpu(
    theme: &Theme,
    snap: &SystemSnapshot,
    live_graph: &LiveGraphHistory,
    i: usize,
    gpu_state: GpuRenderState<'_>,
    cx: &mut Context<RootView>,
    hover_slot: &Rc<RefCell<Option<GraphHover>>>,
) -> Div {
    // The engine grid keeps reading the typed system history directly; only
    // the headline chart-metric sampling had a diverging second fold, and it
    // now rides the same shell dispatch the Iced/TUI shells consume.
    let telemetry = live_graph.store();
    let graph_settings = gpu_state.performance.graph;
    let Some(g) = snap.gpu.get(i) else {
        return div();
    };
    let selected = gpu_state.chart_metric.selected;
    let samples = gpu_chart_metric_history(
        live_graph,
        &g.device_id,
        g.device_generation.get(),
        selected,
    );

    // Hardware identity and driver identity are distinct facts. A resolved
    // product such as "Arc B390" leads; the generic adapter brand qualifies
    // it, while the kernel driver remains in the dedicated stats row.
    let (title, subtitle) = gpu_identity_text(g, i);

    let stats = gpu_stats(g, gpu_state.performance.units);
    let vram_data = vram_composition_data(g);
    let graph_unit = match selected.unit() {
        GpuChartMetricUnit::Percent => GraphUnit::Percent,
        GpuChartMetricUnit::Watts => GraphUnit::Watts,
        GpuChartMetricUnit::Celsius => GraphUnit::Temperature,
        GpuChartMetricUnit::Megahertz => GraphUnit::Megahertz,
    };
    let graph_max = gpu_chart_metric_max(selected.unit(), &samples);
    // Stats footer: the device-status footer (when not Healthy) PLUS the
    // per-engine GPU utilization card (when supported).
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
    let main_content = if gpu_state
        .chart_layout
        .shows_engine_inventory(engine_names.len())
    {
        MainContent::EngineInventory(
            render_gpu_engine_grid(theme, &telemetry.system_history, g, graph_settings)
                .into_any_element(),
        )
    } else {
        MainContent::AggregateGraph
    };
    let selector = render_gpu_chart_metric_selector(theme, i, &gpu_state.chart_metric, cx);
    let main = main_with_stats(MainWithStatsProps {
        theme,
        left_scroll: gpu_state.left_scroll,
        stats_scroll: gpu_state.stats_scroll,
        title,
        subtitle,
        graph_id: (ElementId::from("tm-perf-main-graph"), g.device_id.clone()).into(),
        // The selected family's generation-scoped window arrives as an owned
        // Vec; moving it into `Rc` hands the buffer to the graph without a
        // second copy.
        graph_samples: Rc::from(samples),
        graph_color: theme.gpu,
        graph_opts: GraphOpts {
            max: graph_max,
            ..GraphOpts::default()
        },
        graph_settings,
        graph_unit,
        graph_dual: None,
        main_content,
        stats,
        stats_footer: footer,
        main_column: MainColumnLayout::Viewport,
        left_footer: None,
        hover_slot,
        graph_controls: Some(selector.into_any_element()),
    });
    div().size_full().child(main)
}

/// The headline graph's y-ceiling for one family (ADR-034 stage 2):
/// percent families keep the fixed 0–100 ladder, scalar families scale to
/// their window's finite peak with a sane floor so a lone sample still reads.
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

/// The chart-metric choice pill row (ADR-034 stage 2): one focusable
/// [`elements::Pill`] per family projected from the shared selection.
/// Available families are keyboard tab-stops with pointer + Enter/Space
/// activation through the shell's gate; unavailable families stay visible,
/// dimmed, and outside the focus order — the projection of the same
/// boundary that rejects the selection, not a second rule.
fn render_gpu_chart_metric_selector(
    theme: &Theme,
    gpu_index: usize,
    projection: &GpuChartMetricProjection,
    cx: &mut Context<RootView>,
) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(tokens::SPACE_4)
        .debug_selector(|| "tm-gpu-chart-metric-selector".to_string());
    for choice in projection.choices {
        let available = choice.state != GpuChartMetricChoiceState::Unavailable
            && choice.state != GpuChartMetricChoiceState::SelectedUnavailable;
        let active = choice.state == GpuChartMetricChoiceState::Selected
            || choice.state == GpuChartMetricChoiceState::SelectedUnavailable;
        let stem = choice.metric.id_stem().to_owned();
        let label = i18n::t(choice.metric.label_key()).to_owned();
        let metric = choice.metric;
        let entity = cx.entity();
        let pill = elements::Pill::new(
            (ElementId::from("tm-gpu-chart-metric-pill"), stem.clone()),
            label,
            move |_window: &mut gpui::Window, app: &mut gpui::App| {
                entity.update(app, |view, cx| {
                    view.select_gpu_chart_metric(metric, gpu_index, cx);
                });
            },
            |_, _, _| {},
        )
        .active(active)
        .enabled(available)
        .render(theme);
        row = row.child(
            div()
                .debug_selector(move || format!("tm-gpu-chart-metric-pill:{stem}"))
                .child(pill),
        );
    }
    row
}

impl RootView {
    /// The pill/keyboard activation path (ADR-034 stage 2): route the family
    /// through the shell's availability gate for the viewed device. An
    /// unavailable family changes nothing.
    pub fn select_gpu_chart_metric(
        &mut self,
        metric: GpuChartMetric,
        gpu_index: usize,
        cx: &mut Context<RootView>,
    ) {
        let gate =
            taskmanager_shell::gpu_chart_metric_gate(self.system_snapshot().gpu.get(gpu_index));
        if self.shell.select_gpu_chart_metric(metric, &gate) {
            cx.notify();
        }
    }

    /// The per-tick chart-metric fold (ADR-034 stage 2): reconcile this
    /// window's shared selection against the viewed GPU's gate. Called from
    /// the application update loop before render, next to the engine-rows
    /// visibility reconcile.
    pub(crate) fn reconcile_gpu_chart_metric(&mut self) {
        use crate::gpui_app::root::TopPage;
        use crate::gpui_app::sidebar::SelectedDevice;
        if let (TopPage::Performance, SelectedDevice::Gpu(index)) = (self.page, self.selected) {
            let gate =
                taskmanager_shell::gpu_chart_metric_gate(self.system_snapshot().gpu.get(index));
            self.shell.reconcile_gpu_chart_metric(&gate);
        }
    }
}

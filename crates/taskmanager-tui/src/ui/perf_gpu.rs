//! Per-GPU Performance detail with one dominant utilization history.
//!
//! Current facts never compete for the chart slot. Standard terminals add a
//! bounded engine viewport only after the utilization chart retains a readable
//! height; compact terminals keep a dense fact strip and give every remaining
//! row to the primary chart.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Wrap};

use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::GpuMetrics;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::gpu_chart_metric::{
    GpuChartMetric, GpuChartMetricUnit, gpu_chart_metric_history,
};
use taskmanager_shell::presentation::{bytes, gpu_display_identity};
use taskmanager_ui_contract::IconId;

use crate::{TuiApp, TuiTheme};

const MIN_STANDARD_GRAPH_HEIGHT: u16 = 10;
const MAX_ENGINE_VIEWPORT_HEIGHT: u16 = 8;
/// Standard-layout entry height: the fully-observed full fact strip (ten
/// rows: identity, marketing name, the two proven graphics-API versions,
/// the clock/power row, two split-VRAM rows, throttle, driver, PCI slot)
/// plus the minimum primary chart must fit before engines may join.
const STANDARD_LAYOUT_HEIGHT: u16 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GpuFactDensity {
    Compact,
    Full,
}

/// Explicit responsive GPU composition. The primary graph is present in every
/// state; the optional engine region is possible only in the standard state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GpuPanelLayout {
    Compact {
        facts: Rect,
        graph: Rect,
    },
    Standard {
        facts: Rect,
        graph: Rect,
        engines: Option<Rect>,
    },
}

impl GpuPanelLayout {
    fn resolve(
        area: Rect,
        compact_fact_rows: usize,
        full_fact_rows: usize,
        engine_rows: usize,
    ) -> Self {
        if area.height < STANDARD_LAYOUT_HEIGHT {
            let fact_height = bounded_height(
                compact_fact_rows,
                area.height.saturating_sub(3).min(area.height / 3),
            );
            let [facts, graph] =
                Layout::vertical([Constraint::Length(fact_height), Constraint::Min(1)]).areas(area);
            return Self::Compact { facts, graph };
        }

        let fact_height = bounded_height(
            full_fact_rows,
            area.height.saturating_sub(MIN_STANDARD_GRAPH_HEIGHT),
        );
        let [facts, below] =
            Layout::vertical([Constraint::Length(fact_height), Constraint::Min(1)]).areas(area);
        let requested_engine_height = u16::try_from(engine_rows.saturating_add(2))
            .unwrap_or(u16::MAX)
            .min(MAX_ENGINE_VIEWPORT_HEIGHT);
        let engine_height =
            requested_engine_height.min(below.height.saturating_sub(MIN_STANDARD_GRAPH_HEIGHT));
        if engine_rows == 0 || engine_height < 3 {
            return Self::Standard {
                facts,
                graph: below,
                engines: None,
            };
        }
        let [graph, engines] = Layout::vertical([
            Constraint::Min(MIN_STANDARD_GRAPH_HEIGHT),
            Constraint::Length(engine_height),
        ])
        .areas(below);
        Self::Standard {
            facts,
            graph,
            engines: Some(engines),
        }
    }

    const fn fact_density(self) -> GpuFactDensity {
        match self {
            Self::Compact { .. } => GpuFactDensity::Compact,
            Self::Standard { .. } => GpuFactDensity::Full,
        }
    }

    const fn facts(self) -> Rect {
        match self {
            Self::Compact { facts, .. } | Self::Standard { facts, .. } => facts,
        }
    }

    const fn graph(self) -> Rect {
        match self {
            Self::Compact { graph, .. } | Self::Standard { graph, .. } => graph,
        }
    }

    const fn engines(self) -> Option<Rect> {
        match self {
            Self::Compact { .. } => None,
            Self::Standard { engines, .. } => engines,
        }
    }
}

fn bounded_height(rows: usize, available: u16) -> u16 {
    u16::try_from(rows).unwrap_or(u16::MAX).min(available)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GpuLineViewport {
    start: usize,
    end: usize,
    total: usize,
}

impl GpuLineViewport {
    fn resolve(total: usize, requested: usize, rows: usize) -> Self {
        let visible = total.min(rows);
        let start = requested.min(total.saturating_sub(visible));
        Self {
            start,
            end: start.saturating_add(visible),
            total,
        }
    }

    const fn is_windowed(self) -> bool {
        self.end.saturating_sub(self.start) < self.total
    }
}

pub(super) fn render_gpu_section(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    gpus: &[GpuMetrics],
) {
    if area.height == 0 {
        return;
    }
    if gpus.is_empty() {
        super::render_empty_panel(
            frame,
            theme,
            area,
            t("common.gpu"),
            "No GPU telemetry available",
        );
        return;
    }

    let compact_facts = gpu_fact_lines_with_theme(gpus, theme, GpuFactDensity::Compact);
    let full_facts = gpu_fact_lines_with_theme(gpus, theme, GpuFactDensity::Full);
    let engine_rows = taskmanager_core::core::identity::DeviceId::new(gpus[0].device_id.clone());
    let engine_rows = taskmanager_shell::presentation::gpu_engine_rows::present_gpu_engine_rows(
        app.shell.gpu_engine_rows_state(),
        &engine_rows,
        app.projection()
            .capability_status(&taskmanager_platform_contract::CapabilityId::TELEMETRY_GPU_ENGINES),
    );
    let engine_lines = gpu_engine_lines(gpus, &app, theme, app.prefs.graph_points, engine_rows);
    let layout = GpuPanelLayout::resolve(
        area,
        compact_facts.len(),
        full_facts.len(),
        engine_lines.len(),
    );
    let facts = match layout.fact_density() {
        GpuFactDensity::Compact => compact_facts,
        GpuFactDensity::Full => full_facts,
    };
    frame.render_widget(
        Paragraph::new(facts).wrap(Wrap { trim: true }),
        layout.facts(),
    );
    // The chart-metric selection is the shared shell contract (ADR-034):
    // the section renders the projection for the headline device (the first
    // GPU) and every per-device window follows the selected family in the
    // same frame the `g` cycle or a generation reset changes it.
    let metric = app
        .shell
        .gpu_chart_metric_projection(&taskmanager_shell::gpu_chart_metric_gate(gpus.first()))
        .selected;
    render_gpu_metric_chart(frame, gpus, &app, theme, layout.graph(), metric);
    if let Some(engine_area) = layout.engines() {
        render_gpu_engine_viewport(
            frame,
            theme,
            engine_area,
            &engine_lines,
            app.gpu_engine_scroll,
        );
    }
}

#[cfg(test)]
fn gpu_fact_lines(gpus: &[GpuMetrics], density: GpuFactDensity) -> Vec<Line<'static>> {
    gpu_fact_lines_with_theme(gpus, TuiTheme::default(), density)
}

fn gpu_fact_lines_with_theme(
    gpus: &[GpuMetrics],
    theme: TuiTheme,
    density: GpuFactDensity,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(gpus.len().saturating_mul(10));
    for gpu in gpus {
        let data = super::perf_data::gpu_data(gpu);
        let identity = gpu_display_identity(gpu)
            .headline
            .unwrap_or(taskmanager_shell::presentation::MISSING_VALUE);
        match density {
            GpuFactDensity::Compact => {
                lines.push(Line::from(format!(
                    "{} {} · {} {}",
                    theme.glyph(IconId::Gpu),
                    identity,
                    t("common.utilization"),
                    data.utilization,
                )));
                let power = data
                    .power
                    .as_deref()
                    .unwrap_or(taskmanager_shell::presentation::MISSING_VALUE);
                lines.push(Line::from(format!(
                    "  {} · {} · {} {} · {} {}",
                    data.temperature,
                    data.clock,
                    t("common.power"),
                    power,
                    t("gpu.idle_residency"),
                    data.idle_residency,
                )));
            }
            GpuFactDensity::Full => {
                lines.push(Line::from(format!(
                    "{} {} · {} {} · {} {}",
                    theme.glyph(IconId::Gpu),
                    identity,
                    t("common.utilization"),
                    data.utilization,
                    t("common.temperature"),
                    data.temperature,
                )));
                if let Some(name) = gpu
                    .marketing_name
                    .as_deref()
                    .filter(|value| !value.is_empty())
                {
                    lines.push(Line::from(format!(
                        "  {} {}",
                        t("gpu.marketing_name"),
                        name,
                    )));
                }
                // Proven graphics-API versions (GPUI gpu_stats parity). The
                // core `GpuGraphicsApi` contract is explicit: a provider may
                // leave a version absent when the loader/context is
                // unavailable and consumers must OMIT that row — never render
                // an inferred or dash placeholder.
                if let Some(api) = gpu.graphics_api.as_ref() {
                    if let Some(version) = api
                        .opengl_version
                        .as_deref()
                        .filter(|value| !value.is_empty())
                    {
                        lines.push(Line::from(format!(
                            "  {} {}",
                            t("gpu.opengl_version"),
                            version,
                        )));
                    }
                    if let Some(version) = api
                        .vulkan_version
                        .as_deref()
                        .filter(|value| !value.is_empty())
                    {
                        lines.push(Line::from(format!(
                            "  {} {}",
                            t("gpu.vulkan_version"),
                            version,
                        )));
                    }
                }
                let power = data
                    .power
                    .as_deref()
                    .unwrap_or(taskmanager_shell::presentation::MISSING_VALUE);
                lines.push(Line::from(format!(
                    "  {} {} · {} {} · {} {} · {} {}",
                    t("common.clock"),
                    data.clock,
                    t("gpu.max_clock"),
                    data.max_clock,
                    t("common.power"),
                    power,
                    t("gpu.idle_residency"),
                    data.idle_residency,
                )));
                for vram in &data.vram {
                    lines.push(Line::from(format!(
                        "  {}  {} / {}",
                        vram.label,
                        bytes(vram.used),
                        bytes(vram.total),
                    )));
                }
                if let Some(reason) = data.throttle_reason.as_deref() {
                    lines.push(Line::from(format!("  {} {}", t("gpu.throttling"), reason,)));
                }
                if let Some(driver) = gpu.driver.as_deref() {
                    lines.push(Line::from(format!("  {} {}", t("common.driver"), driver,)));
                }
                // Raw PCI function identity (GPUI gpu_stats tail row). An
                // unattached/unprobed GPU omits the row instead of a dash.
                if let Some(slot) = gpu
                    .pci_slot
                    .as_deref()
                    .filter(|slot| !slot.trim().is_empty())
                {
                    lines.push(Line::from(format!("  {} {}", t("gpu.pci_slot"), slot,)));
                }
            }
        }
    }
    lines
}

fn gpu_engine_lines(
    gpus: &[GpuMetrics],
    shell: &ShellApp,
    theme: TuiTheme,
    graph_window: usize,
    engine_rows: taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsPresentation<'_>,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, gpu) in gpus.iter().enumerate() {
        for engine in &gpu.engines {
            let window = shell.history.gpu_engine_usage_pct_for(
                &gpu.device_id,
                gpu.device_generation.get(),
                &engine.name,
            );
            lines.push(Line::from(vec![
                Span::raw(format!("{} · {} ", gpu.device_id, engine.name)),
                Span::raw(super::observed_percentage(Some(engine.usage_pct))),
                Span::raw(" "),
                Span::styled(
                    super::sparkline::device_trend_in(theme.terminal.glyphs, &window, graph_window),
                    Style::new().fg(theme.warn),
                ),
            ]));
        }
        if index == 0 {
            match &engine_rows {
                taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsPresentation::Active(engines) => {
                    if engines.is_empty() {
                        lines.push(Line::from(t("gpu.engines_none_reported")));
                    }
                    for engine in *engines {
                        lines.push(Line::from(format!(
                            "{} · {} {}",
                            gpu.device_id,
                            engine.name,
                            super::observed_percentage(Some(engine.utilization_pct)),
                        )));
                    }
                }
                presentation => {
                    if let Some(key) = presentation.message_key() {
                        lines.push(Line::from(t(key)));
                    }
                    if matches!(
                        presentation,
                        taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsPresentation::MissingDependency
                    ) {
                        lines.push(Line::from(t("gpu.engines_install_hint")));
                    }
                }
            }
        }
    }
    lines
}

fn render_gpu_metric_chart(
    frame: &mut Frame<'_>,
    gpus: &[GpuMetrics],
    shell: &ShellApp,
    theme: TuiTheme,
    area: Rect,
    metric: GpuChartMetric,
) {
    if area.height == 0 {
        return;
    }
    let title = format!("{} · {}", t("common.gpu"), t(metric.label_key()));
    let windows: Vec<Vec<f32>> = gpus
        .iter()
        .map(|gpu| {
            gpu_chart_metric_history(
                &shell.history,
                &gpu.device_id,
                gpu.device_generation.get(),
                metric,
            )
        })
        .collect();
    if windows
        .iter()
        .all(|window| window.iter().filter(|value| value.is_finite()).count() < 2)
    {
        // The honest dash/gap projection (ADR-034): a selected family with
        // no trustworthy samples renders the collecting placeholder, never
        // a fabricated flat line at zero.
        super::render_centered_state(
            frame,
            theme,
            area,
            &title,
            IconId::Refresh,
            t("perf.collecting_samples"),
        );
        return;
    }

    let series: Vec<Vec<(f64, f64)>> = windows
        .iter()
        .map(|window| {
            window
                .iter()
                .enumerate()
                .map(|(index, &value)| (index as f64, f64::from(value)))
                .collect()
        })
        .collect();
    let colors = [
        theme.warn,
        theme.accent,
        theme.good,
        theme.danger,
        theme.color(Color::Cyan),
        theme.color(Color::Magenta),
    ];
    let datasets: Vec<Dataset<'_>> = series
        .iter()
        .enumerate()
        .map(|(index, data)| {
            let name = gpu_display_identity(&gpus[index])
                .headline
                .unwrap_or(gpus[index].device_id.as_str());
            let color = colors[index % colors.len()];
            Dataset::default()
                .name(Span::styled(name.to_owned(), Style::new().fg(color)))
                .data(data)
                .marker(Marker::HalfBlock)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(color))
        })
        .collect();
    let x_max = series.iter().map(Vec::len).max().unwrap_or(2).max(2) as f64;
    let x_axis = Axis::default()
        .bounds([0.0, x_max])
        .labels([Line::from(t("perf.older")), Line::raw(t("perf.now"))]);
    let y_axis = gpu_metric_y_axis(metric.unit(), &windows);
    frame.render_widget(
        Chart::new(datasets)
            .block(super::panel(&title, theme))
            .x_axis(x_axis)
            .y_axis(y_axis),
        area,
    );
}

/// The chart's y axis in the selected family's unit (ADR-034 stage 2:
/// “TUI 摘要单位与循环键同波更新”). Percent families keep the fixed
/// 0–100 ladder; scalar families scale to the windows' finite peak so the
/// same tick that flips the metric flips the axis, labels, and unit.
fn gpu_metric_y_axis(unit: GpuChartMetricUnit, windows: &[Vec<f32>]) -> Axis<'static> {
    if unit == GpuChartMetricUnit::Percent {
        return Axis::default().bounds([0.0, 100.0]).labels([
            Line::from("0%"),
            Line::raw("50%"),
            Line::raw("100%"),
        ]);
    }
    let peak = windows
        .iter()
        .flat_map(|window| window.iter().copied())
        .filter(|value| value.is_finite())
        .fold(0.0_f64, |peak, value| peak.max(f64::from(value)));
    let upper = if peak > 0.0 { peak * 1.1 } else { 1.0 };
    let format = |value: f64| match unit {
        GpuChartMetricUnit::Watts => taskmanager_shell::presentation::power_w(upper_scale(value)),
        GpuChartMetricUnit::Celsius => {
            taskmanager_shell::presentation::temperature_c(upper_scale(value))
        }
        // The axis constructor is only reached for non-percent units; the
        // percent arm above caught `Percent`.
        GpuChartMetricUnit::Megahertz | GpuChartMetricUnit::Percent => {
            taskmanager_shell::presentation::megahertz(upper_scale(value))
        }
    };
    Axis::default().bounds([0.0, upper]).labels([
        Line::from("0"),
        Line::raw(format(upper / 2.0)),
        Line::raw(format(upper)),
    ])
}

/// Axis labels render through the shared `f32` formatters; narrow the `f64`
/// axis coordinates back without an unchecked cast.
fn upper_scale(value: f64) -> f32 {
    value.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32
}

fn render_gpu_engine_viewport(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    area: Rect,
    lines: &[Line<'static>],
    requested: usize,
) {
    let viewport = GpuLineViewport::resolve(
        lines.len(),
        requested,
        usize::from(area.height.saturating_sub(2)),
    );
    let title = if viewport.is_windowed() {
        format!(
            "{} · ↑/↓ {}–{} / {}",
            t("gpu.per_engine_title"),
            viewport.start.saturating_add(1),
            viewport.end,
            viewport.total,
        )
    } else {
        t("gpu.per_engine_title").to_owned()
    };
    frame.render_widget(
        Paragraph::new(lines[viewport.start..viewport.end].to_vec())
            .block(super::panel(&title, theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_gpu_tests.rs"]
mod tests;

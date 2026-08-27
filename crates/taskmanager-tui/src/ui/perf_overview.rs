//! Performance CPU/Memory composition. CPU consumes the shared stable metric
//! inventory as an all-facts strip, one dominant utilization history and a
//! scrollable per-core viewport; Memory keeps its gauges, composition and shared
//! CPU/memory history. [`render_perf_overview`] is the page entry point.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, Gauge, GraphType, Paragraph};
use taskmanager_application::{SystemSnapshot, i18n::t};
use taskmanager_shell::history::MetricSeries;
use taskmanager_shell::presentation::{graph_summary, missing_value};
use taskmanager_ui_contract::IconId;

use super::panel;
use super::perf_overview_data::{CpuMetricFact, cpu_gauge_value, cpu_metric_facts};
use super::render_centered_state;
use super::{perf_core_grid, perf_memory};
use crate::PerfDevice;
use crate::TuiApp;
use crate::TuiTheme;

/// Render the selected CPU or Memory overview through its dedicated layout.
pub(super) fn render_perf_overview(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    snapshot: &SystemSnapshot,
) {
    match app.perf_device {
        PerfDevice::Cpu => render_cpu_overview(frame, app, theme, area, snapshot),
        PerfDevice::Memory => render_memory_overview(frame, app, theme, area, snapshot),
        PerfDevice::Disk
        | PerfDevice::Network
        | PerfDevice::Gpu
        | PerfDevice::Battery
        | PerfDevice::Fan => {}
    }
}

fn render_memory_overview(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    snapshot: &SystemSnapshot,
) {
    let [gauges, below] = Layout::vertical([Constraint::Length(5), Constraint::Min(0)]).areas(area);
    let [cpu, memory, swap] = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .areas(gauges);
    render_gauge(
        frame,
        theme,
        cpu,
        t("common.cpu"),
        cpu_gauge_value(snapshot),
        theme.accent,
    );
    render_gauge(
        frame,
        theme,
        memory,
        t("common.memory"),
        snapshot.memory.used_percentage_observed(),
        theme.good,
    );
    render_gauge(
        frame,
        theme,
        swap,
        t("mem.swap"),
        snapshot.memory.swap_percentage_observed(),
        theme.warn,
    );

    let needed = perf_memory::composition_height(&snapshot.memory);
    let composition_height = if area.height >= 20 && area.height.saturating_sub(5 + needed) >= 6 {
        needed
    } else {
        0
    };
    let graph = if composition_height > 0 {
        let [composition, graph] =
            Layout::vertical([Constraint::Length(composition_height), Constraint::Min(0)])
                .areas(below);
        perf_memory::render_memory_composition(frame, app, &snapshot.memory, theme, composition);
        graph
    } else if area.height >= 22 {
        // Preserve the original graph gate: the history graph only allocates
        // when the content area is tall enough, else the gauges-only frame.
        below
    } else {
        Rect::ZERO
    };
    render_utilization_graph(frame, app, theme, graph);
}

fn render_cpu_overview(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    snapshot: &SystemSnapshot,
) {
    let facts = cpu_metric_facts(&snapshot.cpu);
    let [fact_strip, below] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);
    render_cpu_facts(frame, theme, fact_strip, &facts);

    let full_core_height = perf_core_grid::grid_height(app, below.width);
    match cpu_chart_layout(below, full_core_height) {
        CpuChartLayout::GraphOnly { graph } => {
            render_cpu_utilization_graph(frame, app, theme, graph);
        }
        CpuChartLayout::GraphWithCores { graph, cores } => {
            render_cpu_utilization_graph(frame, app, theme, graph);
            perf_core_grid::render_core_grid(frame, app, theme, cores);
        }
    }
}

/// Typed responsive composition for the CPU chart region. The main history is
/// never sacrificed for optional per-core detail: core rows join only when the
/// graph can retain a ten-row readable surface, and their viewport is capped
/// to one third of the region (six rows maximum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CpuChartLayout {
    GraphOnly { graph: Rect },
    GraphWithCores { graph: Rect, cores: Rect },
}

fn cpu_chart_layout(area: Rect, full_core_height: u16) -> CpuChartLayout {
    const MIN_GRAPH_HEIGHT_WITH_CORES: u16 = 10;
    const MAX_CORE_VIEWPORT_HEIGHT: u16 = 6;

    let core_height =
        full_core_height.min(area.height.saturating_div(3).min(MAX_CORE_VIEWPORT_HEIGHT));
    if core_height < 2 || area.height < MIN_GRAPH_HEIGHT_WITH_CORES.saturating_add(core_height) {
        return CpuChartLayout::GraphOnly { graph: area };
    }
    let [graph, cores] = Layout::vertical([
        Constraint::Min(MIN_GRAPH_HEIGHT_WITH_CORES),
        Constraint::Length(core_height),
    ])
    .areas(area);
    CpuChartLayout::GraphWithCores { graph, cores }
}

fn render_cpu_facts(frame: &mut Frame<'_>, theme: TuiTheme, area: Rect, facts: &[CpuMetricFact]) {
    if area.height == 0 {
        return;
    }
    let lines: Vec<Line<'static>> = facts
        .chunks(2)
        .take(usize::from(area.height))
        .map(|row| {
            let mut spans = Vec::with_capacity(row.len() * 4);
            for (index, fact) in row.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled("  │  ", Style::new().fg(theme.dim)));
                }
                spans.push(Span::styled(
                    format!("{} ", fact.label),
                    Style::new().fg(theme.dim),
                ));
                spans.push(Span::styled(
                    fact.value.clone(),
                    Style::new().fg(if fact.available {
                        theme.accent
                    } else {
                        theme.dim
                    }),
                ));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_gauge(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    area: Rect,
    title: &str,
    value: Option<f32>,
    color: Color,
) {
    let bounded = value
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 100.0));
    let label = bounded.map_or_else(missing_value, |value| format!("{value:.1}%"));
    frame.render_widget(
        Gauge::default()
            .block(panel(title, theme))
            .gauge_style(
                Style::new()
                    .fg(if bounded.is_some() { color } else { theme.dim })
                    .bg(theme.gauge_track_bg),
            )
            .label(label)
            .ratio(f64::from(bounded.unwrap_or(0.0)) / 100.0),
        area,
    );
}

/// The Memory page's two-series CPU% + memory% chart, read from the shared
/// `LiveGraphHistory` window (`app.history.series(...)`, G-02 — the retired
/// frontend-local ring double-collected). A window with fewer than two samples
/// renders an honest collecting state instead of a fabricated flat line.
fn render_utilization_graph(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    if area.height == 0 {
        return;
    }
    let title = t("perf.cpu_memory_history");
    let cpu_values = app.history.series(MetricSeries::CpuUsagePercent);
    let memory_values = app.history.series(MetricSeries::MemoryUsagePercent);
    if cpu_values.len() < 2 && memory_values.len() < 2 {
        render_centered_state(
            frame,
            theme,
            area,
            title,
            IconId::Refresh,
            t("perf.collecting_samples"),
        );
        return;
    }
    let cpu: Vec<(f64, f64)> = cpu_values
        .iter()
        .enumerate()
        .map(|(index, &value)| (index as f64, f64::from(value)))
        .collect();
    let memory: Vec<(f64, f64)> = memory_values
        .iter()
        .enumerate()
        .map(|(index, &value)| (index as f64, f64::from(value)))
        .collect();
    let summary = join_summary([
        summary_line(t("common.cpu"), &cpu_values, |value| format!("{value:.0}%")),
        summary_line(t("common.memory"), &memory_values, |value| {
            format!("{value:.0}%")
        }),
    ]);
    // The x domain spans the larger series so both lines share one axis;
    // clamped to two so a single sample cannot collapse it to zero width.
    let x_max = usize::max(cpu.len(), memory.len()).max(2) as f64;
    let datasets = vec![
        Dataset::default()
            .name(Span::styled(t("common.cpu"), Style::new().fg(theme.accent)))
            .data(&cpu)
            .marker(Marker::HalfBlock)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(theme.accent)),
        Dataset::default()
            .name(Span::styled(
                t("common.memory"),
                Style::new().fg(theme.good),
            ))
            .data(&memory)
            .marker(Marker::HalfBlock)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(theme.good)),
    ];
    let x_axis = Axis::default()
        .bounds([0.0, x_max])
        .labels([Line::from(t("perf.older")), Line::raw(t("perf.now"))]);
    let y_axis = Axis::default().bounds([0.0, 100.0]).labels([
        Line::from("0%"),
        Line::raw("50%"),
        Line::raw("100%"),
    ]);
    let mut block = panel(title, theme);
    if let Some(summary) = summary {
        block = block.title_bottom(Line::from(Span::styled(
            summary,
            Style::new().fg(theme.dim),
        )));
    }
    frame.render_widget(
        Chart::new(datasets)
            .block(block)
            .x_axis(x_axis)
            .y_axis(y_axis),
        area,
    );
}

/// Render the CPU page's one dominant history: total utilization. Temperature,
/// frequency and power remain simultaneously visible as scalar facts above;
/// they do not compete with the primary graph for vertical space.
fn render_cpu_utilization_graph(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    if area.height == 0 {
        return;
    }
    let title = t("perf.cpu_utilization_history");
    let samples = app.history.series(MetricSeries::CpuUsagePercent);
    if samples.len() < 2 {
        render_centered_state(
            frame,
            theme,
            area,
            title,
            IconId::Refresh,
            t("perf.collecting_samples"),
        );
        return;
    }
    let data: Vec<(f64, f64)> = samples
        .iter()
        .enumerate()
        .map(|(index, &value)| (index as f64, f64::from(value)))
        .collect();
    // x domain spans the sample count, clamped to two so a short window cannot
    // collapse it to zero width.
    let x_max = data.len().max(2) as f64;
    let datasets = vec![
        Dataset::default()
            .name(Span::styled(t("common.cpu"), Style::new().fg(theme.accent)))
            .data(&data)
            .marker(Marker::HalfBlock)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(theme.accent)),
    ];
    let x_axis = Axis::default()
        .bounds([0.0, x_max])
        .labels([Line::from(t("perf.older")), Line::raw(t("perf.now"))]);
    let y_axis = Axis::default().bounds([0.0, 100.0]).labels([
        Line::from("0%"),
        Line::raw("50%"),
        Line::raw("100%"),
    ]);
    let summary = summary_line(t("common.cpu"), &samples, |value| format!("{value:.0}%"));
    let mut block = panel(title, theme);
    if let Some(summary) = summary {
        block = block.title_bottom(Line::from(Span::styled(
            summary,
            Style::new().fg(theme.dim),
        )));
    }
    frame.render_widget(
        Chart::new(datasets)
            .block(block)
            .x_axis(x_axis)
            .y_axis(y_axis),
        area,
    );
}

/// Render one compact graph-statistics segment. The reduction is shared with
/// Iced and GPUI, so gaps and a single finite sample have identical semantics
/// in all three frontends.
fn summary_line(
    label: &str,
    samples: &[f32],
    format_value: impl Fn(f32) -> String,
) -> Option<String> {
    let summary = graph_summary(samples)?;
    Some(format!(
        "{label} · {} {} · {} {} · {} {}",
        t("common.latest"),
        format_value(summary.latest),
        t("common.avg"),
        format_value(summary.average),
        t("common.peak"),
        format_value(summary.maximum),
    ))
}

fn join_summary(lines: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let lines: Vec<String> = lines.into_iter().flatten().collect();
    (!lines.is_empty()).then(|| lines.join("  │  "))
}

//! Per-core utilization grid for the Performance page's Cpu view.
//!
//! Mirrors gpui's per-core matrix (`crates/taskmanager-gpui/src/gpui_app/cpu_view/per_core_grid.rs`):
//! when the projection carries a per-logical-CPU type inventory
//! (`hardware.cpu_types`), cells group under the typed P-cores / E-cores /
//! LP-E-cores / Cores headers, each header carrying its core count, in that
//! fixed order. Without that inventory the grid fail-closes to the historical
//! flat layout — grouping is never guessed from missing data.
//!
//! Each cell plots the core's SHARED per-core utilization window
//! (`per_core_usage_series`) as a mini trend through the one sparkline
//! component's profile-aware API, so an ASCII profile paints the same trend
//! on the ASCII ladder at paint time; the trend is tinted by the latest
//! sample's load tier, so a pinned core still pops at a glance. Beside it a
//! three-segment readout — utilization · frequency · temperature — reads the
//! core's current typed observations; a segment whose observation is
//! unavailable renders the honest "—", never a fabricated zero.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::CpuType;
use taskmanager_core::core::metrics::CpuMetrics;

use super::sparkline::{recent_window_with, sparkline_in};
use crate::{TuiApp, TuiGlyphMode, TuiTheme};

/// Width of one grid cell: three-character core label + space + six-character
/// trend + space + the widest three-segment readout ("100% · 4.94 GHz ·
/// 100 °C") + two-character gutter.
const CELL_WIDTH: u16 = 37;

/// Samples in one cell's mini trend (the most-recent window of the shared
/// per-core ring), right-aligned so every trend ends at the readout edge.
const CELL_TREND_CHARS: usize = 6;

/// Load-tier band edges (percent). Below [`WARN_EDGE`] is green, up to
/// [`DANGER_EDGE`] is amber, above is red.
const WARN_EDGE: f32 = 60.0;
const DANGER_EDGE: f32 = 85.0;

/// The typed group presentation order, mirroring gpui's per-core matrix.
const GROUP_ORDER: [CpuType; 4] = [
    CpuType::Performance,
    CpuType::Efficient,
    CpuType::LowPower,
    CpuType::Unknown,
];

/// One typed topology group: a core class and the logical indices in it.
struct CoreGroup {
    core_type: CpuType,
    indices: Vec<usize>,
}

impl CoreGroup {
    /// The locale key for the group header label.
    fn label_key(&self) -> &'static str {
        match self.core_type {
            CpuType::Performance => "cpu.performance_cores",
            CpuType::Efficient => "cpu.efficiency_cores",
            CpuType::LowPower => "cpu.low_power_cores",
            CpuType::Unknown => "common.cores",
        }
    }

    /// Header line plus the cell rows the group's cores occupy.
    fn line_count(&self, cols: usize) -> usize {
        1 + self.indices.len().div_ceil(cols)
    }
}

/// The grid's full height for the current core count and terminal width. The
/// overview uses it when every row fits and otherwise supplies a bounded
/// viewport; the `+1` is the fixed title/range line. Group headers count as
/// lines so the viewport budget sees the real grouped height.
#[must_use]
pub(super) fn grid_height(app: &TuiApp, width: u16) -> u16 {
    let cores = app.history.per_core_usage_series().len();
    if cores == 0 {
        return 0;
    }
    let cols = columns_for(width);
    let lines = match topology_groups(app, cores) {
        Some(groups) => groups.iter().map(|group| group.line_count(cols)).sum(),
        None => cores.div_ceil(cols),
    };
    lines as u16 + 1
}

/// The typed topology groups for `core_count` lanes, or `None` when the
/// projection carries no per-logical-CPU type inventory (fail-closed flat
/// layout). An index past the inventory's end classifies as `Unknown`, so a
/// history that outgrew a stale inventory still renders — honestly grouped.
fn topology_groups(app: &TuiApp, core_count: usize) -> Option<Vec<CoreGroup>> {
    let cpu_types = &app.projection().hardware.as_ref()?.cpu_types;
    if cpu_types.is_empty() {
        return None;
    }
    let groups: Vec<CoreGroup> = GROUP_ORDER
        .into_iter()
        .map(|core_type| CoreGroup {
            core_type,
            indices: (0..core_count)
                .filter(|&index| cpu_types.get(index).copied().unwrap_or_default() == core_type)
                .collect(),
        })
        .filter(|group| !group.indices.is_empty())
        .collect();
    Some(groups)
}

/// Render the per-core grid into `area`. The first line is a fixed title/range
/// line and the remaining rows are a clamped slice of the content lines (cell
/// rows, plus group headers when the topology is grouped). Up/Down and
/// PageUp/PageDown move `cpu_core_scroll`, so a high-core-count host remains
/// reachable on a short terminal instead of being dropped.
pub(super) fn render_core_grid(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let cores = app.history.per_core_usage_series();
    if cores.is_empty() || area.height == 0 {
        return;
    }
    let cols = columns_for(area.width).max(1);
    let mode = theme.terminal.glyphs;
    let cpu = app.snapshot().map(|snapshot| &snapshot.cpu);
    let mut lines: Vec<Line<'static>> = Vec::new();
    match topology_groups(app, cores.len()) {
        Some(groups) => {
            for group in &groups {
                lines.push(group_header(theme, group));
                push_cell_rows(&mut lines, theme, mode, cpu, &group.indices, &cores, cols);
            }
        }
        None => {
            let all: Vec<usize> = (0..cores.len()).collect();
            push_cell_rows(&mut lines, theme, mode, cpu, &all, &cores, cols);
        }
    }

    let visible_rows = usize::from(area.height.saturating_sub(1));
    let start = app
        .cpu_core_scroll
        .min(lines.len().saturating_sub(visible_rows.max(1)));
    let end = (start + visible_rows).min(lines.len());
    let title = if visible_rows == 0 {
        format!(" {}  ↑↓", t("common.cores"))
    } else if lines.len() > visible_rows {
        format!(
            " {}  {}-{}/{}  ↑↓",
            t("common.cores"),
            start + 1,
            end,
            lines.len()
        )
    } else {
        format!(" {}", t("common.cores"))
    };
    let mut paint: Vec<Line<'static>> = Vec::with_capacity(end.saturating_sub(start) + 1);
    paint.push(Line::from(vec![Span::styled(
        title,
        Style::new().fg(theme.dim),
    )]));
    paint.extend(lines.into_iter().skip(start).take(visible_rows));
    frame.render_widget(Paragraph::new(paint), area);
}

/// The group header line: the typed label with its core count, mirroring
/// gpui's `label + count` header pair.
fn group_header(theme: TuiTheme, group: &CoreGroup) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {}", t(group.label_key())),
            Style::new().fg(theme.dim).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", group.indices.len()),
            Style::new().fg(theme.fg_dim),
        ),
    ])
}

/// Append one cell row per `cols` slice of `indices`, reading each core's
/// shared window and current observations.
fn push_cell_rows(
    lines: &mut Vec<Line<'static>>,
    theme: TuiTheme,
    mode: TuiGlyphMode,
    cpu: Option<&CpuMetrics>,
    indices: &[usize],
    cores: &[Vec<f32>],
    cols: usize,
) {
    for row in indices.chunks(cols) {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(row.len() * 5);
        for &core_index in row {
            let cell = core_cell(
                cores.get(core_index).map(Vec::as_slice).unwrap_or_default(),
                cpu,
                core_index,
                mode,
            );
            spans.push(Span::styled(
                format!("C{core_index:02}"),
                Style::new().fg(theme.dim),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("{:>1$}", cell.trend, CELL_TREND_CHARS),
                Style::new().fg(tier_color(theme, cell.utilization)),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(cell.readout, Style::new().fg(theme.dim)));
            spans.push(Span::raw("  "));
        }
        lines.push(Line::from(spans));
    }
}

/// The number of grid columns that fit `width`, at least one.
fn columns_for(width: u16) -> usize {
    (width / CELL_WIDTH).max(1) as usize
}

/// One core cell: the mini trend, the three-segment readout, and the latest
/// finite utilization that drives the tier tint. Every readout segment reads
/// the core's current typed observation and renders the shared dash when
/// unobserved — a fully-dark core reads "— · — · —", never a fabricated 0.
fn core_cell(
    samples: &[f32],
    cpu: Option<&CpuMetrics>,
    core_index: usize,
    mode: TuiGlyphMode,
) -> CoreCell {
    let trend = sparkline_in(mode, recent_window_with(samples, CELL_TREND_CHARS));
    let utilization = samples
        .iter()
        .rev()
        .find(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 100.0));
    // The three-segment readout folds the core's current typed observations
    // in the page's data layer; paint only joins the trend with it.
    let readout = super::perf_data::core_cell_readout(cpu, core_index);
    CoreCell {
        trend,
        readout,
        utilization,
    }
}

/// The load-tier color for one core's latest utilization: green below
/// [`WARN_EDGE`], amber up to [`DANGER_EDGE`], red above — `dim` while no
/// sample is finite.
fn tier_color(theme: TuiTheme, utilization: Option<f32>) -> Color {
    let Some(pct) = utilization else {
        return theme.dim;
    };
    let pct = pct.clamp(0.0, 100.0);
    if pct >= DANGER_EDGE {
        theme.danger
    } else if pct >= WARN_EDGE {
        theme.warn
    } else {
        theme.good
    }
}

/// The projection of one core's shared window plus current observations onto
/// its grid cell.
struct CoreCell {
    trend: String,
    readout: String,
    utilization: Option<f32>,
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_core_grid_tests.rs"]
mod tests;

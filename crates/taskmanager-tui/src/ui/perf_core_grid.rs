//! Per-core utilization grid for the Performance page's Cpu view.
//!
//! Mirrors gpui's per-core mini-graph grid (`crates/taskmanager-gpui/src/gpui_app/cpu_view.rs`): one
//! cell per logical core, each a compact 0–100% bar colored by load tier
//! (green / amber / red) so a pinned core pops at a glance — the Win11 Task
//! Manager / Mission-Center per-core treatment. Samples come from the SHARED
//! per-core windows in `LiveGraphHistory` (`per_core_usage_series`), so the TUI
//! keeps no second history. A core with no finite sample yet renders an honest
//! "—" readout, never a fabricated 0%.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::missing_value;

use crate::{TuiApp, TuiTheme};

/// Width of one grid cell: three-character core label + space + eight-character
/// bar + space + four-character percentage + two-character gutter.
const CELL_WIDTH: u16 = 19;

/// Characters in one utilization bar (the 0–100% scale).
const BAR_CHARS: usize = 8;

/// The horizontal bar characters, empty→full.
const FILLED: char = '▓';
const EMPTY: char = '░';

/// Load-tier band edges (percent). Below [`WARN_EDGE`] is green, up to
/// [`DANGER_EDGE`] is amber, above is red.
const WARN_EDGE: f32 = 60.0;
const DANGER_EDGE: f32 = 85.0;

/// The grid's full height for the current core count and terminal width. The
/// overview uses it when every row fits and otherwise supplies a bounded
/// viewport; the `+1` is the fixed title/range line.
#[must_use]
pub(super) fn grid_height(app: &TuiApp, width: u16) -> u16 {
    let cores = app.history.per_core_usage_series().len();
    if cores == 0 {
        return 0;
    }
    let cols = columns_for(width);
    cores.div_ceil(cols) as u16 + 1
}

/// Render the per-core grid into `area`. The first line is a fixed title/range
/// line and the remaining rows are a clamped slice of the canonical topology.
/// Up/Down and PageUp/PageDown move `cpu_core_scroll`, so a high-core-count
/// host remains reachable on a short terminal instead of being dropped.
pub(super) fn render_core_grid(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let cores = app.history.per_core_usage_series();
    if cores.is_empty() || area.height == 0 {
        return;
    }
    let cols = columns_for(area.width).max(1);
    let row_count = cores.len().div_ceil(cols);
    let visible_rows = usize::from(area.height.saturating_sub(1));
    let start = app
        .cpu_core_scroll
        .min(row_count.saturating_sub(visible_rows.max(1)));
    let end = (start + visible_rows).min(row_count);
    let title = if visible_rows == 0 {
        format!(" {}  ↑↓", t("common.cores"))
    } else if row_count > visible_rows {
        format!(
            " {}  {}-{}/{}  ↑↓",
            t("common.cores"),
            start + 1,
            end,
            row_count
        )
    } else {
        format!(" {}", t("common.cores"))
    };
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(end.saturating_sub(start) + 1);
    lines.push(Line::from(vec![Span::styled(
        title,
        Style::new().fg(theme.dim),
    )]));
    for (row_offset, row) in cores
        .chunks(cols)
        .skip(start)
        .take(visible_rows)
        .enumerate()
    {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(row.len() * 4);
        for (offset, samples) in row.iter().enumerate() {
            let core_index = (start + row_offset) * cols + offset;
            let cell = core_cell(samples);
            spans.push(Span::styled(
                format!("C{core_index:02}"),
                Style::new().fg(theme.dim),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                FILLED.to_string().repeat(cell.filled),
                Style::new().fg(tier_color(theme, cell.utilization)),
            ));
            spans.push(Span::styled(
                EMPTY.to_string().repeat(BAR_CHARS - cell.filled),
                Style::new().fg(theme.dim),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(cell.readout, Style::new().fg(theme.dim)));
            spans.push(Span::raw("  "));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The number of grid columns that fit `width`, at least one.
fn columns_for(width: u16) -> usize {
    (width / CELL_WIDTH).max(1) as usize
}

/// One core cell: the filled-bar length, the percent readout, and the
/// utilization that drives the tier color. A core with no finite sample yields
/// zero filled and `None` (renders the empty bar and "—", never a 0%).
fn core_cell(samples: &[f32]) -> CoreCell {
    match samples.last().copied() {
        None => CoreCell {
            filled: 0,
            readout: missing_value(),
            utilization: None,
        },
        Some(current) => {
            let clamped = current.clamp(0.0, 100.0);
            let filled = (clamped / 100.0 * BAR_CHARS as f32).round() as usize;
            CoreCell {
                filled,
                readout: format!("{clamped:>3.0}%"),
                utilization: Some(clamped),
            }
        }
    }
}

/// The load-tier color for one core's utilization: green below [`WARN_EDGE`],
/// amber up to [`DANGER_EDGE`], red above — `dim` while no sample is finite.
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

/// The projection of one core's samples onto its bar cell.
struct CoreCell {
    filled: usize,
    readout: String,
    utilization: Option<f32>,
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_core_grid_tests.rs"]
mod tests;

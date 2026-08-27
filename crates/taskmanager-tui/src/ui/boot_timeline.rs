//! Startup-page boot-timeline waterfall (BN-05).
//!
//! A bounded ascii-bar projection of the measured boot critical chain: one
//! row per timed unit window (normalized bar + duration), an honest
//! "no timing data" row listing untimed units (counted, never placed), and a
//! `+N` row for collapsed overflow. The whole block stays silent until typed
//! evidence arrives and stays silent on a typed failure — the same semantics
//! as the GPUI waterfall, projected through the shared
//! [`taskmanager_application::boot_timeline_rows`] decision.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use taskmanager_application::i18n::t;
use taskmanager_application::{BootTimeline, StartupBootEvidenceSnapshot, boot_timeline_rows};

use crate::TuiTheme;

/// Waterfall bar track width in cells (a layout contract, not a theme token).
pub(super) const TIMELINE_BAR_CELLS: usize = 24;
/// Minimum visible bar width so a 0-duration activation is still a mark.
pub(super) const TIMELINE_MIN_BAR_CELLS: usize = 1;
/// Unit-name column width.
const UNIT_COLUMN: usize = 24;

/// One renderable waterfall row (measured window, untimed listing, or
/// collapsed overflow).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TimelineRow {
    /// Filled bar cells (0 for rows that carry no bar).
    pub bar_cells: usize,
    /// Left-column text (unit name or the localized "no timing data" label).
    pub label: String,
    /// Right-column text (duration for measured rows, `count · names` for the
    /// untimed row, empty otherwise).
    pub detail: String,
    /// Whether the row renders dim (untimed/collapsed metadata).
    pub dim: bool,
}

/// Pure waterfall projection over one typed evidence snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TimelineProjection {
    pub total_ms: u64,
    pub rows: Vec<TimelineRow>,
}

/// Project the waterfall rows, or `None` when the block must stay silent
/// (no typed evidence yet, or a typed critical-chain failure).
pub(super) fn project_timeline(
    evidence: Option<&StartupBootEvidenceSnapshot>,
) -> Option<TimelineProjection> {
    let timeline: BootTimeline = boot_timeline_rows(evidence?)?;
    let mut rows: Vec<TimelineRow> = timeline
        .segments
        .iter()
        .map(|segment| {
            let fraction = timeline.fraction_of_total(segment);
            let cells = (fraction * TIMELINE_BAR_CELLS as f32)
                .round()
                .max(TIMELINE_MIN_BAR_CELLS as f32) as usize;
            TimelineRow {
                bar_cells: cells.min(TIMELINE_BAR_CELLS),
                label: segment.unit.clone(),
                detail: format!("{} ms", segment.duration_ms),
                dim: false,
            }
        })
        .collect();
    if timeline.untimed_count > 0 {
        rows.push(TimelineRow {
            bar_cells: 0,
            label: t("startup.timeline_untimed").to_string(),
            detail: format!(
                "{} · {}",
                timeline.untimed_count,
                timeline.untimed_units.join(" · ")
            ),
            dim: true,
        });
    }
    if timeline.collapsed_count > 0 {
        rows.push(TimelineRow {
            bar_cells: 0,
            label: String::new(),
            detail: format!("+{}", timeline.collapsed_count),
            dim: true,
        });
    }
    Some(TimelineProjection {
        total_ms: timeline.total_ms,
        rows,
    })
}

/// Render the waterfall block into `area`. Renders nothing when the
/// projection is silent (no evidence / typed failure).
pub(super) fn render_boot_timeline(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    evidence: Option<&StartupBootEvidenceSnapshot>,
    area: Rect,
) {
    let Some(projection) = project_timeline(evidence) else {
        return;
    };
    let title = format!(" {} · {} ms ", t("startup.timeline"), projection.total_ms);
    let lines: Vec<Line<'_>> = projection
        .rows
        .iter()
        .map(|row| {
            let (label_color, detail_color, bar_color) = if row.dim {
                (theme.dim, theme.dim, theme.dim)
            } else {
                (Color::White, theme.dim, theme.accent)
            };
            let bar: String = if row.bar_cells == 0 {
                " ".repeat(TIMELINE_BAR_CELLS)
            } else {
                format!(
                    "{}{}",
                    "█".repeat(row.bar_cells),
                    " ".repeat(TIMELINE_BAR_CELLS - row.bar_cells)
                )
            };
            Line::from(vec![
                Span::styled(
                    format!(" {:<UNIT_COLUMN$} ", row.label),
                    Style::new().fg(label_color),
                ),
                Span::styled(bar, Style::new().fg(bar_color)),
                Span::styled(
                    format!(" {:<9} ", row.detail),
                    Style::new().fg(detail_color),
                ),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.border)),
        ),
        area,
    );
}

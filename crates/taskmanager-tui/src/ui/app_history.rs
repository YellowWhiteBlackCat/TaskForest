//! Durable per-application history page.

use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row};
use ratatui::{Frame, layout::Rect};
use taskmanager_application::{
    ApplicationHistoryProjection, ApplicationHistoryStatus, ApplicationHistoryUnavailableReason,
    i18n::t,
};
use taskmanager_core::core::history::HistoryWindow;
use taskmanager_shell::presentation::{bytes, missing_value};

use crate::{TuiApp, TuiGlyphMode, TuiTheme};

pub(super) fn render_app_history(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    render_app_history_projection(
        frame,
        theme,
        area,
        &app.application_history_projection(),
        app.selected,
        app.application_history_unavailable_reason(),
    );
}

/// The page body projected from one immutable [`ApplicationHistoryProjection`],
/// split from the app-backed entry above so behavior tests can paint the Ready
/// table from a hand-built typed projection without a live replay session.
fn render_app_history_projection(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    area: Rect,
    projection: &ApplicationHistoryProjection,
    selected: usize,
    unavailable_reason: Option<ApplicationHistoryUnavailableReason>,
) {
    let title = format!(
        "{} · {}  [1] 1h  [2] 24h  [3] 7d",
        t("history.application.title"),
        window_label(projection.selected_window)
    );
    if projection.status != ApplicationHistoryStatus::Ready {
        let detail = match projection.status {
            ApplicationHistoryStatus::Disabled => t("history.application.disabled_detail"),
            ApplicationHistoryStatus::Unavailable => t("history.application.unavailable_detail"),
            ApplicationHistoryStatus::Connecting => t("history.application.connecting_detail"),
            ApplicationHistoryStatus::Collecting => t("history.application.collecting_detail"),
            ApplicationHistoryStatus::Ready => "",
        };
        let detail = unavailable_reason.map_or_else(
            || detail.to_owned(),
            |reason| format!("{detail} ({})", reason.stable_code()),
        );
        super::render_empty_panel(frame, theme, area, &title, &detail);
        return;
    }

    let row_window = super::table_window(projection.rows.len(), selected, area);
    let rows = projection.rows[row_window.start..row_window.end]
        .iter()
        .map(|row| {
            let provenance = t(if row.identity.is_verified() {
                "history.application.verified"
            } else {
                "history.application.unverified"
            });
            let process_peak = row
                .peak_process_count()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map_or_else(missing_value, |value| format!("{:.0}", value));
            let name = Cell::from(Line::from(vec![
                Span::styled(
                    row.display_name().to_owned(),
                    Style::new().fg(theme.color(Color::White)),
                ),
                Span::styled(format!(" · {provenance}"), Style::new().fg(theme.dim)),
            ]));
            let cpu = row
                .peak_cpu_usage_pct()
                .filter(|value| value.is_finite())
                .map_or_else(missing_value, |value| format!("{value:.1}%"));
            let memory = row
                .peak_memory_bytes()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map_or_else(missing_value, |value| {
                    bytes(value.min(u64::MAX as f64) as u64)
                });
            let samples = row
                .cpu_usage
                .as_ref()
                .map(|series| series.gap_aware_samples())
                .unwrap_or_else(|| std::sync::Arc::from([]));
            Row::new([
                name,
                Cell::from(cpu).style(Style::new().fg(theme.accent)),
                Cell::from(memory),
                Cell::from(process_peak),
                Cell::from(history_trend_in(theme.terminal.glyphs, &samples))
                    .style(Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)),
            ])
        })
        .collect();
    super::render_table(
        frame,
        super::TableRenderProps {
            theme,
            area,
            title: &title,
            rows,
            widths: [
                Constraint::Min(20),
                Constraint::Length(9),
                Constraint::Length(11),
                Constraint::Length(12),
                Constraint::Length(super::sparkline::SPARKLINE_MAX_SAMPLES as u16),
            ],
            headers: [
                t("common.name"),
                t("history.application.peak_cpu"),
                t("history.application.peak_memory"),
                t("history.application.peak_processes"),
                t("proc.trend"),
            ],
            selected: row_window.selected,
            sort: None,
        },
    );
}

fn window_label(window: HistoryWindow) -> &'static str {
    match window {
        HistoryWindow::OneHour => "1h",
        HistoryWindow::TwentyFourHours => "24h",
        HistoryWindow::SevenDays => "7d",
    }
}

/// The glyph for an explicit recording-downtime gap under the selected
/// repertoire. Both repertoires pick a character outside their own ramp so a
/// gap can never read as a real level; the ASCII underline mirrors the shared
/// sparkline's ASCII gap. (The Unicode gap stays the renderer-local space —
/// deliberately narrower than the device trends' mid-dot, because this trend
/// renders inside a table text column.)
const fn history_trend_gap(mode: TuiGlyphMode) -> char {
    match mode {
        TuiGlyphMode::Unicode => ' ',
        TuiGlyphMode::Ascii => '_',
    }
}

/// The ramp character for one clamped normalized level (index always 0..=7)
/// under the selected glyph repertoire. Both repertoires index the shared
/// `sparkline` component's published ramps (`super::sparkline::
/// SPARKLINE_BLOCKS` / `super::sparkline::SPARKLINE_ASCII_BLOCKS`) — the
/// one ladder single-source — so the app-history trend and the device trends
/// carry the same level at the same index by construction.
const fn history_trend_block(mode: TuiGlyphMode, index: usize) -> char {
    match mode {
        TuiGlyphMode::Unicode => super::sparkline::SPARKLINE_BLOCKS[index],
        TuiGlyphMode::Ascii => super::sparkline::SPARKLINE_ASCII_BLOCKS[index],
    }
}

/// Project the bounded recent window of one application's CPU-history samples
/// (oldest→newest, `NaN` = a recording-downtime gap) onto a single-line trend
/// in the given terminal glyph repertoire. Min/max normalization uses the
/// finite samples only, so downtime gaps never flatten the shape, and a window
/// without a single finite sample renders the honest "collecting" text instead
/// of a fabricated trend. The Unicode repertoire keeps the historical block
/// ramp byte for byte; the ASCII repertoire paints the same normalized levels
/// through the shared [`super::sparkline::SPARKLINE_ASCII_BLOCKS`] ladder at
/// paint time, so an ASCII-only
/// terminal reads a monotonic gradient straight from the renderer instead of
/// the collapsed output of the post-paint cell rewrite.
fn history_trend_in(mode: TuiGlyphMode, samples: &[f32]) -> String {
    let samples = super::sparkline::recent_window(samples);
    let finite = samples.iter().copied().filter(|sample| sample.is_finite());
    let min = finite.clone().fold(f32::INFINITY, f32::min);
    let max = finite.fold(f32::NEG_INFINITY, f32::max);
    if !min.is_finite() || !max.is_finite() {
        return t("history.application.collecting").to_owned();
    }
    let range = max - min;
    samples
        .iter()
        .map(|sample| {
            if !sample.is_finite() {
                return history_trend_gap(mode);
            }
            let normalized = if range > 0.0 {
                ((*sample - min) / range).clamp(0.0, 1.0)
            } else {
                0.5
            };
            history_trend_block(mode, ((normalized * 7.0).round() as usize).min(7))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/app_history_tests.rs"]
mod tests;

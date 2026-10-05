//! Persisted Performance review with fixed controls and whole curve-row admission.

use crate::{TuiApp, TuiTheme};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use taskmanager_application::{ApplicationHistoryStatus, PerformanceHistoryProjection, i18n::t};
use taskmanager_core::core::history::HistoryWindow;
use taskmanager_shell::presentation::{history_replay::row_heading, missing_value};

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    render_projection(
        frame,
        &app.performance_history_projection(),
        app.history_replay_row_scroll(),
        theme,
        area,
    );
}

fn render_projection(
    frame: &mut Frame<'_>,
    model: &PerformanceHistoryProjection,
    row_scroll: usize,
    theme: TuiTheme,
    area: Rect,
) {
    let inner = super::panel(t("perf.replay.title"), theme).inner(area);
    frame.render_widget(super::panel(t("perf.replay.title"), theme), area);
    let actions = format!(
        "1 1h · 2 24h · 3 7d\nf {} · r {}\n↑/↓ {}",
        t("perf.replay.refresh"),
        t("perf.replay.back_to_live"),
        t("proc_insights.scroll_hint")
    );
    let footer = Paragraph::new(actions.as_str()).wrap(Wrap { trim: false });
    let slots = super::pinned_actions::PinnedActions::new(inner, &actions);
    let [header, body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(slots.content);
    let footer_area = slots.actions;
    let mut heading = format!(
        "{} · {}",
        t("perf.replay.title"),
        window_label(model.selected_window)
    );
    if model.stale() {
        heading.push_str(&format!(" · {}", t("history.application.refreshing")));
    }
    if let Some(error) = &model.failure {
        heading.push_str(&format!(" · {}", error.kind().stable_code()));
    }
    frame.render_widget(
        Paragraph::new(heading).style(Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)),
        header,
    );
    frame.render_widget(footer, footer_area);
    if model.rows.is_empty() {
        let message = match model.status {
            ApplicationHistoryStatus::Disabled => t("history.application.disabled_detail"),
            ApplicationHistoryStatus::Unavailable => t("history.application.unavailable_detail"),
            ApplicationHistoryStatus::Connecting => t("history.application.connecting_detail"),
            ApplicationHistoryStatus::Collecting | ApplicationHistoryStatus::Ready => {
                t("perf.replay.empty")
            }
        };
        frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: true }), body);
        return;
    }
    // A curve row is mandatory; its complete statistics band is secondary.
    // Narrow/short frames admit whole two-line rows; larger frames admit all
    // four lines, including one clear bottom gap, and never paint half a row.
    let row_height = if body.height >= 4 { 4 } else { 2 };
    let admitted = usize::from(body.height / row_height).min(model.rows.len());
    let start = row_scroll.min(model.rows.len().saturating_sub(admitted.max(1)));
    for (index, row) in model.rows[start..start.saturating_add(admitted)]
        .iter()
        .enumerate()
    {
        let rect = Rect::new(
            body.x,
            body.y.saturating_add(index as u16 * row_height),
            body.width,
            row_height,
        );
        let peak = row
            .peak_value
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, |value| format!("{value:.1}"));
        let name = format!(
            "{} · {} {}",
            row_heading(&row.key),
            t("perf.replay.peak"),
            peak
        );
        let samples = row.gap_aware_samples();
        let graph = super::sparkline::history_trend_in(theme.terminal.glyphs, &samples);
        let mut lines = vec![
            Line::from(Span::styled(name, Style::new().fg(theme.accent))),
            Line::from(Span::styled(
                graph,
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
        ];
        if row_height == 4 {
            lines.push(Line::from(format!(
                "{} {} · {} {} · {} {}",
                t("perf.replay.observed"),
                row.observed,
                t("perf.replay.gaps"),
                row.gaps,
                t("perf.replay.clock_jumps"),
                row.clock_jumps
            )));
        }
        frame.render_widget(Paragraph::new(lines), rect);
    }
}

fn window_label(window: HistoryWindow) -> &'static str {
    match window {
        HistoryWindow::OneHour => "1h",
        HistoryWindow::TwentyFourHours => "24h",
        HistoryWindow::SevenDays => "7d",
    }
}

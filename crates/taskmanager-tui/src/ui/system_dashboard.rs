//! Whole metric groups inside the terminal's bounded System dashboard viewport.

use super::{panel, sparkline::history_trend_with_width_in};
use crate::{TuiApp, TuiTheme};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use taskmanager_application::{
    i18n::t,
    system_timeline::{SystemHistoryWindow, TimelineMetric, TimelineStatistic},
};
use taskmanager_shell::presentation::system_timeline::{coverage, readout};

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let series = app.shell.system_timeline_series(app.system_history_window);
    let title = format!(
        "{} · b {} · ↑/↓",
        t("dashboard.title"),
        t("dashboard.hardware")
    );
    let border = panel(&title, theme);
    let inner = border.inner(area);
    frame.render_widget(border, area);
    let mut selector = Vec::new();
    for (index, window) in SystemHistoryWindow::ALL.into_iter().enumerate() {
        selector.push(Span::styled(
            format!("{}:{} ", index + 1, window.label()),
            Style::new().fg(if window == app.system_history_window {
                theme.accent
            } else {
                theme.dim
            }),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(selector)),
        Rect { height: 1, ..inner },
    );
    let projection = app.shell.projection();
    let count = projection
        .processes
        .as_ref()
        .map_or_else(|| "—".to_owned(), |rows| rows.len().to_string());
    let summary = format!(
        "{} {} · {} {}",
        t("dashboard.processes"),
        count,
        t("dashboard.active_alerts"),
        projection.alert_active.len()
    );
    frame.render_widget(
        Paragraph::new(summary),
        Rect {
            y: inner.y.saturating_add(1),
            height: 1,
            ..inner
        },
    );
    let slots = usize::from(inner.height.saturating_sub(2) / 4);
    let first = app
        .system_scroll
        .min(TimelineMetric::ALL.len().saturating_sub(1));
    for (slot, metric) in TimelineMetric::ALL
        .into_iter()
        .skip(first)
        .take(slots)
        .enumerate()
    {
        let lines = vec![
            Line::from(Span::styled(
                t(metric.label_key()),
                Style::new().fg(theme.accent),
            )),
            Line::from(format!(
                "{} {} · {} {}",
                t("dashboard.latest"),
                readout(&series, metric, TimelineStatistic::Latest),
                t("dashboard.peak"),
                readout(&series, metric, TimelineStatistic::Peak)
            )),
            Line::from(history_trend_with_width_in(
                theme.terminal.glyphs,
                &series.samples(metric),
                usize::from(inner.width),
            )),
            Line::from(coverage(&series, metric)),
        ];
        let y = inner
            .y
            .saturating_add(2)
            .saturating_add(u16::try_from(slot * 4).unwrap_or(0));
        frame.render_widget(
            Paragraph::new(lines),
            Rect {
                y,
                height: 4,
                ..inner
            },
        );
    }
}

//! Durable per-application history page.

use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Cell, Paragraph, Row, Wrap};
use ratatui::{Frame, layout::Rect};
use taskmanager_application::{
    ApplicationHistoryProjection, ApplicationHistoryRow, ApplicationHistoryStatus,
    ApplicationHistoryUnavailableReason, i18n::t,
};
use taskmanager_core::core::history::HistoryWindow;
use taskmanager_shell::presentation::{bytes, missing_value};

use super::sparkline::history_trend_in;
use crate::{TuiApp, TuiTheme};

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
        "{} · {}",
        t("history.application.title"),
        window_label(projection.selected_window)
    );
    let actions = history_actions();
    let slots = super::pinned_actions::PinnedActions::new(area, &actions);
    frame.render_widget(
        Paragraph::new(actions).wrap(Wrap { trim: false }),
        slots.actions,
    );
    let area = slots.content;
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

    if area.width < 96 {
        render_history_cards(frame, theme, area, projection, selected, &title);
        return;
    }
    let row_window = super::table_window(projection.rows.len(), selected, area);
    let rows = projection.rows[row_window.start..row_window.end]
        .iter()
        .map(|row| {
            let facts = history_row_text(row, theme);
            Row::new([
                Cell::from(facts[0].clone()),
                Cell::from(facts[1].clone()).style(Style::new().fg(theme.accent)),
                Cell::from(facts[2].clone()),
                Cell::from(facts[3].clone()),
                Cell::from(facts[4].clone())
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

fn history_actions() -> String {
    format!("1 1h · 2 24h · 3 7d\nf {}", t("perf.replay.refresh"))
}

pub(super) fn capture_has_visible_rows(area: Rect) -> bool {
    let slots = super::pinned_actions::PinnedActions::new(area, &history_actions());
    if area.width < 96 {
        slots.content.height >= 7
    } else {
        slots.content.height >= 5
    }
}

fn history_row_text(row: &ApplicationHistoryRow, theme: TuiTheme) -> [String; 5] {
    let provenance = t(if row.identity.is_verified() {
        "history.application.verified"
    } else {
        "history.application.unverified"
    });
    let name = format!("{} {}", row.display_name(), provenance);
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
    let count = row
        .peak_process_count()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(missing_value, |value| format!("{value:.0}"));
    let samples = row
        .cpu_usage
        .as_ref()
        .map(|series| series.gap_aware_samples())
        .unwrap_or_else(|| std::sync::Arc::from([]));
    [
        name,
        cpu,
        memory,
        count,
        history_trend_in(theme.terminal.glyphs, &samples),
    ]
}

fn render_history_cards(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    area: Rect,
    projection: &ApplicationHistoryProjection,
    selected: usize,
    title: &str,
) {
    let block = super::panel(title, theme);
    let body = block.inner(area);
    frame.render_widget(block, area);
    let height = if body.height >= 6 { 6 } else { 5 };
    let count = usize::from(body.height / height);
    if count == 0 {
        return;
    }
    let selected = selected.min(projection.rows.len().saturating_sub(1));
    let start = selected
        .saturating_sub(count / 2)
        .min(projection.rows.len().saturating_sub(count));
    for (index, row) in projection.rows.iter().enumerate().skip(start).take(count) {
        let facts = history_row_text(row, theme);
        let lines = [
            facts[0].clone(),
            format!("{} {}", t("history.application.peak_cpu"), facts[1]),
            format!("{} {}", t("history.application.peak_memory"), facts[2]),
            format!("{} {}", t("history.application.peak_processes"), facts[3]),
            facts[4].clone(),
        ];
        let slot = Rect::new(
            body.x,
            body.y
                + u16::try_from(index - start)
                    .unwrap_or(u16::MAX)
                    .saturating_mul(height),
            body.width,
            5,
        );
        let style = Style::new().fg(if index == selected {
            theme.accent
        } else {
            theme.color(Color::White)
        });
        frame.render_widget(Paragraph::new(lines.join("\n")).style(style), slot);
    }
}

fn window_label(window: HistoryWindow) -> &'static str {
    match window {
        HistoryWindow::OneHour => "1h",
        HistoryWindow::TwentyFourHours => "24h",
        HistoryWindow::SevenDays => "7d",
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/app_history_tests.rs"]
mod tests;

//! Bounded service-log panel for the Services page.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::{ServiceId, ServiceLogAvailability, i18n::t};

use super::super::panel;
use crate::{TuiApp, TuiTheme};

/// Render the frozen service identity, active filters and visible feed rows.
pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let Some(open) = app.shell.service_log.as_ref() else {
        return;
    };
    let follow = if open.feed.follow {
        t("common.follow")
    } else {
        t("common.hold")
    };
    let paused = if open.feed.paused {
        format!(" · {}", t("common.paused"))
    } else {
        String::new()
    };
    let title = format!(
        "{} · {} {}{} · {}",
        t("svc.logs"),
        open.service_id().map_or("—", ServiceId::as_str),
        follow,
        paused,
        t("svc.logs_controls"),
    );
    let entries = app
        .shell
        .visible_service_log_entries(app.service_log_now_micros)
        .unwrap_or_default();
    let mut lines: Vec<Line<'static>> = Vec::new();
    if entries.is_empty() {
        let state = app.shell.service_log_provider_state();
        let message = match state.map(|state| &state.availability) {
            Some(ServiceLogAvailability::Empty) => t("svc.logs_empty").to_string(),
            Some(ServiceLogAvailability::Loading) => t("svc.logs_loading").to_string(),
            Some(ServiceLogAvailability::CaughtUp) => t("svc.logs_time_all").to_string(),
            Some(ServiceLogAvailability::Disconnected)
            | Some(ServiceLogAvailability::Unavailable)
            | Some(ServiceLogAvailability::Stale) => t("svc.logs_failed").to_string(),
            Some(ServiceLogAvailability::Available) => t("svc.logs_waiting_entries").to_string(),
            None => t("svc.logs_loading").to_string(),
        };
        lines.push(Line::from(Span::styled(
            message,
            Style::new().fg(theme.dim),
        )));
    } else {
        for entry in entries.iter().take(12) {
            lines.push(Line::from(entry.message.clone()));
        }
        if entries.len() > 12 {
            lines.push(Line::from(Span::styled(
                t("svc.logs_more").replace("{count}", &(entries.len() - 12).to_string()),
                Style::new().fg(theme.dim),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(title.as_str(), theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}

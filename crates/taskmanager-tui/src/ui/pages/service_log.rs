//! Bounded service-log panel for the Services page.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::ServiceLogAvailability;
use taskmanager_core::core::target::ServiceId;

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
    // The control suffix single-sources the panel's four declared protocol
    // arms (`f p l t`) through the layer-3 hint vocabulary. The leading
    // "Esc closes" is the structural close copy — deliberately not a protocol
    // arm (the panel's `q` close stays handwritten at its dispatch site) — so
    // it keeps its own catalog key here.
    let title = format!(
        "{} · {} {}{} · {} · {}",
        t("svc.logs"),
        open.service_id().map_or("—", ServiceId::as_str),
        follow,
        paused,
        t("tui.surface.panel_close"),
        crate::command_palette::surface_hint_run(
            crate::command_palette::TuiSurfaceScope::ServiceLogPanel
        ),
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

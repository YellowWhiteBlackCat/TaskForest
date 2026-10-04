//! First-Run setup guidance overlay for the TUI frontend.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use super::containers::Modal;
use crate::TuiTheme;

pub(super) fn render_first_run_overlay_at(frame: &mut Frame<'_>, theme: TuiTheme, popup: Rect) {
    let inner = Modal::new(theme, IconId::Settings, t("first_run.title")).render(frame, popup);
    let [body, footer] = Layout::vertical([Constraint::Min(6), Constraint::Length(3)]).areas(inner);

    let lines = vec![
        Line::from(vec![Span::styled(
            t("first_run.title"),
            Style::new()
                .fg(theme.color(Color::White))
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            t("first_run.description"),
            Style::new().fg(theme.dim),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            t("first_run.restart_required"),
            Style::new().fg(theme.accent),
        )]),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), body);

    let hint = vec![
        Span::styled(
            " [Esc] ",
            Style::new().fg(theme.color(Color::Black)).bg(theme.accent),
        ),
        Span::styled(
            format!(" {}", t("common.close")),
            Style::new().fg(theme.dim),
        ),
    ];
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), Line::from(hint)]).alignment(Alignment::Center),
        footer,
    );
}

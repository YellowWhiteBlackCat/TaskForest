//! Footer projection for activity, typed notices and control outcomes.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use taskmanager_application::i18n::t;

use crate::{TuiApp, TuiTheme};

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let state = if app.paused() {
        t("common.paused_badge")
    } else {
        t("common.live")
    };
    let style = if app.paused() { theme.warn } else { theme.good };
    let state_span = Span::styled(
        format!(" {state} "),
        Style::new().fg(Color::Black).bg(style),
    );
    let alert_span: Option<Span<'static>> =
        (!app.shell.projection().alert_active.is_empty()).then(|| {
            let count = app.shell.projection().alert_active.len();
            let worst = app
                .shell
                .projection()
                .alert_active
                .iter()
                .map(|alert| alert.severity)
                .max()
                .unwrap_or(taskmanager_application::alerts::AlertSeverity::Info);
            let color = match worst {
                taskmanager_application::alerts::AlertSeverity::Critical => theme.danger,
                taskmanager_application::alerts::AlertSeverity::Warning => theme.warn,
                taskmanager_application::alerts::AlertSeverity::Info => theme.good,
            };
            Span::styled(
                format!(
                    " {} ",
                    t("alerts.active").replacen("{}", &count.to_string(), 1)
                ),
                Style::new().fg(Color::Black).bg(color),
            )
        });
    let shortcut_span = Span::styled(t("footer.shortcuts"), Style::new().fg(theme.dim));
    let (marker, feedback_color) =
        app.shell
            .feedback_notice()
            .map_or(("", Color::White), |notice| match notice.severity() {
                taskmanager_shell::FeedbackSeverity::Info => ("", Color::White),
                taskmanager_shell::FeedbackSeverity::Success => ("\u{2713} ", theme.good),
                taskmanager_shell::FeedbackSeverity::Warning => ("\u{26a0} ", theme.warn),
                taskmanager_shell::FeedbackSeverity::Error => ("\u{26a0} ", theme.danger),
            });
    let feedback_width = usize::from(area.width.saturating_sub(30)).max(12);
    let feedback = if app
        .shell
        .feedback_notice()
        .is_some_and(|notice| notice.source() == taskmanager_shell::FeedbackSource::Persistence)
    {
        compact_feedback(app.feedback_text(), feedback_width)
    } else {
        app.feedback_text().to_owned()
    };
    let mut spans = vec![state_span];
    if let Some(alert_span) = alert_span {
        spans.push(alert_span);
    }
    spans.push(Span::styled(
        format!(" {marker}{feedback}  "),
        Style::new().fg(feedback_color),
    ));
    spans.push(shortcut_span);
    let lines = vec![Line::from(spans)];
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::new()
                    .borders(Borders::TOP)
                    .border_style(Style::new().fg(theme.dim)),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn compact_feedback(value: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(3);
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max_chars {
        return value.to_owned();
    }
    let tail_len = max_chars.saturating_sub(1);
    let tail: String = chars[chars.len().saturating_sub(tail_len)..]
        .iter()
        .collect();
    format!("…{tail}")
}

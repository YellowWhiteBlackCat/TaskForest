//! Footer projection for activity, typed notices and control outcomes.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use taskmanager_application::i18n::t;

use crate::{TuiApp, TuiInputScope, TuiSurfaceKind, TuiTheme};
use taskmanager_core::core::alerts::AlertSeverity;
use taskmanager_shell::{FeedbackSource, FeedbackTone};

pub(super) fn render(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    input_scope: TuiInputScope,
) {
    let state = if app.paused() {
        t("common.paused_badge")
    } else {
        t("common.live")
    };
    let style = if app.paused() { theme.warn } else { theme.good };
    let state_span = Span::styled(
        format!(" {state} "),
        Style::new().fg(theme.color(Color::Black)).bg(style),
    );
    let active_alerts = app.shell.projection().alert_center.active_alerts();
    let alert_span: Option<Span<'static>> = (!active_alerts.is_empty()).then(|| {
        let count = active_alerts.len();
        let worst = active_alerts
            .iter()
            .map(|alert| alert.severity)
            .max()
            .unwrap_or(AlertSeverity::Info);
        let color = match worst {
            AlertSeverity::Critical => theme.danger,
            AlertSeverity::Warning => theme.warn,
            AlertSeverity::Info => theme.good,
        };
        Span::styled(
            format!(
                " {} ",
                t("alerts.active").replacen("{}", &count.to_string(), 1)
            ),
            Style::new().fg(theme.color(Color::Black)).bg(color),
        )
    });
    let (marker, feedback_color) =
        app.shell
            .feedback_presentation()
            .map_or(("", theme.color(Color::White)), |feedback| {
                match feedback.tone() {
                    FeedbackTone::Neutral => ("", theme.color(Color::White)),
                    FeedbackTone::Positive => ("\u{2713} ", theme.good),
                    FeedbackTone::Caution => ("\u{26a0} ", theme.warn),
                    FeedbackTone::Danger => ("\u{26a0} ", theme.danger),
                }
            });
    let feedback_width = usize::from(area.width.saturating_sub(30)).max(12);
    let feedback = if app
        .shell
        .feedback_notice()
        .is_some_and(|notice| notice.source() == FeedbackSource::Persistence)
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
    if input_scope != TuiInputScope::LocalSurface(TuiSurfaceKind::Health) {
        spans.push(Span::styled(
            t("footer.shortcuts"),
            Style::new().fg(theme.dim),
        ));
    }
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
    super::text::truncate_tail_cells(value, max_chars.max(3))
}

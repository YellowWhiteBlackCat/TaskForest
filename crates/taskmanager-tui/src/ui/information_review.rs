//! Bounded information review; title/actions stay outside the body scroll owner.

use super::containers::Modal;
use super::process_details::{clamped_scroll, wrapped_content_height};
use crate::TuiTheme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_ui_contract::IconId;

pub(super) fn render(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    title: &str,
    text: &str,
    scroll: usize,
    actions: &str,
    popup: Rect,
) {
    let inner = Modal::new(theme, IconId::System, title).render(frame, popup);
    let footer = Paragraph::new(actions.to_owned()).wrap(Wrap { trim: false });
    let footer_height = u16::try_from(footer.line_count(inner.width).saturating_add(1))
        .unwrap_or(u16::MAX)
        .min(inner.height);
    let [body, actions] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(footer_height)]).areas(inner);
    let lines: Vec<_> = text.lines().map(Line::from).collect();
    let (scroll, _) = clamped_scroll(
        wrapped_content_height(&lines, body.width),
        body.height.saturating_sub(1),
        scroll,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    frame.render_widget(footer, actions);
}

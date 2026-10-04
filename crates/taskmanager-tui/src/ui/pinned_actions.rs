//! Bounded review content and a complete pinned action group with bottom safety.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Paragraph, Wrap};

pub(super) struct PinnedActions {
    pub(super) content: Rect,
    pub(super) actions: Rect,
}
impl PinnedActions {
    pub(super) fn new(area: Rect, text: &str) -> Self {
        let rows = u16::try_from(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .line_count(area.width),
        )
        .unwrap_or(u16::MAX)
        .saturating_add(1)
        .min(area.height);
        let [content, actions] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(rows)]).areas(area);
        Self { content, actions }
    }
}

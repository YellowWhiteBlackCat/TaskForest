//! The Services page's own search field.
//!
//! Geometry comes from the committed frame plan; the title/caret style comes
//! from the focus plan; the query and its visible count are shell-owned data.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use super::super::{TuiFocusPlan, panel};
use crate::{TuiApp, TuiTheme};

/// Paint the field. The caret and title come from the committed focus plan;
/// the match counter is a data read of the shell-owned query and its visible
/// row count.
pub(super) fn render(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    focus: TuiFocusPlan,
) {
    if area.height < 3 {
        return;
    }
    let query = app.shell.services_query.as_str();
    let focused = focus.search_field_focused();
    let match_counter = if query.trim().is_empty() {
        String::new()
    } else {
        let count = app.shell.visible_service_count();
        let key = if count == 1 {
            "tui.search_matches_one"
        } else {
            "tui.search_matches_many"
        };
        t(key).replacen("{count}", &count.to_string(), 1)
    };
    let cursor = app.shell.services_query_cursor();
    let (before, after) = query.split_at(cursor);
    let mut spans = vec![Span::raw(format!("{} ", theme.glyph(IconId::Search)))];
    spans.push(Span::raw(before));
    if focused {
        spans.push(Span::styled(
            "▌",
            Style::new().fg(theme.color(Color::White)),
        ));
    }
    spans.push(Span::raw(after));
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(if focused {
                Style::new().fg(theme.color(Color::White))
            } else {
                Style::new().fg(theme.dim)
            })
            .block(panel(
                &format!("{}{}", t("tui.services_search_title"), match_counter),
                theme,
            )),
        area,
    );
}

//! Responsive terminal header projection.
//!
//! Keyboard hints stay in the footer, so a narrow terminal can spend its
//! limited cells on page identity instead of wrapping a long label + shortcut
//! string onto another row.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use taskmanager_assets::product;
use taskmanager_shell::{PageHelp, page_help};
use taskmanager_ui_contract::IconId;

use crate::{TuiApp, TuiTheme};

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let brand = if area.width < 68 {
        " TF "
    } else {
        product::NAME
    };
    let mut spans = vec![Span::styled(
        brand,
        Style::new()
            .fg(theme.color(Color::Black))
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )];
    for PageHelp {
        page,
        icon,
        label,
        shortcut,
        ..
    } in page_help()
    {
        let active = app.page() == page;
        spans.push(Span::styled(
            header_tab_text_with_theme(icon, label, shortcut, area.width, theme),
            if active {
                Style::new()
                    .fg(theme.color(Color::White))
                    .bg(theme.highlight_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme.dim)
            },
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .block(
                Block::new()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::new().fg(theme.dim)),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(test)]
fn header_tab_text(icon: IconId, label: &str, shortcut: &str, width: u16) -> String {
    header_tab_text_with_theme(icon, label, shortcut, width, TuiTheme::default())
}

fn header_tab_text_with_theme(
    icon: IconId,
    label: &str,
    shortcut: &str,
    width: u16,
    theme: TuiTheme,
) -> String {
    if width >= 140 {
        format!(" {} {} {} ", theme.glyph(icon), label, shortcut)
    } else if width >= 88 {
        format!(" {} {} ", theme.glyph(icon), label)
    } else if width >= 68 {
        format!(" {} {} ", theme.glyph(icon), bounded_header_label(label, 6))
    } else {
        format!(" {} ", theme.glyph(icon))
    }
}

fn bounded_header_label(label: &str, max_chars: usize) -> String {
    super::text::truncate_cells(label, max_chars)
}

#[cfg(test)]
#[path = "../../tests/gui/ui/header_tests.rs"]
mod tests;

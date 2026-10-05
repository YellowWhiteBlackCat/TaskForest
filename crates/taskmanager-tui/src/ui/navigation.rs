//! The terminal rail consumes its assigned cells and the canonical page order.
use crate::{TuiApp, TuiTheme};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use taskmanager_shell::page_help;
use taskmanager_ui_contract::page_descriptors;
pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, rail: Rect) {
    let descriptors = page_descriptors();
    let pages = page_help();
    let rows: Vec<_> = pages
        .iter()
        .zip(descriptors)
        .take(usize::from(rail.height))
        .map(|(page, descriptor)| {
            let selected = app.page() == page.page;
            let label =
                super::text::truncate_cells(page.label, usize::from(rail.width.saturating_sub(4)));
            Line::from(Span::styled(
                format!("{} {}", theme.glyph(descriptor.icon), label),
                Style::new()
                    .fg(if selected { theme.accent } else { theme.dim })
                    .bg(if selected {
                        theme.highlight_bg
                    } else {
                        theme.bg
                    }),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), rail);
}

#[cfg(test)]
#[path = "../../tests/gui/ui/navigation_tests.rs"]
mod tests;

//! Column-visibility menu overlay (Applications page).
//!
//! `C` on the Applications page opens a menu over the toggleable process-table
//! columns (PID and Name are always-visible identity columns and never appear
//! here). Enter/Space toggles a column's hidden flag; the renderer drops the
//! hidden columns from every row + the header so a narrow terminal can trade
//! columns it cannot show for the ones it needs. The menu cursor is frozen in
//! [`TuiApp::column_menu_selection`]; the column ORDER comes from the single
//! source [`TuiApp::toggleable_columns`] so the menu and the renderer can never
//! disagree.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use crate::TuiApp;
use crate::TuiTheme;

/// Render the column-visibility menu centred over `area`. Each row shows the
/// column label plus its current visibility state; the active row is
/// highlighted like the other menus.
pub fn render_column_menu(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let columns = TuiApp::toggleable_columns();
    let popup = centered(area, 44, columns.len() as u16 + 4);
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            crate::icon_glyph(IconId::Settings),
            t("tui.columns_title")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows: Vec<Line<'static>> = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let visible = app.column_visible(*column);
            let active = app.column_menu_selection() == Some(index);
            let marker = if visible { "✓" } else { " " };
            let text = format!(" {marker} {:<12}", column.label());
            if active {
                Line::from(vec![
                    Span::styled("› ", Style::new().fg(theme.accent)),
                    Span::styled(
                        text,
                        Style::new()
                            .fg(Color::White)
                            .bg(theme.highlight_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("  ", Style::new().fg(theme.dim)),
                    Span::styled(text, Style::new().fg(Color::White)),
                ])
            }
        })
        .collect();

    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    frame.render_widget(Paragraph::new(rows), body);
    frame.render_widget(
        Paragraph::new(vec![Line::from(vec![
            Span::styled(" Enter ", Style::new().fg(Color::Black).bg(theme.accent)),
            Span::styled(
                format!(" {}", t("tui.toggle_esc_close")),
                Style::new().fg(theme.dim),
            ),
        ])])
        .alignment(Alignment::Center),
        footer,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

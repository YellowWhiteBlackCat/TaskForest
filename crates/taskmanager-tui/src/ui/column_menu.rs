//! Column-visibility menu overlay (Applications page).
//!
//! `C` on the Applications page opens a menu over the toggleable process-table
//! columns (PID and Name are always-visible identity columns and never appear
//! here). Enter/Space toggles a column's hidden flag; the renderer drops the
//! hidden columns from every row + the header so a narrow terminal can trade
//! columns it cannot show for the ones it needs. The menu cursor lives in
//! [`TuiSurface::ColumnMenu`] and is mirrored into the frame plan's
//! `MenuItem` control, which is what this renderer highlights; the column
//! ORDER comes from the single source [`TuiApp::toggleable_columns`] so the
//! menu and the renderer can never disagree.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use super::containers::{KeyHint, Modal};
use crate::TuiApp;
use crate::TuiTheme;
use crate::bindings::{COLUMN_MENU_HINTS, menu_hint_pairs};

/// Render the column-visibility menu from the committed focus plan. Each row
/// shows the column label plus its current visibility state; the highlighted
/// row is the plan's `MenuItem` control when it names this surface, and any
/// other control paints no highlight (fail-closed).
pub(super) fn render_column_menu_at(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    popup: Rect,
) {
    let columns = TuiApp::toggleable_columns();
    let inner = Modal::new(theme, IconId::Settings, t("tui.columns_title")).render(frame, popup);

    let rows: Vec<Line<'static>> = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let visible = app.column_visible(*column);
            let active = focus.menu_item(crate::TuiSurfaceKind::ColumnMenu) == Some(index);
            let marker = if visible { "✓" } else { " " };
            let text = format!(" {marker} {}", super::text::pad_cells(column.label(), 12));
            if active {
                Line::from(vec![
                    Span::styled("› ", Style::new().fg(theme.accent)),
                    Span::styled(
                        text,
                        Style::new()
                            .fg(theme.color(Color::White))
                            .bg(theme.highlight_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("  ", Style::new().fg(theme.dim)),
                    Span::styled(text, Style::new().fg(theme.color(Color::White))),
                ])
            }
        })
        .collect();

    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    frame.render_widget(Paragraph::new(rows), body);
    frame.render_widget(
        KeyHint::centered(theme, menu_hint_pairs(&COLUMN_MENU_HINTS)),
        footer,
    );
}

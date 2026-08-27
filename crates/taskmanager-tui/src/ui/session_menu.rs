//! Session-action menu overlay (Users page).
//!
//! Enter on the Users page opens a two-action menu (Disconnect / Lock) for the
//! selected row. The menu freezes the selected [`SessionItem`] at open time so
//! a list refresh cannot redirect the intent to a different session. Picking an
//! action opens the shared confirmation gate: the shell arms
//! [`taskmanager_application::SessionControlConfirmation`] and owns the
//! 'y'/'n' confirmation vocabulary in
//! `ShellApp::handle_local_char` (2026-08-17 uplift), so the platform request
//! is emitted only on explicit confirmation (y), mirroring the service-action
//! flow. The menu overlay itself stays renderer-owned (a terminal table
//! affordance), while the gate semantics are shared shell vocabulary.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use taskmanager_application::i18n::t;
use taskmanager_application::{SessionControlAction, SessionItem};
use taskmanager_ui_contract::IconId;

use crate::TuiTheme;

/// The action menu's frozen target: the provider-issued session row plus the
/// menu cursor. Storing the row (not an index) keeps the intent stable while
/// the overlay is open.
#[derive(Clone, Debug)]
pub struct SessionMenuTarget {
    pub session: SessionItem,
    pub selection: usize,
}

/// The actions offered by the menu, in display order.
pub const MENU_ACTIONS: [SessionControlAction; 2] =
    [SessionControlAction::Disconnect, SessionControlAction::Lock];

/// Render the session-action menu centred over `area`.
pub fn render_session_menu(
    frame: &mut Frame<'_>,
    menu: &SessionMenuTarget,
    theme: TuiTheme,
    area: Rect,
) {
    let popup = centered(area, 52, 11);
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            crate::icon_glyph(IconId::Users),
            t("users.session_actions")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).areas(inner);

    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::new().fg(theme.dim)),
        Span::styled(
            menu.session.user.as_str(),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  session {}  {}",
                menu.session.id.as_str(),
                if menu.session.remote {
                    t("users.remote")
                } else {
                    t("users.local")
                }
            ),
            Style::new().fg(theme.dim),
        ),
    ])];
    lines.push(Line::from(""));
    for (index, action) in MENU_ACTIONS.into_iter().enumerate() {
        let selected = index == menu.selection;
        let label = action_label(action);
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "▸ " } else { "  " },
                Style::new().fg(theme.accent),
            ),
            Span::styled(
                label,
                Style::new()
                    .fg(if selected { Color::White } else { theme.dim })
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), body);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(" ↑↓ ", Style::new().fg(Color::Black).bg(theme.accent)),
                Span::styled(
                    format!(" {} · ", t("menu.word_move")),
                    Style::new().fg(theme.dim),
                ),
                Span::styled("Enter", Style::new().fg(Color::Black).bg(theme.accent)),
                Span::styled(
                    format!(" {} · ", t("menu.word_select")),
                    Style::new().fg(theme.dim),
                ),
                Span::styled("Esc", Style::new().fg(Color::Black).bg(theme.accent)),
                Span::styled(
                    format!(" {}", t("menu.word_cancel")),
                    Style::new().fg(theme.dim),
                ),
            ]),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

/// TUI-local display label for one session action, resolved through the shared
/// catalog so it localizes with the active language.
pub fn action_label(action: SessionControlAction) -> &'static str {
    match action {
        SessionControlAction::Disconnect => t("users.disconnect"),
        SessionControlAction::Lock => t("users.lock"),
    }
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

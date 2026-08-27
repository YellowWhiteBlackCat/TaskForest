//! Startup-entry action menu overlay (Startup page).
//!
//! Enter on the Startup page opens a two-action menu (Enable / Disable) for
//! the selected entry. The menu freezes the selected [`StartupEntry`] at open
//! time so a list refresh cannot redirect the intent to a different entry.
//! Picking an action opens the shared confirmation overlay (the shell's gated
//! `pending_startup` slot); the platform request is emitted only on explicit
//! confirmation (y), mirroring the session-action flow.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use taskmanager_application::StartupEntry;
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use crate::TuiTheme;

/// The action menu's frozen target: the provider-issued startup entry plus
/// the menu cursor. Storing the entry (not an index) keeps the intent stable
/// while the overlay is open.
#[derive(Clone, Debug)]
pub struct StartupMenuTarget {
    pub entry: StartupEntry,
    pub selection: usize,
}

/// The enabled-state offered by each menu row, in display order (the menu
/// mirrors the session menu's action list shape).
pub const MENU_ACTIONS: [bool; 2] = [true, false];

/// The localized label for one menu action (Enable / Disable).
#[must_use]
pub fn action_label(enabled: bool) -> &'static str {
    if enabled {
        t("startup.enable")
    } else {
        t("startup.disable")
    }
}

/// Render the startup-action menu centred over `area`.
pub fn render_startup_menu(
    frame: &mut Frame<'_>,
    menu: &StartupMenuTarget,
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
            crate::icon_glyph(IconId::Startup),
            t("startup.applications")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).areas(inner);
    let lines: Vec<Line<'_>> = MENU_ACTIONS
        .iter()
        .enumerate()
        .map(|(index, enabled)| {
            let selected = index == menu.selection;
            let label = action_label(*enabled);
            if selected {
                Line::from(vec![
                    Span::styled(" ▸ ", Style::new().fg(theme.accent)),
                    Span::styled(label, Style::new().fg(Color::Black).bg(theme.accent)),
                    Span::styled(
                        format!("  {}", menu.entry.name.as_str()),
                        Style::new().fg(Color::White),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(label, Style::new().fg(Color::White)),
                    Span::styled(
                        format!("  {}", menu.entry.name.as_str()),
                        Style::new().fg(Color::Gray),
                    ),
                ])
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), body);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓ ", Style::new().fg(Color::Black).bg(theme.accent)),
            Span::styled(
                format!(" {} · ", t("menu.word_move")),
                Style::new().fg(theme.dim),
            ),
            Span::styled(" Enter ", Style::new().fg(Color::Black).bg(theme.accent)),
            Span::styled(
                format!(" {} · ", t("menu.word_select")),
                Style::new().fg(theme.dim),
            ),
            Span::styled(" Esc ", Style::new().fg(Color::Black).bg(theme.accent)),
            Span::styled(
                format!(" {}", t("menu.word_cancel")),
                Style::new().fg(theme.dim),
            ),
        ]))
        .alignment(Alignment::Center),
        footer,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let horizontal = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .flex(ratatui::layout::Flex::Center)
    .split(area);
    let vertical = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(height),
        Constraint::Min(0),
    ])
    .flex(ratatui::layout::Flex::Center)
    .split(horizontal[1]);
    vertical[1]
}

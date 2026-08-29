//! Startup-entry action menu overlay (Startup page).
//!
//! Enter on the Startup page opens a two-action menu (Enable / Disable) for
//! the selected entry. The menu freezes the selected [`StartupEntry`] at open
//! time so a list refresh cannot redirect the intent to a different entry.
//! Picking an action opens the shared confirmation overlay (the shell's gated
//! `pending_startup` slot); the platform request is emitted only on explicit
//! confirmation (y), mirroring the session-action flow.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_ui_contract::IconId;

use super::containers::{KeyHint, Modal};
use crate::TuiTheme;
use crate::bindings::{STARTUP_MENU_HINTS, menu_hint_pairs};

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

/// Render the startup-action menu centred over `area`.  Test-only entry: the
/// caller supplies the committed focus plan so the highlighted row stays the
/// plan's decision, not the frozen menu state's.
#[cfg(test)]
pub fn render_startup_menu(
    frame: &mut Frame<'_>,
    menu: &StartupMenuTarget,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    area: Rect,
) {
    render_startup_menu_at(
        frame,
        menu,
        theme,
        focus,
        super::planned_popup(
            area,
            crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::StartupMenu),
        ),
    );
}

pub(super) fn render_startup_menu_at(
    frame: &mut Frame<'_>,
    menu: &StartupMenuTarget,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    popup: Rect,
) {
    let inner = Modal::new(theme, IconId::Startup, t("startup.applications")).render(frame, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).areas(inner);
    let lines: Vec<Line<'_>> = MENU_ACTIONS
        .iter()
        .enumerate()
        .map(|(index, enabled)| {
            let selected = focus.menu_item(crate::TuiSurfaceKind::StartupMenu) == Some(index);
            let label = action_label(*enabled);
            if selected {
                Line::from(vec![
                    Span::styled(" ▸ ", Style::new().fg(theme.accent)),
                    Span::styled(
                        label,
                        Style::new().fg(theme.color(Color::Black)).bg(theme.accent),
                    ),
                    Span::styled(
                        format!("  {}", menu.entry.name.as_str()),
                        Style::new().fg(theme.color(Color::White)),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(label, Style::new().fg(theme.color(Color::White))),
                    Span::styled(
                        format!("  {}", menu.entry.name.as_str()),
                        Style::new().fg(theme.color(Color::Gray)),
                    ),
                ])
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), body);
    frame.render_widget(
        KeyHint::centered(theme, menu_hint_pairs(&STARTUP_MENU_HINTS)),
        footer,
    );
}

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
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_core::core::session::{SessionControlAction, SessionItem};
use taskmanager_ui_contract::IconId;

use super::containers::{KeyHint, Modal};
use crate::TuiTheme;
use crate::bindings::{ACTION_MENU_HINTS, menu_hint_pairs};

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

/// Render the session-action menu from the committed focus plan.  The
/// highlighted row is the plan's `MenuItem` control when it names this
/// surface; any other control paints no highlight (fail-closed).
pub(super) fn render_session_menu_at(
    frame: &mut Frame<'_>,
    menu: &SessionMenuTarget,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    popup: Rect,
) {
    let inner = Modal::new(theme, IconId::Users, t("users.session_actions")).render(frame, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).areas(inner);

    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::new().fg(theme.dim)),
        Span::styled(
            menu.session.user.as_str(),
            Style::new()
                .fg(theme.color(Color::White))
                .add_modifier(Modifier::BOLD),
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
        let selected = focus.menu_item(crate::TuiSurfaceKind::SessionMenu) == Some(index);
        let label = action_label(action);
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "▸ " } else { "  " },
                Style::new().fg(theme.accent),
            ),
            Span::styled(
                label,
                Style::new()
                    .fg(if selected {
                        theme.color(Color::White)
                    } else {
                        theme.dim
                    })
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
            KeyHint::line(theme, menu_hint_pairs(&ACTION_MENU_HINTS)),
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

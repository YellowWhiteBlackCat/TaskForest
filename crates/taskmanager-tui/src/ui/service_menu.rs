//! Service-action menu overlay (Services page).
//!
//! Enter on the Services page opens a five-action menu (Start / Stop /
//! Restart / Enable / Disable) for the selected row. The menu freezes the selected
//! [`ServiceItem`] at open time so a list refresh cannot redirect the intent
//! to a different service. Picking an action records the provider-issued
//! target via [`ShellApp::select_service_control`] and opens the shared
//! confirmation overlay; the platform request is emitted only by
//! `ConfirmServiceControl` (y), mirroring the end-task flow.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::{ServiceAction, ServiceItem};
use taskmanager_ui_contract::IconId;

use super::containers::{KeyHint, Modal};
use crate::TuiTheme;
use crate::bindings::{ACTION_MENU_HINTS, menu_hint_pairs};

/// The action menu's frozen target: the provider-issued service row plus the
/// menu cursor. Storing the row (not an index) keeps the intent stable while
/// the overlay is open.
#[derive(Clone, Debug)]
pub struct ServiceMenuTarget {
    pub service: ServiceItem,
    pub selection: usize,
}

/// The actions offered by the menu, in display order.
pub const MENU_ACTIONS: [ServiceAction; 5] = [
    ServiceAction::Start,
    ServiceAction::Stop,
    ServiceAction::Restart,
    ServiceAction::Enable,
    ServiceAction::Disable,
];

/// Render the service-action menu from the committed focus plan.  The
/// highlighted row is the plan's `MenuItem` control when it names this
/// surface; any other control paints no highlight (fail-closed).
pub(super) fn render_service_menu_at(
    frame: &mut Frame<'_>,
    menu: &ServiceMenuTarget,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    popup: Rect,
) {
    let inner = Modal::new(theme, IconId::Service, t("svc.service_actions")).render(frame, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(7), Constraint::Length(3)]).areas(inner);

    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::new().fg(theme.dim)),
        Span::styled(
            menu.service.name.as_str(),
            Style::new()
                .fg(theme.color(Color::White))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", menu.service.status.as_str()),
            Style::new().fg(status_color(menu.service.status.as_str(), theme)),
        ),
    ])];
    lines.push(Line::from(""));
    let focused = focus.menu_item(crate::TuiSurfaceKind::ServiceMenu);
    for (index, action) in MENU_ACTIONS.into_iter().enumerate() {
        let selected = focused == Some(index);
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

/// TUI-local display label for one gated service action, resolved through the
/// shared catalog so it localizes with the active language.
pub fn action_label(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Start => t("svc.start"),
        ServiceAction::Stop => t("svc.stop"),
        ServiceAction::Restart => t("svc.restart"),
        ServiceAction::Enable => t("svc.enable"),
        ServiceAction::Disable => t("svc.disable"),
    }
}

fn status_color(status: &str, theme: TuiTheme) -> Color {
    match status {
        "Active" => theme.good,
        "Failed" => theme.danger,
        _ => theme.dim,
    }
}

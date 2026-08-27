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
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use taskmanager_application::i18n::t;
use taskmanager_application::{ServiceAction, ServiceItem};
use taskmanager_ui_contract::IconId;

use crate::TuiTheme;

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

/// Render the service-action menu centred over `area`.
pub fn render_service_menu(
    frame: &mut Frame<'_>,
    menu: &ServiceMenuTarget,
    theme: TuiTheme,
    area: Rect,
) {
    let popup = centered(area, 52, 13);
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            crate::icon_glyph(IconId::Service),
            t("svc.service_actions")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(7), Constraint::Length(3)]).areas(inner);

    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::new().fg(theme.dim)),
        Span::styled(
            menu.service.name.as_str(),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", menu.service.status.as_str()),
            Style::new().fg(status_color(menu.service.status.as_str(), theme)),
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

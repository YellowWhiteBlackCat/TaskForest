//! Process-action menu overlay (Applications page).
//!
//! `a` on the Applications page opens the action menu (End task / End
//! process tree / Suspend / Resume / Force kill / the three priority tiers /
//! Open file location / Search online) for the selected row. The menu freezes
//! the selected [`ProcessItem`] at open time so a list refresh cannot
//! redirect the intent. Picking an action emits a [`PlatformEffect`] routed
//! through the platform integration ports (resource-reveal / url-open); the
//! frontend never spawns
//! the opener itself — the dependency firewall owns native command execution
//! in the platform adapter. (Clipboard copy is not offered here: a terminal
//! frontend has no in-process clipboard, and spawning a helper binary is
//! forbidden by the same firewall.)

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use taskmanager_application::PriorityTier;
use taskmanager_application::ProcessItem;
use taskmanager_application::i18n::t;
use taskmanager_application::{PlatformEffect, ResourceRevealRequest, UrlOpenRequest};
use taskmanager_shell::presentation::search_url_for;
use taskmanager_ui_contract::IconId;

use crate::TuiTheme;

/// The action menu's frozen target: the process row plus the menu cursor.
#[derive(Clone, Debug)]
pub struct ProcessMenuTarget {
    pub item: ProcessItem,
    pub selection: usize,
}

/// The actions offered by the menu, in display order (GPUI's proc-row menu
/// vocabulary: control actions first — End task directly followed by End
/// process tree — then the priority picker, then the integration actions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessMenuAction {
    EndTask,
    EndProcessTree,
    Suspend,
    Resume,
    Kill,
    PriorityHigh,
    PriorityNormal,
    PriorityLow,
    OpenLocation,
    SearchOnline,
}

/// The actions in display order.
pub const MENU_ACTIONS: [ProcessMenuAction; 10] = [
    ProcessMenuAction::EndTask,
    ProcessMenuAction::EndProcessTree,
    ProcessMenuAction::Suspend,
    ProcessMenuAction::Resume,
    ProcessMenuAction::Kill,
    ProcessMenuAction::PriorityHigh,
    ProcessMenuAction::PriorityNormal,
    ProcessMenuAction::PriorityLow,
    ProcessMenuAction::OpenLocation,
    ProcessMenuAction::SearchOnline,
];

/// The scheduling-priority tier for one priority picker action (GPUI/Iced
/// parity: the picker offers High / Normal / Low; the platform adapter owns
/// the tier→native-primitive mapping).
#[must_use]
pub fn priority_tier(action: ProcessMenuAction) -> Option<PriorityTier> {
    match action {
        ProcessMenuAction::PriorityHigh => Some(PriorityTier::High),
        ProcessMenuAction::PriorityNormal => Some(PriorityTier::Normal),
        ProcessMenuAction::PriorityLow => Some(PriorityTier::Low),
        _ => None,
    }
}

/// Localized label for one menu action. The three priority tiers route
/// through the shell's single tier→label fold
/// (`presentation::priority_tier_label`, §4.0 同一律) so the menu names each
/// tier the same way every frontend's menus, toasts, and confirmations do.
pub fn action_label(action: ProcessMenuAction) -> &'static str {
    match action {
        ProcessMenuAction::EndTask => t("proc.end_task"),
        ProcessMenuAction::EndProcessTree => t("proc.end_process_tree"),
        ProcessMenuAction::Suspend => t("proc.suspend"),
        ProcessMenuAction::Resume => t("proc.resume"),
        ProcessMenuAction::Kill => t("proc.kill"),
        ProcessMenuAction::PriorityHigh
        | ProcessMenuAction::PriorityNormal
        | ProcessMenuAction::PriorityLow => {
            // priority_tier is total over the priority variants; the Normal
            // fallback keeps the production tree panic-free.
            let tier = priority_tier(action).unwrap_or(PriorityTier::Normal);
            taskmanager_shell::presentation::priority_tier_label(tier)
        }
        ProcessMenuAction::OpenLocation => t("proc.open_location"),
        ProcessMenuAction::SearchOnline => t("proc.search_online"),
    }
}

/// Resolve one of the two integration actions (open location / search
/// online) into a platform effect, or `None` with an honest status line when
/// the process lacks the required identity/name. The effect is routed through
/// the shared integration ports; the caller queues it. Control actions
/// (End/Suspend/Resume/Kill/priority) are routed by the caller through the
/// shell's batch path instead.
#[must_use]
pub fn resolve_action(target: &ProcessMenuTarget) -> Option<PlatformEffect> {
    match MENU_ACTIONS.get(target.selection).copied() {
        Some(ProcessMenuAction::OpenLocation) => {
            let identity =
                taskmanager_application::FrozenProcessIdentity::from_process(&target.item)?;
            Some(PlatformEffect::RevealResource(ResourceRevealRequest {
                target: identity,
                cached_executable: target.item.current_exe_path().map(ToOwned::to_owned),
            }))
        }
        Some(ProcessMenuAction::SearchOnline) => {
            if target.item.name.trim().is_empty() {
                return None;
            }
            Some(PlatformEffect::OpenUrl(UrlOpenRequest {
                url: search_url_for(&target.item.name),
            }))
        }
        // The control actions are handled by the caller (they need the shell's
        // batch path / confirmation gates), never resolved here.
        Some(
            ProcessMenuAction::EndTask
            | ProcessMenuAction::EndProcessTree
            | ProcessMenuAction::Suspend
            | ProcessMenuAction::Resume
            | ProcessMenuAction::Kill
            | ProcessMenuAction::PriorityHigh
            | ProcessMenuAction::PriorityNormal
            | ProcessMenuAction::PriorityLow,
        )
        | None => None,
    }
}

/// Render the process-action menu centred over `area`.
pub fn render_process_menu(
    frame: &mut Frame<'_>,
    menu: &ProcessMenuTarget,
    theme: TuiTheme,
    area: Rect,
) {
    // 10 actions + the frozen-target row + the footer hint; height adapts so
    // the last action never clips off the overlay (inner height = popup − 2
    // borders − 3 footer).
    let popup = centered(area, 52, MENU_ACTIONS.len() as u16 + 7);
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            crate::icon_glyph(IconId::Applications),
            t("proc.actions")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).areas(inner);

    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::new().fg(theme.dim)),
        Span::styled(
            menu.item.name.as_str(),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {}", menu.item.pid), Style::new().fg(theme.dim)),
    ])];
    lines.push(Line::from(""));
    for (index, action) in MENU_ACTIONS.into_iter().enumerate() {
        let selected = index == menu.selection;
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "▸ " } else { "  " },
                Style::new().fg(theme.accent),
            ),
            Span::styled(
                action_label(action),
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

//! Batch-control menu overlay (Applications page).
//!
//! `B` on the Applications page opens a menu over the marked multi-select
//! set (`m` marks rows). Each row is one batch action (End task / Force kill /
//! Suspend / Resume / the three priority tiers / Clear selection); the menu
//! header shows the frozen marked count so a keyboard user sees exactly how
//! many processes the actions apply to. End / Kill gate behind the batch
//! confirmation; the rest submit directly through the shell's shared batch
//! path.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_core::core::process::PriorityTier;
use taskmanager_ui_contract::IconId;

use super::containers::{KeyHint, Modal};
use crate::BatchMenuTarget;
use crate::TuiTheme;
use crate::bindings::{ACTION_MENU_HINTS, menu_hint_pairs};

/// The actions offered by the batch menu, in display order. Each applies to
/// the whole marked set; End / Kill are gated behind the confirmation, the
/// rest submit directly. The priority picker offers the same typed tier set
/// as GPUI's action bar and Iced's pick_list (§8.1 语义平价律).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchMenuAction {
    End,
    Kill,
    Suspend,
    Resume,
    PriorityHigh,
    PriorityNormal,
    PriorityLow,
    Clear,
}

/// The actions in display order.
pub const MENU_ACTIONS: [BatchMenuAction; 8] = [
    BatchMenuAction::End,
    BatchMenuAction::Kill,
    BatchMenuAction::Suspend,
    BatchMenuAction::Resume,
    BatchMenuAction::PriorityHigh,
    BatchMenuAction::PriorityNormal,
    BatchMenuAction::PriorityLow,
    BatchMenuAction::Clear,
];

/// The scheduling-priority tier for one priority picker row (GPUI/Iced batch
/// parity: the picker offers High / Normal / Low).
#[must_use]
pub fn priority_tier(action: BatchMenuAction) -> Option<PriorityTier> {
    match action {
        BatchMenuAction::PriorityHigh => Some(PriorityTier::High),
        BatchMenuAction::PriorityNormal => Some(PriorityTier::Normal),
        BatchMenuAction::PriorityLow => Some(PriorityTier::Low),
        _ => None,
    }
}

/// Localized label for one batch-menu action. The priority tiers route
/// through the shell's single tier→label fold
/// (`presentation::priority_tier_label`, §8.1 同一律).
pub fn action_label(action: BatchMenuAction) -> &'static str {
    match action {
        BatchMenuAction::End => t("proc.end_task"),
        BatchMenuAction::Kill => t("proc.kill"),
        BatchMenuAction::Suspend => t("proc.suspend"),
        BatchMenuAction::Resume => t("proc.resume"),
        BatchMenuAction::PriorityHigh
        | BatchMenuAction::PriorityNormal
        | BatchMenuAction::PriorityLow => {
            // priority_tier is total over the priority variants; the Normal
            // fallback keeps the production tree panic-free.
            let tier = priority_tier(action).unwrap_or(PriorityTier::Normal);
            taskmanager_shell::presentation::priority_tier_label(tier)
        }
        BatchMenuAction::Clear => t("proc.clear_selection"),
    }
}

/// Render the batch-control menu from the committed focus plan. The frozen
/// marked count heads the menu so the scope is visible before any action. The
/// highlighted row is the plan's `MenuItem` control when it names this
/// surface; any other control paints no highlight (fail-closed).
pub(super) fn render_batch_menu_at(
    frame: &mut Frame<'_>,
    menu: &BatchMenuTarget,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    popup: Rect,
) {
    let inner =
        Modal::new(theme, IconId::Applications, t("proc.batch_actions")).render(frame, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).areas(inner);

    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::new().fg(theme.dim)),
        Span::styled(
            t("proc.marked_count").replacen("{}", &menu.marked_count.to_string(), 1),
            Style::new()
                .fg(theme.color(Color::White))
                .add_modifier(Modifier::BOLD),
        ),
    ])];
    lines.push(Line::from(""));
    for (index, action) in MENU_ACTIONS.into_iter().enumerate() {
        let selected = focus.menu_item(crate::TuiSurfaceKind::BatchMenu) == Some(index);
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "▸ " } else { "  " },
                Style::new().fg(theme.accent),
            ),
            Span::styled(
                action_label(action),
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

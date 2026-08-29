//! The Applications action-bar priority presets + the multi-select hint, plus
//! their unit tests — extracted from [`super`] so `applications.rs` stays under
//! the source-size budget. Both items are re-exported so the page builder keeps
//! calling them unqualified.

use super::*;
use taskmanager_core::core::process::PriorityTier;

/// The three nice-level presets the Applications action bar offers (mirrors the
/// GPUI batch `SetPriority` high/normal/low projection). A small `Display` +
/// `Clone` enum so the iced [`pick_list`](iced::widget::pick_list) can render
/// and dispatch it without a per-option `FocusTarget`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PriorityChoice {
    High,
    Normal,
    Low,
}

impl PriorityChoice {
    pub(super) const ALL: [Self; 3] = [Self::High, Self::Normal, Self::Low];

    pub(super) fn action(self) -> taskmanager_core::core::process::ProcessBatchAction {
        use taskmanager_core::core::process::ProcessBatchAction;
        match self {
            // The typed tier carries the semantics; the platform adapter owns
            // the tier→native-primitive mapping (mirrors GPUI's action bar
            // exactly so identical labels never diverge).
            Self::High => ProcessBatchAction::SetPriority(PriorityTier::High),
            Self::Normal => ProcessBatchAction::SetPriority(PriorityTier::Normal),
            Self::Low => ProcessBatchAction::SetPriority(PriorityTier::Low),
        }
    }
}

impl std::fmt::Display for PriorityChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The shell owns the tier→label fold (§8.1 同一律).
        let tier = match self {
            Self::High => PriorityTier::High,
            Self::Normal => PriorityTier::Normal,
            Self::Low => PriorityTier::Low,
        };
        write!(
            f,
            "{}",
            taskmanager_shell::presentation::priority_tier_label(tier)
        )
    }
}

/// The action-bar hint that mirrors the live multi-select scope: a single
/// selected row keeps the legacy "Delete confirms" note, while a multi-select
/// batch surfaces the count so the user knows a verb will reach N rows.
pub(super) fn selection_hint(selected_count: usize) -> String {
    if selected_count > 1 {
        t("proc.selected_delete_hint").replace("{count}", &selected_count.to_string())
    } else {
        t("proc.delete_confirms").to_string()
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/applications/priority_choice_tests.rs"]
mod tests;

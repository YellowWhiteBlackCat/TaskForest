//! The Applications action-bar priority presets + the multi-select hint, plus
//! their unit tests — extracted from [`super`] so `applications.rs` stays under
//! the source-size budget. Both items are re-exported so the page builder keeps
//! calling them unqualified.
//!
//! The priority vocabulary has one owner: `taskforest_core::core::process`
//! owns the offered tier set and its order, and
//! `taskmanager_shell::presentation` owns the tier→label fold. The local
//! option type carries the core tier; it declares no tier of its own.

use super::*;
use taskforest_core::core::process::{PriorityTier, ProcessBatchAction};
use taskmanager_shell::presentation::priority_tier_label;
use taskmanager_theme::tokens::{FONT_13, FONT_CAPTION, SPACE_8};
use taskmanager_ui_contract::presentation::sizes::ControlRole;

/// The complete width the priority group needs to show its localized caption
/// and placeholder. The measured text plus the shared `Input` padding keeps
/// the trigger from collapsing to a handle-only sliver in a wide action row.
#[must_use]
pub(super) fn priority_group_width(theme_snapshot: &Theme) -> f32 {
    let caption =
        crate::text_metrics::measured_text_width(t("proc.priority"), f32::from(FONT_CAPTION));
    let placeholder =
        crate::text_metrics::measured_text_width(t("proc.choose_priority"), f32::from(FONT_13));
    let padding = crate::theme::control_padding_x(theme_snapshot, ControlRole::Input);
    caption + placeholder + padding * 2.0 + f32::from(SPACE_8)
}

/// The priority options the Applications action bar offers.
///
/// `taskforest_core::core::process::PriorityTier` owns the offered set and
/// its order; `taskmanager_shell::presentation::priority_tier_label` owns the
/// one wording fold. This type is only the thin toolkit adapter the shared
/// [`pick_list`](iced::widget::pick_list) needs to render a row and dispatch a
/// choice. It carries the core tier as its payload — it is NOT a second
/// priority vocabulary — so no frontend can spell a tier set, a label or an
/// action of its own.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct PriorityOption {
    /// The core tier this option carries. Public to the page module so a
    /// behavior test can pin the carried tier without a second accessor.
    pub(super) tier: PriorityTier,
}

/// The one static pick-list option set. It is derived from the core's own
/// `PriorityTier::ALL` through a const fold, so the view tree can borrow a
/// stable slice without restating the offered ladder or allocating per frame.
pub(super) const PRIORITY_OPTIONS: [PriorityOption; 3] = PriorityOption::all();

impl PriorityOption {
    /// The options the core owner publishes, in the core's order.
    pub(super) const fn all() -> [Self; 3] {
        let mut options = [Self {
            tier: PriorityTier::Normal,
        }; 3];
        let mut index = 0;
        while index < PriorityTier::ALL.len() {
            options[index] = Self {
                tier: PriorityTier::ALL[index],
            };
            index += 1;
        }
        options
    }

    /// The typed batch action this option publishes.
    pub(super) const fn action(self) -> ProcessBatchAction {
        ProcessBatchAction::SetPriority(self.tier)
    }
}

impl std::fmt::Display for PriorityOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The shell owns the tier→label fold; no tier spelling is written
        // here.
        write!(f, "{}", priority_tier_label(self.tier))
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

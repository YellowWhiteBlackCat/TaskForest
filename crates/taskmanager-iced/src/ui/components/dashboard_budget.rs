//! Complete dashboard groups inside an allocated Iced body slot.

use taskmanager_ui_contract::presentation::card_flow::{CardFlowSpec, admit_card_groups_capped};

/// Compact-summary geometry (px contract). This is Iced's own presentation
/// decision: below either axis the four-value summary card degrades to one
/// text line, and the remaining columns stay one wide in narrow slots.
const COMPACT_SUMMARY_HEIGHT: f32 = 440.0;
const COMPACT_SUMMARY_WIDTH: f32 = 600.0;
const SUMMARY_CARD_HEIGHT: f32 = 148.0;
const COMPACT_SUMMARY_LINE_HEIGHT: f32 = 68.0;
/// One dashboard card's own footprint, handed to the shared card-flow
/// contract. The contract owns the whole-group admission and the height
/// clamp; the SHAPE owns the footprint it measures, so these numbers are
/// Iced's card footprint and not a second copy of the arithmetic.
///
/// Iced's metric card carries a title, a stats line and a coverage line
/// inside the standard card padding, so its fixed chrome is taller than the
/// GPUI card's: the floor is exactly `chrome + the minimum chart` and the
/// ceiling is the same card at its largest chart. A card is never taller
/// than its content and never shorter than its readable floor.
const CARD_HEIGHT_FLOOR: f32 = 132.0;
const CARD_HEIGHT_CEILING: f32 = 232.0;
const GROUP_GAP: f32 = 8.0;
/// The card's fixed title/stat/caption chrome (px contract). Iced subtracts
/// this from the shared card height to size its own chart; the whole-group
/// admission and the floor..ceiling clamp stay with the shared contract.
/// The floor above is this chrome plus the minimum chart, so an admitted
/// group always fits the card it renders.
const CARD_CHROME_HEIGHT: f32 = 100.0;
const CHART_HEIGHT_FLOOR: f32 = 32.0;
const CHART_HEIGHT_CEILING: f32 = 100.0;
/// Product content cap: four timeline metrics, never a layout fact.
const MAX_CARDS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DashboardBudget {
    pub(crate) compact_summary: bool,
    pub(crate) columns: usize,
    pub(crate) count: usize,
    pub(crate) chart_height: f32,
}
impl DashboardBudget {
    pub(crate) fn resolve(width: f32, height: f32) -> Self {
        let compact_summary = height < COMPACT_SUMMARY_HEIGHT || width < COMPACT_SUMMARY_WIDTH;
        let columns = if width >= COMPACT_SUMMARY_WIDTH { 2 } else { 1 };
        let fixed = if compact_summary {
            COMPACT_SUMMARY_LINE_HEIGHT
        } else {
            SUMMARY_CARD_HEIGHT
        };
        let available = height - fixed - GROUP_GAP;
        let spec = CardFlowSpec::new(CARD_HEIGHT_FLOOR, CARD_HEIGHT_CEILING).with_gap(GROUP_GAP);
        let admitted =
            admit_card_groups_capped(available, columns, spec, MAX_CARDS.div_ceil(columns));
        let count = admitted.cards().min(MAX_CARDS);
        Self {
            compact_summary,
            columns: admitted.columns,
            count,
            chart_height: if admitted.groups == 0 {
                0.0
            } else {
                (admitted.card_height - CARD_CHROME_HEIGHT)
                    .clamp(CHART_HEIGHT_FLOOR, CHART_HEIGHT_CEILING)
            },
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/dashboard_budget_tests.rs"]
mod tests;

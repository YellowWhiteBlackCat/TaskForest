//! Complete dashboard groups inside an allocated Iced body slot.
//!
//! The shared `card_flow` contract admits whole groups and returns a card
//! height that is a FLOOR, not an exact extent: the Iced dashboard stretches
//! each admitted group into an elastic line and grows its cards into that
//! line, so a slot larger than the measured chrome never leaves a dead band
//! below the last row. Only the compact-summary decision, the card's own
//! footprint and the chart's fixed plot height stay local.

use taskmanager_ui_contract::presentation::card_flow::{CardFlowSpec, admit_card_groups_capped};
use taskmanager_ui_contract::presentation::dashboard::{DASHBOARD_METRIC_COUNT, max_metric_groups};
use taskmanager_ui_contract::presentation::vocabulary::BOTTOM_SAFETY_BAND;

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
/// A dashboard metric CARD's own internal text-and-padding footprint (px
/// contract): the titled-card header, the latest/peak readout line, the
/// coverage caption, the card's inner spacing and padding. This is NOT a
/// [`taskmanager_ui_contract::presentation::chrome_rows::ChromeRowSpec`]:
/// that contract models the fixed rows a PAGE paints ABOVE its content slot
/// (section tabs, window controls, paging) measured from the shared control
/// ladder; a row of text lines inside one card is neither a page row nor a
/// row of tab/pill controls, and it has no `ChromeRowRole`. The card's
/// footprint is Iced's own composition of the shared type ladder and card
/// padding, so it stays a named local measurement. The word "chrome" is
/// reserved for page chrome rows; this is card content furniture.
const CARD_CONTENT_FURNITURE_HEIGHT: f32 = 100.0;
const CHART_HEIGHT_FLOOR: f32 = 32.0;
const CHART_HEIGHT_CEILING: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DashboardBudget {
    pub(crate) compact_summary: bool,
    pub(crate) columns: usize,
    pub(crate) count: usize,
    /// The shared contract's admitted card height, spent as the card's
    /// FLOOR: an elastic group line is never shorter than this, and grows
    /// into whatever the measured slot still holds.
    pub(crate) card_height_floor: f32,
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
        // The bottom safety band is a real fixed obligation: the elastic
        // group lines stop at it. Charging it BEFORE admission keeps the
        // shared verdict's floor inside the slot the view actually paints,
        // and leaves the same band GPUI reserves from its chrome budget.
        let available = height - fixed - GROUP_GAP - BOTTOM_SAFETY_BAND;
        let spec = CardFlowSpec::new(CARD_HEIGHT_FLOOR, CARD_HEIGHT_CEILING).with_gap(GROUP_GAP);
        let admitted =
            admit_card_groups_capped(available, columns, spec, max_metric_groups(columns));
        let count = admitted.cards().min(DASHBOARD_METRIC_COUNT);
        Self {
            compact_summary,
            columns: admitted.columns,
            count,
            card_height_floor: admitted.card_height,
            chart_height: if admitted.groups == 0 {
                0.0
            } else {
                (admitted.card_height - CARD_CONTENT_FURNITURE_HEIGHT)
                    .clamp(CHART_HEIGHT_FLOOR, CHART_HEIGHT_CEILING)
            },
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/dashboard_budget_tests.rs"]
mod tests;

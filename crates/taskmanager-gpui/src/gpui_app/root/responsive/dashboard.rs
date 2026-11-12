//! Whole dashboard metric groups after root-owned shell and page chrome.

use taskmanager_theme::tokens;
use taskmanager_theme::tokens::{RowDensity, UiSize};
use taskmanager_ui_contract::presentation::budget::PageLayoutBudget;
use taskmanager_ui_contract::presentation::card_flow::{CardFlowSpec, admit_card_groups_capped};
use taskmanager_ui_contract::presentation::card_width::{CardGridWidthSpec, admit_card_grid_width};
use taskmanager_ui_contract::presentation::dashboard::{DashboardMetricWindow, max_metric_groups};
use taskmanager_ui_contract::presentation::vocabulary::BOTTOM_SAFETY_BAND;

use super::FrameBudget;
use super::chrome_rows::DashboardChromeRows;

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_app/root/responsive/dashboard_budget_tests.rs"]
mod tests;

pub(crate) const CARD_FLOOR: f32 = 112.0;
/// A history chart is elastic content, so its ceiling is the tallest slot
/// the measured reference family can hand two whole groups. The old 172px
/// ceiling stopped the cards less than halfway into a tall frame's review
/// slot and left a visible dead band under the last row. The shared card-flow
/// contract still owns the floor clamp, whole-group admission and gap
/// arithmetic; this ceiling is the shape's own card footprint.
pub(crate) const CARD_CEILING: f32 = 420.0;
pub(crate) const CARD_GAP: f32 = tokens::SPACE_8.0;
pub(crate) const SUMMARY_CARD_HEIGHT: f32 = 88.0;
pub(crate) const SUMMARY_COUNT_HEIGHT: f32 = 24.0;
/// The dashboard root's vertical gap between its own rows.
pub(crate) const ROOT_ROW_GAP: f32 = tokens::SPACE_8.0;
/// The content width at (and above) which the section header's two groups
/// fit side by side on ONE line.
///
/// The section tabs and the page actions are label-width facts: below this
/// width they no longer fit on one line and the header stacks into two. The
/// header's line HEIGHT is not a page fact — it comes from the shared
/// chrome-row contract — so only the wrap point lives here, and both the
/// budget and the page spend this ONE threshold, so the row the page paints
/// and the row the budget reserves can never disagree.
pub(crate) const SECTION_HEADER_ONE_LINE_WIDTH: f32 = 880.0;
/// The content width at (and above) which the window-controls row keeps its
/// title, its range pills and its coverage readout on ONE line.
///
/// The same fact as the section header's: localized labels decide where the
/// row wraps, the shared contract decides how tall a line is.
pub(crate) const WINDOW_CONTROLS_ONE_LINE_WIDTH: f32 = 500.0;
/// The largest column count the dashboard's metric grid paints.
pub(crate) const GRID_COLUMN_CAP: usize = 2;

/// The dashboard metric card's own footprint, handed to the shared card-flow
/// contract that owns the whole-group admission and the height clamp.
fn card_flow_spec() -> CardFlowSpec {
    CardFlowSpec::new(CARD_FLOOR, CARD_CEILING).with_gap(CARD_GAP)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DashboardSummaries {
    Cards,
    Counts,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DashboardBudget {
    pub columns: usize,
    pub metric_count: usize,
    pub card_height: f32,
    /// The width one metric card takes in its row.
    ///
    /// The measured content width and the column count are the only inputs:
    /// the cards share the row minus the gaps between them, so the row fills
    /// the page slot exactly and no dead band survives at its right edge.
    pub card_width: f32,
    pub summaries: DashboardSummaries,
    /// Height left for the complete lower card groups after every fixed
    /// obligation and the bottom safety band have been charged.
    pub review_height: f32,
    /// The chrome rows the page paints above the metric slot.
    ///
    /// The page renders each row with its reserved height as a minimum, so
    /// the painted row and the reserved row are one fact.
    pub chrome_rows: DashboardChromeRows,
}

impl DashboardBudget {
    pub(crate) fn from_frame(frame: FrameBudget, ui_size: UiSize, density: RowDensity) -> Self {
        Self::from_page_layout(frame.page_layout(), ui_size, density)
    }

    pub(crate) fn from_page_layout(
        layout: PageLayoutBudget,
        ui_size: UiSize,
        density: RowDensity,
    ) -> Self {
        let width = layout.content_width.max(0.0);
        let height = layout.content_height.max(0.0);
        // The chrome rows are measured from the shared control ladder
        // through the shared chrome-row contract: the page paints exactly
        // what the budget reserves, so the metric cards never stop short of
        // the bottom of their own slot.
        let chrome_rows = DashboardChromeRows::from_page_layout(layout, ui_size, density);
        let body_gap = f32::from(tokens::SPACE_6);
        // Everything the page owes before the summary row and the metric
        // cards: the three chrome rows, the gap between the page's header
        // and its body column, the bottom safety band. The coverage readout
        // renders INSIDE the window controls row, so it is not a row of its
        // own and must not be charged as one.
        let fixed_rows = chrome_rows.total() + body_gap + BOTTOM_SAFETY_BAND;
        // A visible summary row adds one more row to the page, and therefore
        // one more gap between the page's rows. The bare layout keeps two.
        let fixed_with_summary = fixed_rows + ROOT_ROW_GAP * 3.0;
        let fixed_without_summary = fixed_rows + ROOT_ROW_GAP * 2.0;
        // The metric cards are MANDATORY: the summary row is an optional
        // whole group that only survives while a complete card group still
        // fits behind it. It degrades cards → counts → nothing, never half.
        let summaries =
            if width >= 900.0 && height - fixed_with_summary - SUMMARY_CARD_HEIGHT >= CARD_FLOOR {
                DashboardSummaries::Cards
            } else if height - fixed_with_summary - SUMMARY_COUNT_HEIGHT >= CARD_FLOOR {
                DashboardSummaries::Counts
            } else {
                DashboardSummaries::None
            };
        let (summary_height, fixed) = match summaries {
            DashboardSummaries::Cards => (SUMMARY_CARD_HEIGHT, fixed_with_summary),
            DashboardSummaries::Counts => (SUMMARY_COUNT_HEIGHT, fixed_with_summary),
            DashboardSummaries::None => (0.0, fixed_without_summary),
        };
        let review_height = (height - fixed - summary_height).max(0.0);
        let columns = if width >= 600.0 { 2 } else { 1 };
        // The metric row is a PLAIN grid: equal cards, one gap, no group
        // header and no overflow action. The shared card-width contract
        // owns the share arithmetic, so this row and the GPU insight row
        // can never disagree about how wide one card is.
        let card_width = admit_card_grid_width(
            width,
            columns,
            CardGridWidthSpec::new(CARD_GAP, GRID_COLUMN_CAP),
        )
        .card_width;
        // Whole-group admission and the card-height clamp belong to the
        // SHARED card-flow contract (ADR-064): this page supplies the
        // measured slot, its column count and its own card footprint, then
        // spends the verdict. No second copy of the arithmetic lives here.
        let flow = admit_card_groups_capped(
            review_height,
            columns,
            card_flow_spec(),
            max_metric_groups(columns),
        );
        Self {
            columns,
            metric_count: flow.cards(),
            card_height: flow.card_height,
            card_width,
            summaries,
            review_height,
            chrome_rows,
        }
    }

    /// The metric window this page shows, resolved against the metrics the
    /// product owns (the shared dashboard contract owns that arithmetic).
    pub(crate) fn metric_window(self, requested: usize) -> DashboardMetricWindow {
        DashboardMetricWindow::resolve(requested, self.metric_count)
    }

    /// The flex basis one metric card uses inside the shared adaptive grid.
    ///
    /// The basis decides when another card belongs on the next row, so it
    /// is rounded DOWN to a whole pixel: a fractional basis could exceed
    /// the measured row by a rounding error and push the second card onto
    /// a row of its own. The grid grows the cards into the row afterwards,
    /// so the rendered width is the exact share either way.
    pub(crate) fn card_basis(self) -> f32 {
        // The budget already clamps the reserved width at zero, so rounding
        // it down keeps the basis inside the measured row.
        self.card_width.floor()
    }
}

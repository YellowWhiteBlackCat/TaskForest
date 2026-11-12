//! Whole dashboard metric groups after root-owned shell and page chrome.

use taskmanager_theme::tokens;
use taskmanager_ui_contract::presentation::budget::PageLayoutBudget;
use taskmanager_ui_contract::presentation::card_flow::{CardFlowSpec, admit_card_groups_capped};
use taskmanager_ui_contract::presentation::dashboard::{DashboardMetricWindow, max_metric_groups};
use taskmanager_ui_contract::presentation::vocabulary::BOTTOM_SAFETY_BAND;

use super::FrameBudget;

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
pub(crate) const SECTION_SINGLE_ROW_HEIGHT: f32 = 40.0;
pub(crate) const SECTION_WRAPPED_HEIGHT: f32 = 72.0;
pub(crate) const WINDOW_CONTROLS_HEIGHT: f32 = 32.0;
pub(crate) const PAGING_HEIGHT: f32 = 32.0;
/// The dashboard root's vertical gap between its own rows.
pub(crate) const ROOT_ROW_GAP: f32 = tokens::SPACE_8.0;

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
}

/// The width one metric card takes in a row of `columns` cards.
///
/// The row's cards share the measured content width minus the gaps between
/// them. Deriving the width here keeps the column geometry in the budget
/// instead of a page-local fraction, so the rendered row and the budget can
/// never disagree.
fn metric_card_width(content_width: f32, columns: usize, gap: f32) -> f32 {
    let columns = columns.max(1);
    let gaps = gap * (columns.saturating_sub(1)) as f32;
    ((content_width - gaps) / columns as f32).max(0.0)
}

impl DashboardBudget {
    pub(crate) fn from_frame(frame: FrameBudget) -> Self {
        Self::from_page_layout(frame.page_layout())
    }

    pub(crate) fn from_page_layout(layout: PageLayoutBudget) -> Self {
        let width = layout.content_width.max(0.0);
        let height = layout.content_height.max(0.0);
        let section_height = if width < 1050.0 {
            SECTION_WRAPPED_HEIGHT
        } else {
            SECTION_SINGLE_ROW_HEIGHT
        };
        let body_gap = f32::from(tokens::SPACE_6);
        // Everything the page owes before the summary row and the metric
        // cards: the section actions, the fixed window-controls row, the
        // paging row, the gaps between the page's own rows, and the bottom
        // safety band. The coverage readout renders INSIDE the window
        // controls row, so it is not a row of its own and must not be
        // charged as one.
        let fixed_rows =
            section_height + WINDOW_CONTROLS_HEIGHT + PAGING_HEIGHT + body_gap + BOTTOM_SAFETY_BAND;
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
        let card_width = metric_card_width(width, columns, CARD_GAP);
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

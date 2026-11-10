//! Whole dashboard metric groups after root-owned shell and page chrome.

use taskmanager_theme::tokens;
use taskmanager_ui_contract::presentation::budget::PageLayoutBudget;
use taskmanager_ui_contract::presentation::card_flow::{CardFlowSpec, admit_card_groups_capped};
use taskmanager_ui_contract::presentation::vocabulary::BOTTOM_SAFETY_BAND;

use super::FrameBudget;

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_app/root/responsive/dashboard_budget_tests.rs"]
mod tests;

pub(crate) const CARD_FLOOR: f32 = 112.0;
pub(crate) const CARD_CEILING: f32 = 172.0;
pub(crate) const CARD_GAP: f32 = tokens::SPACE_8.0;
pub(crate) const SUMMARY_CARD_HEIGHT: f32 = 88.0;
pub(crate) const SUMMARY_COUNT_HEIGHT: f32 = 24.0;
pub(crate) const SECTION_SINGLE_ROW_HEIGHT: f32 = 40.0;
pub(crate) const SECTION_WRAPPED_HEIGHT: f32 = 72.0;
pub(crate) const WINDOW_CONTROLS_HEIGHT: f32 = 32.0;
pub(crate) const PAGING_HEIGHT: f32 = 32.0;
pub(crate) const COVERAGE_HEIGHT: f32 = 24.0;
/// The dashboard owns four metrics at most; a two-column layout spends the
/// same four metrics in two groups.
const MAX_METRIC_GROUPS: usize = 4;

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
    pub summaries: DashboardSummaries,
    /// Height left for the complete lower card groups after every fixed
    /// obligation and the bottom safety band have been charged.
    pub review_height: f32,
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
        let content_gap = f32::from(tokens::SPACE_8);
        // Everything the page owes before the summary row and the metric
        // cards: the section actions, the fixed window/coverage/paging rows,
        // the gaps between them, and the bottom safety band.
        let fixed_without_summary = section_height
            + WINDOW_CONTROLS_HEIGHT
            + PAGING_HEIGHT
            + COVERAGE_HEIGHT
            + body_gap
            + content_gap * 2.0
            + BOTTOM_SAFETY_BAND;
        // The metric cards are MANDATORY: the summary row is an optional
        // whole group that only survives while a complete card group still
        // fits behind it. It degrades cards → counts → nothing, never half.
        let summaries = if width >= 900.0
            && height - fixed_without_summary - SUMMARY_CARD_HEIGHT >= CARD_FLOOR
        {
            DashboardSummaries::Cards
        } else if height - fixed_without_summary - SUMMARY_COUNT_HEIGHT >= CARD_FLOOR {
            DashboardSummaries::Counts
        } else {
            DashboardSummaries::None
        };
        let summary_height = match summaries {
            DashboardSummaries::Cards => SUMMARY_CARD_HEIGHT,
            DashboardSummaries::Counts => SUMMARY_COUNT_HEIGHT,
            DashboardSummaries::None => 0.0,
        };
        let fixed = fixed_without_summary + summary_height;
        let review_height = (height - fixed).max(0.0);
        let columns = if width >= 600.0 { 2 } else { 1 };
        // Whole-group admission and the card-height clamp belong to the
        // SHARED card-flow contract (ADR-064): this page supplies the
        // measured slot, its column count and its own card footprint, then
        // spends the verdict. No second copy of the arithmetic lives here.
        let flow = admit_card_groups_capped(
            review_height,
            columns,
            card_flow_spec(),
            MAX_METRIC_GROUPS / columns,
        );
        Self {
            columns,
            metric_count: flow.cards(),
            card_height: flow.card_height,
            summaries,
            review_height,
        }
    }

    pub(crate) fn first_metric(self, requested: usize) -> usize {
        requested.min(MAX_METRIC_GROUPS.saturating_sub(self.metric_count.max(1)))
    }
}

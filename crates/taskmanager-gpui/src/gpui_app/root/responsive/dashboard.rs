//! Whole dashboard metric groups after root-owned shell and page chrome.

use super::FrameBudget;

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
}

impl DashboardBudget {
    pub(crate) fn from_frame(frame: FrameBudget) -> Self {
        let width = f32::from(frame.content.size.width);
        let height = f32::from(frame.content.size.height);
        let summaries = if width >= 900.0 && height >= 500.0 {
            DashboardSummaries::Cards
        } else if height >= 300.0 {
            DashboardSummaries::Counts
        } else {
            DashboardSummaries::None
        };
        let summary_height = match summaries {
            DashboardSummaries::Cards => 88.0,
            DashboardSummaries::Counts => 24.0,
            DashboardSummaries::None => 0.0,
        };
        // Section actions can wrap as one complete group; the remaining fixed
        // rows are window/coverage, paging, gaps, and the bottom safety inset.
        let section_height = if width < 1050.0 { 72.0 } else { 40.0 };
        let available =
            (height - section_height - summary_height - 32.0 - 32.0 - 24.0 - 12.0).max(0.0);
        let columns = if width >= 600.0 { 2 } else { 1 };
        let rows = (((available + 8.0) / 120.0).floor() as usize).min(4 / columns);
        let metric_count = rows * columns;
        let card_height = if rows == 0 {
            0.0
        } else {
            ((available - rows.saturating_sub(1) as f32 * 8.0) / rows as f32).clamp(112.0, 172.0)
        };
        Self {
            columns,
            metric_count,
            card_height,
            summaries,
        }
    }

    pub(crate) fn first_metric(self, requested: usize) -> usize {
        requested.min(4_usize.saturating_sub(self.metric_count.max(1)))
    }
}

//! Terminal health review state and bounded navigation over cached projections.
use crate::TuiApp;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HealthReviewMode {
    #[default]
    Rules,
    Storage,
    Sensors,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct HealthReviewState {
    pub(crate) mode: HealthReviewMode,
    pub(crate) selected: usize,
}
impl TuiApp {
    pub(crate) fn select_health_review(&mut self, mode: HealthReviewMode) {
        self.health_review = HealthReviewState { mode, selected: 0 };
    }
    pub(crate) fn health_review_move(&mut self, delta: isize) {
        let count = crate::ui::health_review::groups(self).len();
        self.health_review.selected = self
            .health_review
            .selected
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
    }
}

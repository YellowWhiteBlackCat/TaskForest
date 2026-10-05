//! Terminal health review state and bounded navigation over cached projections.
use crate::TuiApp;
use taskmanager_core::core::alerts::{AlertEventKind, export_alert_events_json};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HealthReviewMode {
    #[default]
    Rules,
    Storage,
    Sensors,
    ActiveAlerts,
    Events,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct HealthReviewState {
    pub(crate) mode: HealthReviewMode,
    pub(crate) selected: usize,
    pub(crate) event_filter: Option<AlertEventKind>,
}
impl TuiApp {
    pub(crate) fn select_health_review(&mut self, mode: HealthReviewMode) {
        self.health_review = HealthReviewState {
            mode,
            ..Default::default()
        };
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

impl TuiApp {
    pub(crate) fn filter_event_history(&mut self) {
        self.health_review.event_filter = match self.health_review.event_filter {
            None => Some(AlertEventKind::Activated),
            Some(AlertEventKind::Activated) => Some(AlertEventKind::Cleared),
            Some(AlertEventKind::Cleared) => None,
        };
        self.health_review.selected = 0;
    }
    pub(crate) fn export_event_history_to(&mut self, sink: &mut impl std::io::Write) {
        let result = export_alert_events_json(self.projection().alert_center.event_history())
            .map_err(|error| error.to_string())
            .and_then(|json| {
                crate::clipboard::write_clipboard(sink, &json).map_err(|error| error.to_string())
            });
        let (severity, message) = match result {
            Ok(()) => (
                FeedbackSeverity::Success,
                "Events sent to terminal clipboard".to_owned(),
            ),
            Err(error) => (FeedbackSeverity::Error, error),
        };
        self.report_notice(
            FeedbackSource::Clipboard,
            severity,
            FeedbackLifecycle::UntilReplaced,
            message,
        );
    }
}

#[cfg(test)]
#[path = "../tests/headless/event_center.rs"]
mod tests;

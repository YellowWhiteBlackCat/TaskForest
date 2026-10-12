use super::frame_text;
use crate::demo_app;
use crate::health_review::HealthReviewMode;
use taskmanager_application::i18n::t;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};
use taskmanager_test_support::pin_english;

#[test]
fn health_review_owner_hides_global_shortcuts_and_keeps_footer_status_and_feedback() {
    pin_english();
    let mut app = demo_app();
    app.report_notice(
        FeedbackSource::Demo,
        FeedbackSeverity::Success,
        FeedbackLifecycle::UntilReplaced,
        "Health review refreshed",
    );
    app.toggle_health();
    app.select_health_review(HealthReviewMode::Events);

    let text = frame_text(&app, 120, 36);
    let footer_start = text.lines().count().saturating_sub(3);
    let footer = text
        .lines()
        .skip(footer_start)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        footer.contains(t("common.live")),
        "the root footer keeps its live status:\n{footer}"
    );
    assert!(
        footer.contains("Health review refreshed"),
        "the root footer keeps its feedback:\n{footer}"
    );
    assert!(
        !footer.contains(t("footer.shortcuts")),
        "global footer shortcuts must be hidden while HealthReview owns input:\n{footer}"
    );
    for local_hint in [
        "Up/Down groups · p Alerts · e Event center",
        "w Storage health · s Sensors · q Alert rules",
        "F2 Filter",
        "F5 Export",
        "F8 Clear",
        "h / Esc Close",
    ] {
        assert!(
            text.contains(local_hint),
            "the HealthReview owner keeps its local hint {local_hint:?}:\n{text}"
        );
    }
}

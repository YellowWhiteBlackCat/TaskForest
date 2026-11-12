use zhuge_ui_tokens::{ALL_STATUS_KINDS, StatusKind};

use crate::app::FeedbackSeverity;
use crate::presentation::{ALERT_MARKER, SUCCESS_MARKER, status_marker};

/// Every product severity projects onto exactly one shared status kind, so
/// the shared ink ladder resolves a notice identically in every frontend.
#[test]
fn every_severity_projects_onto_one_shared_status_kind() {
    let severities = [
        FeedbackSeverity::Info,
        FeedbackSeverity::Success,
        FeedbackSeverity::Warning,
        FeedbackSeverity::Error,
    ];
    let mut resolved: Vec<StatusKind> = severities
        .iter()
        .map(|severity| severity.status_kind())
        .collect();
    resolved.sort_by_key(|kind| ALL_STATUS_KINDS.iter().position(|k| k == kind));
    assert_eq!(
        resolved,
        ALL_STATUS_KINDS.to_vec(),
        "the four product severities must cover the shared status ladder exactly once"
    );
}

/// The shared ink ladder keeps its own meaning: information wears the
/// accent, a completed action the success ink, a caution the warning ink and
/// a failure the danger ink. A frontend resolves these through
/// `StatusColors::resolve`, so the correspondence must be the documented one.
#[test]
fn the_projected_status_kind_keeps_the_documented_ink_meaning() {
    assert_eq!(
        FeedbackSeverity::Info.status_kind(),
        StatusKind::Info,
        "ordinary information resolves to the accent ink"
    );
    assert_eq!(FeedbackSeverity::Success.status_kind(), StatusKind::Success);
    assert_eq!(FeedbackSeverity::Warning.status_kind(), StatusKind::Warning);
    assert_eq!(
        FeedbackSeverity::Error.status_kind(),
        StatusKind::Danger,
        "the shell's error is the shared ladder's danger"
    );
}

/// The text form of the status glyph has ONE owner. A terminal surface wears
/// the same meaning the graphical frontends draw as an icon: an informational
/// notice wears no marker at all, a completed action the check, and both
/// caution and failure the alert marker.
#[test]
fn every_status_kind_answers_with_its_own_text_marker() {
    assert_eq!(
        status_marker(StatusKind::Info),
        None,
        "an informational notice is its copy, not a decoration"
    );
    assert_eq!(status_marker(StatusKind::Success), Some(SUCCESS_MARKER));
    assert_eq!(status_marker(StatusKind::Warning), Some(ALERT_MARKER));
    assert_eq!(status_marker(StatusKind::Danger), Some(ALERT_MARKER));
}

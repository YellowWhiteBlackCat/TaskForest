use std::sync::Arc;

use taskmanager_application::{
    BootBaselineCompletion, BootBaselineCompletionDisposition, BootBaselineCompletionOutcome,
    BootBaselineController, BootBaselineError, BootBaselineErrorKind, BootBaselineRecordKind,
    BootBaselineState, BootBaselineSubmission, BootTimeline, BootTimelineSegment,
};

fn timeline(duration_ms: u64) -> BootTimeline {
    BootTimeline {
        total_ms: duration_ms.saturating_add(10),
        segments: vec![BootTimelineSegment {
            unit: "taskforest.service".to_owned(),
            start_ms: 10,
            end_ms: duration_ms.saturating_add(10),
            duration_ms,
        }],
        collapsed_count: 0,
        untimed_count: 0,
        untimed_units: Vec::new(),
    }
}

#[test]
fn duplicate_evidence_is_suppressed_after_admission_and_success() {
    let mut baseline = BootBaselineController::default();
    let BootBaselineSubmission::Issued(request) = baseline.observe(timeline(20), 1_000) else {
        panic!("first evidence must issue")
    };
    assert_eq!(
        baseline.observe(timeline(20), 1_001),
        BootBaselineSubmission::DuplicateIgnored
    );
    assert_eq!(
        baseline.complete(BootBaselineCompletion {
            request: request.clone(),
            outcome: BootBaselineCompletionOutcome::Recorded {
                kind: BootBaselineRecordKind::NewBoot,
                previous: None,
            },
        }),
        BootBaselineCompletionDisposition::Applied
    );
    assert_eq!(
        baseline.observe(timeline(20), 1_002),
        BootBaselineSubmission::DuplicateIgnored
    );
    assert!(matches!(baseline.state(), BootBaselineState::Ready(_)));
}

#[test]
fn late_completion_cannot_replace_newer_evidence() {
    let mut baseline = BootBaselineController::default();
    let BootBaselineSubmission::Issued(first) = baseline.observe(timeline(20), 1_000) else {
        panic!("first evidence must issue")
    };
    let BootBaselineSubmission::Issued(current) = baseline.observe(timeline(30), 1_100) else {
        panic!("expanded evidence must issue")
    };
    assert_eq!(
        baseline.complete(BootBaselineCompletion {
            request: first,
            outcome: BootBaselineCompletionOutcome::Recorded {
                kind: BootBaselineRecordKind::NewBoot,
                previous: None,
            },
        }),
        BootBaselineCompletionDisposition::StaleIgnored
    );
    assert_eq!(
        baseline.complete(BootBaselineCompletion {
            request: current,
            outcome: BootBaselineCompletionOutcome::Recorded {
                kind: BootBaselineRecordKind::SameBoot,
                previous: Some(Arc::new(timeline(10))),
            },
        }),
        BootBaselineCompletionDisposition::Applied
    );
    assert_eq!(
        baseline.previous_for_current_evidence(),
        Some(&timeline(10))
    );
}

#[test]
fn failed_write_keeps_last_good_and_allows_same_evidence_retry() {
    let mut baseline = BootBaselineController::default();
    let BootBaselineSubmission::Issued(first) = baseline.observe(timeline(20), 1_000) else {
        panic!("first evidence must issue")
    };
    baseline.complete(BootBaselineCompletion {
        request: first,
        outcome: BootBaselineCompletionOutcome::Recorded {
            kind: BootBaselineRecordKind::NewBoot,
            previous: Some(Arc::new(timeline(10))),
        },
    });
    let BootBaselineSubmission::Issued(failed) = baseline.observe(timeline(30), 2_000) else {
        panic!("changed evidence must issue")
    };
    assert_eq!(baseline.previous_for_current_evidence(), None);
    assert_eq!(baseline.last_good_previous(), Some(&timeline(10)));
    baseline.reject_submission(
        failed,
        BootBaselineError::new(BootBaselineErrorKind::Write, "fixture failure"),
    );
    assert_eq!(baseline.previous_for_current_evidence(), None);
    assert_eq!(baseline.last_good_previous(), Some(&timeline(10)));
    assert!(matches!(
        baseline.observe(timeline(30), 2_100),
        BootBaselineSubmission::Issued(_)
    ));
}

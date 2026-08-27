use super::{
    MergedSourceState, SourceNotice, SourceStateKind, device_source_line, merge_source_lines,
    source_line, source_lines, source_notice, source_status_from_operation_failure, truncate_text,
};
use taskmanager_core::{DeviceStatus, FailureKind, ProviderId, SourceOutcome, SourceStatus};
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, OperationFailure, RequestIdGenerator, RetryDisposition,
};

fn source(outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("test.provider"),
        outcome,
        item_count: 0,
    }
}

#[test]
fn healthy_and_empty_sources_do_not_create_a_notice() {
    assert_eq!(
        source_notice(&[
            source(SourceOutcome::Available),
            source(SourceOutcome::Empty),
        ]),
        None
    );
    assert_eq!(source_notice(&[]), None);
}

#[test]
fn unavailable_source_takes_precedence_over_an_earlier_partial_source() {
    assert_eq!(
        source_notice(&[
            source(SourceOutcome::Partial(FailureKind::TimedOut)),
            source(SourceOutcome::Unavailable(FailureKind::Unsupported)),
        ]),
        Some(SourceNotice::Unavailable(FailureKind::Unsupported))
    );
}

#[test]
fn retry_policy_does_not_promise_recovery_without_a_capability_change() {
    assert!(SourceNotice::Partial(FailureKind::TimedOut).is_retryable());
    assert_eq!(
        SourceNotice::Unavailable(FailureKind::TemporarilyUnavailable).retry(),
        RetryDisposition::RetryLater
    );
    assert!(!SourceNotice::Unavailable(FailureKind::PermissionDenied).is_retryable());
    assert!(!SourceNotice::Unavailable(FailureKind::MissingDependency).is_retryable());
    assert!(!SourceNotice::Unavailable(FailureKind::Unsupported).is_retryable());
}

#[test]
fn operation_failure_becomes_unavailable_source_without_fabricating_zero_rows() {
    let mut request_ids = RequestIdGenerator::default();
    let failure = OperationFailure {
        request_id: request_ids.next_id(),
        capability: CapabilityId::SERVICES,
        sequence: EventSequence::new(3),
        kind: FailureKind::TimedOut,
        retry: RetryDisposition::RetryLater,
        provider: Some(ProviderId::borrowed("test.services")),
        observed_at_ms: 42,
    };
    let status = source_status_from_operation_failure(&failure, 4);
    assert_eq!(status.provider, ProviderId::borrowed("test.services"));
    assert_eq!(status.item_count, 4);
    assert_eq!(
        status.outcome,
        SourceOutcome::Unavailable(FailureKind::TimedOut)
    );
}

#[test]
fn every_outcome_maps_to_its_neutral_kind() {
    let available = source_line(&source(SourceOutcome::Available));
    assert_eq!(available.state, SourceStateKind::Ok);
    assert_eq!(available.failure, None);
    assert_eq!(available.origin, "test.provider");

    // A confirmed empty answer is healthy, not a gap.
    let empty = source_line(&source(SourceOutcome::Empty));
    assert_eq!(empty.state, SourceStateKind::Ok);
    assert_eq!(empty.failure, None);

    let partial = source_line(&source(SourceOutcome::Partial(
        FailureKind::PermissionDenied,
    )));
    assert_eq!(partial.state, SourceStateKind::Degraded);
    assert_eq!(partial.failure, Some(FailureKind::PermissionDenied));

    // An unanswered source with no visible rows is failed; with rows it
    // is stale-but-visible, and the count travels with the line.
    let failed = source_line(&source(SourceOutcome::Unavailable(FailureKind::TimedOut)));
    assert_eq!(failed.state, SourceStateKind::Failed);
    assert_eq!(failed.item_count, 0);
    let mut stale_status = source(SourceOutcome::Unavailable(FailureKind::TimedOut));
    stale_status.item_count = 7;
    let stale = source_line(&stale_status);
    assert_eq!(stale.state, SourceStateKind::Stale);
    assert_eq!(stale.item_count, 7);
    assert_eq!(stale.failure, Some(FailureKind::TimedOut));
}

#[test]
fn device_status_maps_to_neutral_kind_with_its_typed_cause() {
    let cases = [
        (DeviceStatus::Healthy, SourceStateKind::Ok, None),
        (
            DeviceStatus::Stale,
            SourceStateKind::Stale,
            Some(FailureKind::TemporarilyUnavailable),
        ),
        (
            DeviceStatus::PermissionDenied,
            SourceStateKind::Failed,
            Some(FailureKind::PermissionDenied),
        ),
        (
            DeviceStatus::MissingTool,
            SourceStateKind::Degraded,
            Some(FailureKind::MissingDependency),
        ),
        (
            DeviceStatus::Unsupported,
            SourceStateKind::Unknown,
            Some(FailureKind::Unsupported),
        ),
    ];
    for (status, kind, failure) in cases {
        let line = device_source_line(
            &ProviderId::borrowed("linux.proc"),
            &taskmanager_core::DeviceState {
                status,
                last_success_ms: Some(12),
            },
        );
        assert_eq!(line.state, kind, "kind for {status:?}");
        assert_eq!(line.failure, failure, "typed cause for {status:?}");
        assert_eq!(line.origin, "linux.proc");
        assert_eq!(line.item_count, 0);
    }
}

#[test]
fn merged_kind_names_the_first_worst_source_and_matches_source_notice() {
    // Stale and failed share the top tier: the first reporter wins.
    let mut with_rows = source(SourceOutcome::Unavailable(FailureKind::TimedOut));
    with_rows.item_count = 3;
    let failed = source(SourceOutcome::Unavailable(FailureKind::Unsupported));
    assert_eq!(
        merge_source_lines(&[with_rows, failed]),
        Some(MergedSourceState {
            kind: SourceStateKind::Stale,
            notice: SourceNotice::Unavailable(FailureKind::TimedOut),
        })
    );
    // A hard failure still outranks a partial answer, whichever came first.
    let failed = source(SourceOutcome::Unavailable(FailureKind::Unsupported));
    let partial_first = [
        source(SourceOutcome::Partial(FailureKind::Rejected)),
        failed.clone(),
    ];
    let merged = merge_source_lines(&partial_first);
    assert_eq!(
        merged.map(|state| state.kind),
        Some(SourceStateKind::Failed)
    );
    assert_eq!(
        merged.map(|state| state.notice),
        source_notice(&partial_first)
    );
    // Healthy-only input has no headline.
    assert_eq!(
        merge_source_lines(&[source(SourceOutcome::Available)]),
        None
    );
}

#[test]
fn source_lines_preserve_provider_order_and_content() {
    let mut second = source(SourceOutcome::Partial(FailureKind::TimedOut));
    second.provider = ProviderId::owned("zzz.late");
    let mut first = source(SourceOutcome::Available);
    first.provider = ProviderId::owned("aaa.early");
    let lines = source_lines(&[first.clone(), second]);
    assert_eq!(
        lines
            .iter()
            .map(|line| line.origin.as_str())
            .collect::<Vec<_>>(),
        vec!["aaa.early", "zzz.late"],
        "the order in is the order out"
    );
    // The aggregate and the per-source fold agree entry by entry.
    assert_eq!(lines.first().cloned(), Some(source_line(&first)));
    assert_eq!(source_line(&first).state, SourceStateKind::Ok);
}

#[test]
fn truncation_respects_char_boundaries_and_limits() {
    assert_eq!(truncate_text("", 4), "");
    assert_eq!(truncate_text("abcd", 4), "abcd");
    assert_eq!(truncate_text("abcde", 4), "abc…");
    assert_eq!(truncate_text("abcdef", 1), "…");
    assert_eq!(truncate_text("abcdef", 0), "");
    // Multi-byte text is truncated at char, not byte, boundaries.
    assert_eq!(truncate_text("任务管理器任务", 5), "任务管理…");
    assert_eq!(truncate_text("任务", 5), "任务");
}

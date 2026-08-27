use super::*;

#[test]
fn log_provider_failure_preserves_success_and_recovery_time() {
    let mut state = ServiceLogProviderState::default();
    state.observe_success(false, 100);
    state.observe_failure(ServiceLogFailure::with_detail(
        ServiceLogErrorKind::TimedOut,
        "journalctl timed out",
    ));

    assert_eq!(state.availability, ServiceLogAvailability::Stale);
    assert_eq!(state.last_success_ms, Some(100));
    assert_eq!(
        state.failure.as_ref().map(|failure| failure.kind),
        Some(ServiceLogErrorKind::TimedOut)
    );

    state.observe_success(false, 300);
    assert_eq!(state.availability, ServiceLogAvailability::Available);
    assert_eq!(state.last_success_ms, Some(300));
    assert_eq!(state.failure, None);
}

#[test]
fn service_log_contract_serializes_stable_snake_case_kinds() {
    assert_eq!(
        serde_json::to_string(&ServiceLogErrorKind::MissingTool).unwrap(),
        "\"missing_tool\""
    );
    assert_eq!(
        serde_json::to_string(&ServiceLogAvailability::Stale).unwrap(),
        "\"stale\""
    );
    assert_eq!(
        serde_json::to_string(&ServiceLogStreamEnd::CaughtUp).unwrap(),
        r#"{"reason":"caught_up"}"#
    );
    let legacy: ServiceLogProviderState =
        serde_json::from_str(r#"{"availability":"available","failure":null,"last_success_ms":42}"#)
            .unwrap();
    assert_eq!(legacy.stream_end, None);
}

#[test]
fn ready_batches_are_non_empty_by_construction() {
    assert_eq!(
        ServiceLogState::from_lines(Vec::new()),
        ServiceLogState::Empty
    );
    assert!(ServiceLogLines::new(Vec::new()).is_none());

    let initial_query = ServiceLogQuery {
        service_id: "demo".into(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    assert_eq!(
        ServiceLogStreamState::from_query_entries(&initial_query, Vec::new()),
        ServiceLogStreamState::Empty
    );
    assert!(ServiceLogEntries::new(Vec::new()).is_none());

    let follow_query = ServiceLogQuery {
        after_cursor: Some("cursor".into()),
        ..initial_query
    };
    assert_eq!(
        ServiceLogStreamState::from_query_entries(&follow_query, Vec::new()),
        ServiceLogStreamState::Ended(ServiceLogStreamEnd::CaughtUp)
    );
}

#[test]
fn disconnected_stream_is_not_a_provider_success_or_empty_log() {
    let mut feed = ServiceLogFeed::default();
    let query = feed
        .next_follow_query(&ServiceId::new("fixture:demo"))
        .expect("default feed follows");
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query,
            state: ServiceLogStreamState::Ended(ServiceLogStreamEnd::disconnected(
                "worker channel closed",
            )),
        },
        100,
    );

    assert_eq!(
        feed.provider.availability,
        ServiceLogAvailability::Disconnected
    );
    assert_eq!(feed.provider.last_success_ms, None);
    assert!(matches!(
        feed.provider.stream_end,
        Some(ServiceLogStreamEnd::Disconnected { .. })
    ));
}

#[test]
fn frontend_line_resolution_uses_shared_stream_lifecycle_policy() {
    let empty = ServiceLogState::Empty;
    assert_eq!(
        ServiceLogStreamState::Loading.resolve_lines(&empty, Vec::new()),
        ServiceLogState::Loading
    );
    assert_eq!(
        ServiceLogStreamState::Ended(ServiceLogStreamEnd::CaughtUp)
            .resolve_lines(&empty, Vec::new()),
        ServiceLogState::Empty
    );
    assert!(matches!(
        ServiceLogStreamState::Ended(ServiceLogStreamEnd::disconnected("closed"))
            .resolve_lines(&empty, Vec::new()),
        ServiceLogState::Unavailable(ServiceLogFailure {
            kind: ServiceLogErrorKind::TemporarilyUnavailable,
            ..
        })
    ));
    assert!(matches!(
        ServiceLogStreamState::Ended(ServiceLogStreamEnd::CaughtUp)
            .resolve_lines(&empty, vec!["entry".into()]),
        ServiceLogState::Ready(_)
    ));
}

#[test]
fn follow_feed_is_bounded_but_keeps_the_latest_cursor_after_trim() {
    let service_id = ServiceId::new("fixture:demo");
    let query = ServiceLogQuery {
        service_id: service_id.clone(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    let total = SERVICE_LOG_FEED_CAPACITY + 3;
    let entries = (0..total)
        .map(|index| ServiceLogEntry {
            cursor: format!("j:{index}"),
            realtime_timestamp_micros: None,
            priority: Some(6),
            level: ServiceLogLevel::Info,
            message: format!("entry {index}"),
        })
        .collect();
    let mut feed = ServiceLogFeed::default();
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: query.clone(),
            state: ServiceLogStreamState::from_query_entries(&query, entries),
        },
        100,
    );

    let oldest = format!("j:{}", total - SERVICE_LOG_FEED_CAPACITY);
    let newest = format!("j:{}", total - 1);
    assert_eq!(feed.entries().len(), SERVICE_LOG_FEED_CAPACITY);
    assert_eq!(
        feed.entries().first().map(|entry| entry.cursor.as_str()),
        Some(oldest.as_str())
    );
    assert_eq!(feed.last_cursor(), Some(newest.as_str()));
    assert_eq!(
        feed.next_follow_query(&service_id)
            .and_then(|query| query.after_cursor),
        Some(newest.clone())
    );

    // A duplicate batch must not grow the feed, while a new cursor still
    // advances the follow position and evicts exactly one oldest row.
    let follow_query = ServiceLogQuery {
        after_cursor: Some(newest),
        ..query
    };
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: follow_query.clone(),
            state: ServiceLogStreamState::from_query_entries(
                &follow_query,
                vec![
                    ServiceLogEntry {
                        cursor: "j:2001".into(),
                        realtime_timestamp_micros: None,
                        priority: Some(6),
                        level: ServiceLogLevel::Info,
                        message: "duplicate-old".into(),
                    },
                    ServiceLogEntry {
                        cursor: "j:2002".into(),
                        realtime_timestamp_micros: None,
                        priority: Some(6),
                        level: ServiceLogLevel::Info,
                        message: "duplicate-newest".into(),
                    },
                    ServiceLogEntry {
                        cursor: "j:2003".into(),
                        realtime_timestamp_micros: None,
                        priority: Some(6),
                        level: ServiceLogLevel::Info,
                        message: "new".into(),
                    },
                ],
            ),
        },
        200,
    );
    assert_eq!(feed.entries().len(), SERVICE_LOG_FEED_CAPACITY);
    let next_oldest = format!("j:{}", total - SERVICE_LOG_FEED_CAPACITY + 1);
    assert_eq!(
        feed.entries().first().map(|entry| entry.cursor.as_str()),
        Some(next_oldest.as_str())
    );
    assert_eq!(feed.last_cursor(), Some("j:2003"));
}

#[test]
fn shared_failures_map_to_actionable_service_log_reasons() {
    for (failure, expected) in [
        (
            FailureKind::PermissionDenied,
            ServiceLogErrorKind::PermissionDenied,
        ),
        (
            FailureKind::MissingDependency,
            ServiceLogErrorKind::MissingTool,
        ),
        (FailureKind::TimedOut, ServiceLogErrorKind::TimedOut),
        (FailureKind::Unsupported, ServiceLogErrorKind::Unsupported),
        (
            FailureKind::TemporarilyUnavailable,
            ServiceLogErrorKind::TemporarilyUnavailable,
        ),
        (FailureKind::Rejected, ServiceLogErrorKind::ProviderFailed),
        (
            FailureKind::IdentityChanged,
            ServiceLogErrorKind::ProviderFailed,
        ),
        (
            FailureKind::ProviderFault,
            ServiceLogErrorKind::ProviderFailed,
        ),
    ] {
        assert_eq!(ServiceLogErrorKind::from_failure(failure), expected);
    }
}

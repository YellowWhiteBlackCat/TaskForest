use super::*;
use crate::engine::services::target::systemd_service_id;

fn service_id() -> taskmanager_core::ServiceId {
    systemd_service_id("demo.service")
}

fn entry(cursor: &str, priority: u8, timestamp: u64) -> ServiceLogEntry {
    ServiceLogEntry {
        cursor: cursor.into(),
        realtime_timestamp_micros: Some(timestamp),
        priority: Some(priority),
        level: level_for_priority(Some(priority)),
        message: cursor.into(),
    }
}

fn query(after_cursor: Option<&str>) -> ServiceLogQuery {
    ServiceLogQuery {
        service_id: service_id(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: after_cursor.map(str::to_string),
    }
}

fn ready(entries: Vec<ServiceLogEntry>) -> ServiceLogStreamState {
    ServiceLogStreamState::from_query_entries(&query(None), entries)
}

#[test]
fn parses_cursor_timestamp_priority_and_message() {
    let fixture = r#"{"__CURSOR":"s=1","__REALTIME_TIMESTAMP":"1000000","PRIORITY":"3","MESSAGE":"failed"}
{"__CURSOR":"s=2","__REALTIME_TIMESTAMP":"2000000","PRIORITY":"6","MESSAGE":"ready"}"#;
    let entries = parse_journal_json_lines(fixture).expect("valid journal fixture");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, ServiceLogLevel::Error);
    assert_eq!(entries[1].message, "ready");
}

#[test]
fn paused_feed_issues_no_follow_query_and_applies_no_result() {
    let mut feed = ServiceLogFeed::default();
    feed.paused = true;
    assert!(feed.next_follow_query(&service_id()).is_none());
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: ServiceLogQuery {
                service_id: service_id(),
                level: ServiceLogLevelFilter::All,
                time: ServiceLogTimeFilter::All,
                after_cursor: None,
            },
            state: ready(vec![entry("one", 6, 10)]),
        },
        10,
    );
    assert!(feed.entries().is_empty());
}

#[test]
fn follow_merges_by_cursor_and_filters_level_and_time() {
    let now = 10_000_000_000;
    let mut feed = ServiceLogFeed::default();
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: ServiceLogQuery {
                service_id: service_id(),
                level: ServiceLogLevelFilter::All,
                time: ServiceLogTimeFilter::All,
                after_cursor: None,
            },
            state: ready(vec![entry("old", 6, now - 2 * 60 * 60 * 1_000_000)]),
        },
        now / 1_000,
    );
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: feed.next_follow_query(&service_id()).unwrap(),
            state: ready(vec![
                entry("old", 6, now),
                entry("warn", 4, now - 1_000),
                entry("debug", 7, now - 1_000),
            ]),
        },
        now / 1_000,
    );
    assert_eq!(feed.entries().len(), 3);
    feed.level = ServiceLogLevelFilter::WarningsAndErrors;
    feed.time = ServiceLogTimeFilter::LastHour;
    let visible = feed.visible_entries(now);
    assert_eq!(
        visible
            .iter()
            .map(|entry| entry.cursor.as_str())
            .collect::<Vec<_>>(),
        ["warn"]
    );
    assert_eq!(
        feed.next_follow_query(&service_id())
            .unwrap()
            .after_cursor
            .as_deref(),
        Some("debug")
    );
}

#[test]
fn disconnected_follow_preserves_entries_cursor_and_last_success_until_recovery() {
    let mut feed = ServiceLogFeed::default();
    let initial_query = feed.next_follow_query(&service_id()).unwrap();
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: initial_query,
            state: ready(vec![entry("one", 6, 10)]),
        },
        100,
    );

    let failed_query = feed.next_follow_query(&service_id()).unwrap();
    assert_eq!(failed_query.after_cursor.as_deref(), Some("one"));
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: failed_query,
            state: ServiceLogStreamState::Ended(ServiceLogStreamEnd::disconnected(
                "service log stream worker disconnected",
            )),
        },
        200,
    );

    assert_eq!(feed.entries().len(), 1);
    assert_eq!(feed.provider.availability, ServiceLogAvailability::Stale);
    assert_eq!(feed.provider.last_success_ms, Some(100));
    assert_eq!(feed.provider.failure, None);
    assert!(matches!(
        feed.provider.stream_end,
        Some(ServiceLogStreamEnd::Disconnected { .. })
    ));
    assert_eq!(
        feed.next_follow_query(&service_id())
            .unwrap()
            .after_cursor
            .as_deref(),
        Some("one")
    );

    let recovery_query = feed.next_follow_query(&service_id()).unwrap();
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: recovery_query,
            state: ready(vec![entry("one", 6, 10), entry("two", 6, 20)]),
        },
        300,
    );

    assert_eq!(
        feed.entries()
            .iter()
            .map(|entry| entry.cursor.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert_eq!(
        feed.provider.availability,
        ServiceLogAvailability::Available
    );
    assert_eq!(feed.provider.last_success_ms, Some(300));
    assert_eq!(feed.provider.failure, None);
    assert_eq!(feed.provider.stream_end, None);
}

#[test]
fn initial_stream_failure_is_typed_unavailable() {
    let mut feed = ServiceLogFeed::default();
    feed.apply_at(
        ServiceLogStreamSnapshot {
            query: feed.next_follow_query(&service_id()).unwrap(),
            state: ServiceLogStreamState::Unavailable(ServiceLogFailure::with_detail(
                ServiceLogErrorKind::MissingTool,
                "journalctl was not found",
            )),
        },
        100,
    );

    assert_eq!(
        feed.provider.availability,
        ServiceLogAvailability::Unavailable
    );
    assert_eq!(feed.provider.last_success_ms, None);
    assert_eq!(
        feed.provider.failure.as_ref().map(|failure| failure.kind),
        Some(ServiceLogErrorKind::MissingTool)
    );
}

#[test]
fn disconnected_stream_worker_is_typed_and_can_be_replaced_for_recovery() {
    let _clocked_worker = ServiceLogStreamWorker::new(|| 42);
    let mut worker = ServiceLogStreamWorker::disconnected();
    let query = ServiceLogQuery {
        service_id: service_id(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    let error = worker
        .request(ServiceLogQuery {
            service_id: service_id(),
            level: ServiceLogLevelFilter::All,
            time: ServiceLogTimeFilter::All,
            after_cursor: None,
        })
        .expect_err("disconnected worker must reject requests");

    assert_eq!(error, ServiceLogStreamRequestError::Disconnected);
    assert!(matches!(
        error.into_state(),
        ServiceLogStreamState::Ended(ServiceLogStreamEnd::Disconnected {
            detail: Some(detail)
        }) if detail.contains("unavailable")
    ));

    worker = ServiceLogStreamWorker::with_fetcher(
        || 42,
        |_, observed_at_ms| {
            assert_eq!(observed_at_ms, 42);
            ServiceLogStreamState::Empty
        },
    );
    worker
        .request(query)
        .expect("replacement worker must accept the recovery request");
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    let recovered = loop {
        if let Some(snapshot) = worker.try_recv().expect("worker remains connected") {
            break snapshot;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "replacement worker recovery timed out"
        );
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(recovered.state, ServiceLogStreamState::Empty);
}

#[test]
fn empty_initial_query_and_caught_up_cursor_are_distinct() {
    assert_eq!(
        classify_stream_outcome(
            &query(None),
            ServiceLogCommandOutcome::Exited {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            100,
        ),
        ServiceLogStreamState::Empty
    );
    assert_eq!(
        classify_stream_outcome(
            &query(Some("cursor")),
            ServiceLogCommandOutcome::Exited {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            100,
        ),
        ServiceLogStreamState::Ended(ServiceLogStreamEnd::CaughtUp)
    );
}

#[test]
fn malformed_or_cursorless_output_is_failure_not_empty_or_eof() {
    for (after_cursor, stdout) in [
        (None, "{not-json}"),
        (Some("cursor"), r#"{"MESSAGE":"missing cursor"}"#),
    ] {
        assert!(matches!(
            classify_stream_outcome(
                &query(after_cursor),
                ServiceLogCommandOutcome::Exited {
                    success: true,
                    stdout: stdout.into(),
                    stderr: String::new(),
                },
                100,
            ),
            ServiceLogStreamState::Unavailable(ServiceLogFailure {
                kind: ServiceLogErrorKind::ProviderFailed,
                ..
            })
        ));
    }
}

#[test]
fn disconnected_result_channel_is_not_reported_as_no_result() {
    let worker = ServiceLogStreamWorker::disconnected();
    assert!(matches!(
        worker.try_recv(),
        Err(ServiceLogStreamEnd::Disconnected {
            detail: Some(detail)
        }) if detail.contains("disconnected")
    ));
}

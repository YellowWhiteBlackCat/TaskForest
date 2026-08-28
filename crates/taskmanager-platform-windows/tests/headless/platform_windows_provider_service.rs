use super::*;
use taskmanager_application::ServiceLogQuery;
#[cfg(windows)]
use taskmanager_core::ServiceLogState;

#[test]
fn non_windows_service_inventory_is_isolated_and_typed() {
    #[cfg(not(windows))]
    {
        let mut inventory = WinServiceInventoryProvider::new();
        assert_eq!(
            inventory.refresh(),
            Err(ProviderFailure::TemporarilyUnavailable)
        );
        let mut dependencies = WinServiceDependenciesProvider::new();
        assert_eq!(
            dependencies.dependencies(&ServiceId::new("x")),
            Err(ProviderFailure::MissingDependency)
        );
    }
}

#[test]
fn service_id_limit_is_part_of_the_native_boundary_contract() {
    #[cfg(windows)]
    {
        assert_eq!(MAX_SERVICE_ID_CHARS, 256);
        assert_eq!(
            valid_service_id(&ServiceId::new("")),
            Err(ProviderFailure::IdentityChanged)
        );
        assert_eq!(
            valid_service_id(&ServiceId::new("a\0b")),
            Err(ProviderFailure::IdentityChanged)
        );
        assert_eq!(
            valid_service_id(&ServiceId::new("a".repeat(MAX_SERVICE_ID_CHARS + 1))),
            Err(ProviderFailure::IdentityChanged)
        );
    }
}

#[test]
fn scm_inventory_never_calls_an_unreadable_or_truncated_empty_list_complete() {
    assert_eq!(scm_inventory_outcome(0, 0, false), SourceOutcome::Empty);
    assert_eq!(
        scm_inventory_outcome(0, 1, false),
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(
        scm_inventory_outcome(12, 0, true),
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(
        scm_inventory_outcome(12, 0, false),
        SourceOutcome::Available
    );
}

#[test]
fn event_log_lanes_never_fabricate_messages() {
    // Off-Windows the winevt source is a missing native dependency for the
    // real providers — never an empty success and never an Unsupported pose.
    let mut snapshot = WinServiceLogSnapshotProvider::new();
    let mut stream = WinServiceLogStreamProvider;
    let query = ServiceLogQuery {
        service_id: ServiceId::new("x"),
        level: taskmanager_core::ServiceLogLevelFilter::All,
        time: taskmanager_core::ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    #[cfg(not(windows))]
    {
        assert_eq!(
            snapshot.snapshot(&ServiceId::new("x")),
            Err(ProviderFailure::MissingDependency)
        );
        assert_eq!(
            stream.stream(&query, 1),
            Err(ProviderFailure::MissingDependency)
        );
    }
    #[cfg(windows)]
    {
        // A service with no provider trail on a live host is the contract's
        // honest Empty/Ready state, not a fabricated failure or success.
        match snapshot.snapshot(&ServiceId::new("x")) {
            Ok(ServiceLogState::Empty | ServiceLogState::Ready(_)) => {}
            other => panic!("snapshot must be an honest state, got {other:?}"),
        }
        assert!(stream.stream(&query, 1).is_ok());
    }
}

#[test]
fn stream_cursor_is_the_decimal_record_id_and_rejects_foreign_cursors() {
    assert_eq!(parse_stream_cursor(None), Ok(None));
    assert_eq!(parse_stream_cursor(Some("4242")), Ok(Some(4242)));
    // A stale or foreign cursor is an identity change, never a silent replay
    // from the beginning.
    assert_eq!(
        parse_stream_cursor(Some("journalctl-cursor")),
        Err(ProviderFailure::IdentityChanged)
    );
    assert_eq!(
        parse_stream_cursor(Some("")),
        Err(ProviderFailure::IdentityChanged)
    );
    assert_eq!(
        parse_stream_cursor(Some("-1")),
        Err(ProviderFailure::IdentityChanged)
    );
}

#[test]
fn windows_levels_map_onto_the_syslog_priority_scale_without_defaults() {
    assert_eq!(windows_level_priority(Some(1)), Some(2));
    assert_eq!(windows_level_priority(Some(2)), Some(2));
    assert_eq!(windows_level_priority(Some(3)), Some(4));
    assert_eq!(windows_level_priority(Some(4)), Some(6));
    assert_eq!(windows_level_priority(Some(5)), Some(7));
    // An omitted level stays unknown instead of defaulting to information.
    assert_eq!(windows_level_priority(None), None);
    assert_eq!(windows_level_priority(Some(99)), None);
    assert_eq!(
        priority_log_level(windows_level_priority(Some(4))),
        taskmanager_core::ServiceLogLevel::Info
    );
    assert_eq!(
        priority_log_level(windows_level_priority(None)),
        taskmanager_core::ServiceLogLevel::Unknown
    );
    // The level filters see the remapped priorities exactly like journalctl's.
    assert!(
        taskmanager_core::ServiceLogLevelFilter::Errors.matches(windows_level_priority(Some(2)))
    );
    assert!(
        !taskmanager_core::ServiceLogLevelFilter::Errors.matches(windows_level_priority(Some(4)))
    );
    assert!(
        taskmanager_core::ServiceLogLevelFilter::WarningsAndErrors
            .matches(windows_level_priority(Some(3)))
    );
}

fn sample_entry(message: &str, level: Option<u8>) -> taskmanager_windows_api::WindowsEventLogEntry {
    taskmanager_windows_api::WindowsEventLogEntry {
        record_id: 4242,
        timestamp_ms: Some(1_767_236_645_123),
        provider: Some("W32Time".to_string()),
        event_id: 7036,
        level,
        message: message.to_string(),
        properties: Vec::new(),
    }
}

#[test]
fn event_entries_use_record_id_cursors_and_honest_message_fallbacks() {
    // Formatted publisher message wins.
    let formatted = event_log_entries(vec![sample_entry("The service started.", Some(4))]);
    assert_eq!(formatted.len(), 1);
    assert_eq!(formatted[0].cursor, "4242");
    assert_eq!(formatted[0].message, "The service started.");
    assert_eq!(
        formatted[0].realtime_timestamp_micros,
        Some(1_767_236_645_123_000)
    );
    assert_eq!(formatted[0].level, taskmanager_core::ServiceLogLevel::Info);

    // Without a formatted message the rendered event data is shown verbatim;
    // with neither, an identification line — never invented content.
    let mut data_only = sample_entry("", Some(3));
    data_only.properties = vec![
        ("param1".to_string(), "W32Time".to_string()),
        ("param2".to_string(), "running".to_string()),
    ];
    assert_eq!(
        event_log_entries(vec![data_only])[0].message,
        "W32Time running"
    );
    assert_eq!(
        event_log_entries(vec![sample_entry("", None)])[0].message,
        "event 7036 from W32Time"
    );
}

#[test]
fn snapshot_lines_carry_timestamp_level_and_message() {
    let lines = event_log_lines(&[sample_entry("running", Some(4))]);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], "2026-01-01T03:04:05.123Z [info] running");
    // Absent timestamps render a marker, not a fabricated epoch zero.
    let mut no_time = sample_entry("no time", None);
    no_time.timestamp_ms = None;
    assert_eq!(event_log_lines(&[no_time])[0], "- [unknown] no time");
    // The formatter is the pure inverse of the boundary timestamp parser.
    assert_eq!(format_event_log_timestamp(0), "1970-01-01T00:00:00.000Z");
}

#[test]
fn non_windows_service_control_is_a_missing_platform_dependency() {
    #[cfg(not(windows))]
    {
        let mut control = WinServiceControlProvider::new();
        assert_eq!(
            control.control(&ServiceId::new("Spooler"), ServiceAction::Restart),
            Err(ProviderFailure::MissingDependency)
        );
    }
}

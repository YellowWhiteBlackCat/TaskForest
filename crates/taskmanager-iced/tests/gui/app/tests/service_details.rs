use super::*;

#[test]
fn service_details_entry_projects_lifecycle_and_dependency_facts() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let _ = app.update(Message::OpenServiceDetailsFor { index: 0 });

    assert!(app.modal_open());
    let service_id = app
        .service_details_target()
        .expect("details target should be frozen")
        .clone();
    assert!(!service_id.as_str().is_empty());
    let snapshot = app.service_details_snapshot();
    assert!(!snapshot.dependencies.is_loading());
    let dependencies = snapshot
        .dependencies
        .projected()
        .expect("demo dependencies are ready");
    assert_eq!(
        dependencies
            .relation_projection(&taskmanager_core::core::services::ServiceRelationKind::Requires),
        "sysinit.target basic.target"
    );
    assert_eq!(
        dependencies
            .relation_projection(&taskmanager_core::core::services::ServiceRelationKind::WantedBy),
        "multi-user.target"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::ServiceDetailsOpen { index: 0 }),
        "iced-service-details-open-0"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::ServiceDetailsRetry),
        "iced-service-details-retry"
    );

    // The real view path owns the modal projection; building it must not panic
    // or fall back to the service-log overlay.
    let _ = crate::ui::view(&app);
    let _ = app.update(Message::RefreshServiceDetails);
    assert_eq!(app.service_details_snapshot().dependencies.failure(), None);

    let _ = app.update(Message::DismissOverlay);
    assert!(!app.modal_open());
    assert!(app.service_details_target().is_none());
}

/// The details modal's MERGED log panel folds a stream for its own service
/// only: a `LogStream` update for the active service resolves to Ready lines,
/// a stream for a different service never leaks into the panel, and the
/// follow pump stays idle while a request is in flight or the panel is
/// paused. Demo mode seeds the panel on open (no host I/O).
#[test]
fn service_details_merged_log_panel_folds_its_own_stream() {
    use taskmanager_application::ServiceUpdate;
    use taskmanager_application::i18n::set_language;
    use taskmanager_core::core::services::{
        ServiceLogEntries, ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter,
        ServiceLogQuery, ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
    };
    use taskmanager_core::core::target::ServiceId;

    set_language(taskmanager_application::i18n::Language::En);

    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let _ = app.update(Message::OpenServiceDetailsFor { index: 0 });
    let service_id = app.service_details_target().expect("details open").clone();

    // Demo seeding: the merged panel resolves Ready with the three seeded
    // entries (through the production apply path, filters at defaults).
    let seeded = app.service_details_snapshot();
    let ServiceLogStateVariant::Ready(lines) = state_variant(&seeded.logs) else {
        panic!("demo logs must resolve Ready, got {:?}", seeded.logs);
    };
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("Started telemetry service"));
    assert!(!seeded.log_paused);

    // Controls mutate only the merged feed: pause, then two level hops and
    // one time hop land on the documented shared orders.
    let _ = app.update(Message::ToggleServiceDetailsLogPaused);
    let _ = app.update(Message::CycleServiceDetailsLogLevel);
    let _ = app.update(Message::CycleServiceDetailsLogLevel);
    let _ = app.update(Message::CycleServiceDetailsLogTime);
    let controls = app.service_details_snapshot();
    assert!(controls.log_paused);
    assert_eq!(controls.log_level, ServiceLogLevelFilter::WarningsAndErrors);
    assert_eq!(controls.log_time, ServiceLogTimeFilter::LastHour);
    // A paused panel never issues a follow query.
    let mut paused_state = app.service_details.clone();
    assert!(
        paused_state.poll_log(10_000).is_none(),
        "paused panel must not poll"
    );
    // Cycle every control back to its default so the isolation check below
    // reads an unfiltered panel.
    let _ = app.update(Message::CycleServiceDetailsLogLevel);
    let _ = app.update(Message::CycleServiceDetailsLogLevel);
    let _ = app.update(Message::CycleServiceDetailsLogTime);
    let _ = app.update(Message::CycleServiceDetailsLogTime);
    let _ = app.update(Message::ToggleServiceDetailsLogPaused);

    // A stream for ANOTHER service never leaks into the open panel.
    let other = ServiceId::new("systemd:other.service");
    app.apply_service_details_updates([ServiceUpdate::LogStream {
        request_id: taskmanager_platform_contract::RequestId::new(2).expect("fixture id"),
        observed_at_ms: 2,
        snapshot: ServiceLogStreamSnapshot {
            query: ServiceLogQuery {
                service_id: other,
                level: ServiceLogLevelFilter::All,
                time: ServiceLogTimeFilter::All,
                after_cursor: None,
            },
            state: ServiceLogStreamState::Ready(
                ServiceLogEntries::new(vec![ServiceLogEntry {
                    cursor: "other-1".into(),
                    realtime_timestamp_micros: None,
                    priority: Some(3),
                    level: ServiceLogLevel::Error,
                    message: "must not leak".into(),
                }])
                .expect("non-empty"),
            ),
        },
    }]);
    let after = app.service_details_snapshot();
    let ServiceLogStateVariant::Ready(lines) = state_variant(&after.logs) else {
        panic!("panel must keep its own lines, got {:?}", after.logs);
    };
    assert!(
        !lines.iter().any(|line| line.contains("must not leak")),
        "a foreign stream must never fold into the open panel"
    );

    // The follow pump on the resumed panel: the next query carries the
    // active service; interval + inflight gate the immediate re-poll; a
    // landed answer clears inflight (demo seeds advance no cursor, so the
    // follow query carries no cursor).
    let mut state = app.service_details.clone();
    let first = state.poll_log(10_000).expect("resumed panel polls");
    assert_eq!(first.0, service_id);
    let request_id = taskmanager_platform_contract::RequestId::new(3).expect("fixture id");
    let attempt_id = state
        .begin_stream_attempt(first.1.clone())
        .expect("targeted attempt starts");
    state.accept_stream(attempt_id, request_id);
    assert!(state.poll_log(10_500).is_none(), "interval gate + inflight");
    state.apply(ServiceUpdate::LogStream {
        request_id,
        observed_at_ms: 10_500,
        snapshot: ServiceLogStreamSnapshot {
            query: ServiceLogQuery {
                service_id: first.0.clone(),
                level: first.1.level,
                time: first.1.time,
                after_cursor: first.1.after_cursor.clone(),
            },
            state: ServiceLogStreamState::Empty,
        },
    });
    let second = state
        .poll_log(20_000)
        .expect("a landed answer clears inflight and the interval passed");
    // The follow query carries the feed's newest cursor (the seeded demo-3
    // entry) so a follow request never re-reads retained rows.
    assert_eq!(second.1.after_cursor.as_deref(), Some("demo-3"));

    // The full modal composes with the merged log panel on screen.
    let _ = crate::ui::view(&app);

    // Helper: a tiny local mirror of the state enum for matching without
    // importing the private inner types.
    fn state_variant<'a>(
        state: &'a taskmanager_core::core::services::ServiceLogState,
    ) -> ServiceLogStateVariant<'a> {
        match state {
            taskmanager_core::core::services::ServiceLogState::Ready(lines) => {
                ServiceLogStateVariant::Ready(lines.as_slice())
            }
            _ => ServiceLogStateVariant::Other,
        }
    }
    enum ServiceLogStateVariant<'a> {
        Ready(&'a [String]),
        Other,
    }
}

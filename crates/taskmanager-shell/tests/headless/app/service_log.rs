//! Service-log open/poll/follow/close state-machine coverage, split out of the
//! main tests module so the file stays under the 800-line source ceiling.
use super::super::*;
use taskmanager_application::{
    CapabilityId, CorrelatedEvent, EventSequence, PlatformEventBatch, PlatformEventContext,
    RequestId, ServiceEvent, ServiceLogEntry, ServiceLogLevelFilter, ServiceLogQuery,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter, ServiceUpdate,
};

#[test]
fn service_log_open_poll_follow_and_close_follow_the_shared_state_machine() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Services;

    // Open the selected service: an initial follow query is emitted.
    let effect = app
        .open_service_log()
        .expect("selected service should open a log stream");
    let PlatformEffect::ServiceLogStream(request) = effect else {
        panic!("log open must cross the typed effect boundary");
    };
    assert_eq!(
        request.query.service_id.as_str(),
        "fixture.service:NetworkManager.service"
    );
    assert_eq!(request.query.after_cursor, None);
    assert_eq!(request.query.level, ServiceLogLevelFilter::All);

    // The first follow is throttled (the open already submitted one); a poll
    // a second later emits a follow query with the merged cursor.
    assert!(
        app.poll_service_log(0).is_none(),
        "open counts as a poll tick"
    );
    assert!(app.poll_service_log(500).is_none(), "under 1s is throttled");
    let follow = app
        .poll_service_log(2_000)
        .expect("a second after open the follow is due");
    let PlatformEffect::ServiceLogStream(follow) = follow else {
        panic!("follow must be a stream request");
    };
    assert_eq!(follow.query.after_cursor, None, "empty feed has no cursor");

    // A Ready stream batch merges into the feed and advances the cursor.
    let entry = ServiceLogEntry {
        cursor: "j:1".into(),
        realtime_timestamp_micros: Some(1_700_000_000_000_000),
        priority: Some(3),
        level: taskmanager_application::ServiceLogLevel::Error,
        message: "bind failed".into(),
    };
    let batch = ServiceLogStreamSnapshot {
        query: ServiceLogQuery {
            service_id: "fixture.service:NetworkManager.service".into(),
            level: ServiceLogLevelFilter::All,
            time: ServiceLogTimeFilter::All,
            after_cursor: None,
        },
        state: ServiceLogStreamState::from_query_entries(
            &ServiceLogQuery {
                service_id: "fixture.service:NetworkManager.service".into(),
                level: ServiceLogLevelFilter::All,
                time: ServiceLogTimeFilter::All,
                after_cursor: None,
            },
            vec![entry],
        ),
    };
    let batch_request = RequestId::new(4).expect("fixture id");
    app.service_log
        .as_mut()
        .expect("service log stays open")
        .lifecycle
        .begin(batch_request, batch.query.clone());
    let mut batch_event = PlatformEventBatch::default();
    batch_event.service_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: batch_request,
            capability: CapabilityId::SERVICES,
            provider: None,
            sequence: EventSequence::new(5),
            observed_at_ms: 50,
        },
        ServiceEvent::Update(ServiceUpdate::LogStream {
            request_id: batch_request,
            observed_at_ms: 50,
            snapshot: batch,
        }),
    ));
    app.apply_platform_batch(batch_event);

    let entries = app
        .visible_service_log_entries(1_700_000_000_000_000)
        .expect("open feed exposes entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "bind failed");

    // The next follow carries the merged cursor; pausing stops follow queries.
    let follow = app
        .poll_service_log(4_000)
        .expect("follow due after another second");
    let PlatformEffect::ServiceLogStream(follow) = follow else {
        panic!("follow must be a stream request");
    };
    assert_eq!(follow.query.after_cursor.as_deref(), Some("j:1"));
    app.toggle_service_log_paused();
    assert!(
        app.poll_service_log(6_000).is_none(),
        "paused must not poll"
    );
    app.toggle_service_log_paused();
    app.toggle_service_log_follow();
    assert!(
        app.poll_service_log(8_000).is_none(),
        "follow-off must not poll"
    );

    // Filter cycles are renderer-neutral and single-source.
    app.toggle_service_log_follow();
    assert_eq!(
        app.service_log.as_ref().unwrap().feed.level,
        ServiceLogLevelFilter::All
    );
    app.cycle_service_log_level();
    assert_eq!(
        app.service_log.as_ref().unwrap().feed.level,
        ServiceLogLevelFilter::Errors
    );
    app.cycle_service_log_time();
    assert_eq!(
        app.service_log.as_ref().unwrap().feed.time,
        ServiceLogTimeFilter::LastHour
    );

    app.close_service_log();
    assert!(app.service_log.is_none());
}

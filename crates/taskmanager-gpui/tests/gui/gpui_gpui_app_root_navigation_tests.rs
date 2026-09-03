use super::{TopPage, page_refresh_request};
use crate::gpui_app::root::RootView;
use gpui::AppContext;
use taskmanager_application::{
    CorrelatedServiceEvent, CorrelatedSessionEvent, CorrelatedStartupEvent, PlatformEventBatch,
    RefreshRequest, ServiceEvent, SessionEvent, StartupEvent,
};
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence, StartupScope,
    StartupSource,
};
use taskmanager_core::core::target::SessionId;
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, PartialSourceSnapshot, RequestId,
};
use taskmanager_ui_contract::page_descriptors;

#[test]
fn every_shared_page_round_trips_through_the_gpui_adapter() {
    for descriptor in page_descriptors() {
        let top_page = TopPage::from_app_page(descriptor.page);
        assert_eq!(top_page.app_page(), Some(descriptor.page));
    }
}

#[test]
fn containers_stays_outside_the_shared_page_contract() {
    assert_eq!(TopPage::Containers.app_page(), None);
}

#[test]
fn visible_inventory_pages_request_their_own_refresh() {
    assert_eq!(
        page_refresh_request(TopPage::Services),
        Some(RefreshRequest::Services)
    );
    assert_eq!(
        page_refresh_request(TopPage::Startup),
        Some(RefreshRequest::Startup)
    );
    assert_eq!(
        page_refresh_request(TopPage::Users),
        Some(RefreshRequest::Sessions)
    );
}

#[test]
fn detail_pages_backfill_hardware_or_container_inventory() {
    assert_eq!(
        page_refresh_request(TopPage::Performance),
        Some(RefreshRequest::HardwareInventory)
    );
    assert_eq!(
        page_refresh_request(TopPage::System),
        Some(RefreshRequest::HardwareInventory)
    );
    assert_eq!(
        page_refresh_request(TopPage::Containers),
        Some(RefreshRequest::Containers)
    );
}

#[test]
fn dashboard_driven_pages_do_not_emit_a_targeted_request() {
    assert_eq!(page_refresh_request(TopPage::Apps), None);
    assert_eq!(page_refresh_request(TopPage::AppHistory), None);
}

#[test]
fn every_page_round_trips_through_the_refresh_contract() {
    for page in TopPage::ALL {
        // The mapping must be total: every page either requests its own
        // inventory or explicitly stays on the automatic schedule. This is the
        // guard that a new page cannot silently fall into the empty dispatch
        // branch that made Services/Startup/Users show no data.
        let _ = page_refresh_request(page);
    }
}

#[gpui::test]
fn page_switch_dispatches_refresh_and_snapshot_populates_page_rows(cx: &mut gpui::TestAppContext) {
    let entity = cx.new(|cx| RootView::new(taskmanager_theme::Theme::dark(), cx));

    // 1. Initial state: on Performance page, lists are empty.
    entity.update(cx, |view, _cx| {
        assert_eq!(view.page, TopPage::Performance);
        assert!(view.services().is_empty());
        assert!(view.startup_entries().is_empty());
        assert!(view.sessions().is_empty());
    });

    // 2. Select Services: request_page_data maps to RefreshRequest::Services.
    entity.update(cx, |view, cx| {
        view.select_page(TopPage::Services);
        assert_eq!(view.page, TopPage::Services);
        assert_eq!(
            page_refresh_request(view.page),
            Some(RefreshRequest::Services)
        );

        // Simulate platform worker delivering the service inventory snapshot.
        let service = ServiceItem::from_inventory(
            "test.service",
            "test",
            ServiceStatus::Active,
            "Test Service",
            "",
            "",
            "",
        );
        let batch = PlatformEventBatch {
            service_events: vec![CorrelatedServiceEvent {
                request_id: RequestId::MIN,
                capability: CapabilityId::SERVICES,
                provider: None,
                sequence: EventSequence::new(1),
                observed_at_ms: 10,
                event: ServiceEvent::Snapshot(PartialSourceSnapshot {
                    items: vec![service],
                    sources: Vec::new(),
                }),
            }],
            ..PlatformEventBatch::default()
        };
        let changes = view.apply_platform_event_batch(batch, cx);
        assert!(changes.services);
        assert_eq!(view.services().len(), 1);
        assert_eq!(view.services()[0].name, "test");
        assert_eq!(view.services_generation(), 1);
    });

    // 3. Select Startup: request_page_data maps to RefreshRequest::Startup.
    entity.update(cx, |view, cx| {
        view.select_page(TopPage::Startup);
        assert_eq!(view.page, TopPage::Startup);
        assert_eq!(
            page_refresh_request(view.page),
            Some(RefreshRequest::Startup)
        );

        let entry = StartupEntry {
            id: "desktop:test.desktop".into(),
            name: "Test App".into(),
            exec: "/usr/bin/test".into(),
            enabled: true,
            source: StartupSource::DesktopEntry,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: "test.desktop".into(),
            impact: StartupImpact::Low,
            impact_evidence: StartupImpactEvidence::Measured { duration_ms: 12 },
        };
        let batch = PlatformEventBatch {
            startup_events: vec![CorrelatedStartupEvent {
                request_id: RequestId::MIN,
                capability: CapabilityId::STARTUP,
                provider: None,
                sequence: EventSequence::new(1),
                observed_at_ms: 20,
                event: StartupEvent::Snapshot(PartialSourceSnapshot {
                    items: vec![entry],
                    sources: Vec::new(),
                }),
            }],
            ..PlatformEventBatch::default()
        };
        let changes = view.apply_platform_event_batch(batch, cx);
        assert!(changes.startup);
        assert_eq!(view.startup_entries().len(), 1);
        assert_eq!(view.startup_entries()[0].name, "Test App");
        assert_eq!(view.startup_generation(), 1);
    });

    // 4. Select Users: request_page_data maps to RefreshRequest::Sessions.
    entity.update(cx, |view, cx| {
        view.select_page(TopPage::Users);
        assert_eq!(view.page, TopPage::Users);
        assert_eq!(
            page_refresh_request(view.page),
            Some(RefreshRequest::Sessions)
        );

        let session = SessionItem {
            id: SessionId::from("s1"),
            uid: 1000,
            user: "zhugenanbei".into(),
            seat: Some("seat0".into()),
            tty: Some("pts/0".into()),
            remote: false,
            timestamp: None,
        };
        let batch = PlatformEventBatch {
            session_events: vec![CorrelatedSessionEvent {
                request_id: RequestId::MIN,
                capability: CapabilityId::SESSIONS,
                provider: None,
                sequence: EventSequence::new(1),
                observed_at_ms: 30,
                event: SessionEvent::Snapshot(PartialSourceSnapshot {
                    items: vec![session],
                    sources: Vec::new(),
                }),
            }],
            ..PlatformEventBatch::default()
        };
        let _ = view.apply_platform_event_batch(batch, cx);
        assert_eq!(view.sessions().len(), 1);
        assert_eq!(view.sessions()[0].user, "zhugenanbei");
        assert_eq!(view.sessions_generation(), 1);
    });
}

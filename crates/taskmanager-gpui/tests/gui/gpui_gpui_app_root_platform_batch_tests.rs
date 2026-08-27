use super::*;
use gpui::AppContext;
use taskmanager_application::{
    CapabilityId, EventSequence, FailureKind, OperationFailure, PlatformEventBatch, ProviderId,
    RequestIdGenerator, RetryDisposition, SourceOutcome,
};

#[gpui::test]
fn inventory_operation_failure_reaches_the_page_source_projection(cx: &mut gpui::TestAppContext) {
    let entity = cx.new(|cx| RootView::new(crate::gpui_app::theme::Theme::dark(), cx));
    let mut request_ids = RequestIdGenerator::default();
    let failure = OperationFailure {
        request_id: request_ids.next_id(),
        capability: CapabilityId::SERVICES,
        sequence: EventSequence::new(2),
        kind: FailureKind::TimedOut,
        retry: RetryDisposition::RetryLater,
        provider: Some(ProviderId::borrowed("test.services")),
        observed_at_ms: 10,
    };
    let hardware_failure = OperationFailure {
        request_id: request_ids.next_id(),
        capability: CapabilityId::HARDWARE_INVENTORY,
        sequence: EventSequence::new(3),
        kind: FailureKind::MissingDependency,
        retry: RetryDisposition::AfterCapabilityChange,
        provider: Some(ProviderId::borrowed("test.hardware")),
        observed_at_ms: 11,
    };
    entity.update(cx, |view, cx| {
        let changes = view.apply_platform_event_batch(
            PlatformEventBatch {
                failures: vec![failure],
                ..PlatformEventBatch::default()
            },
            cx,
        );
        assert!(changes.services);
        assert_eq!(view.services_generation(), 1);
        assert_eq!(
            view.services_generation(),
            view.projection().services_revision,
            "rows and source status share the shell domain revision",
        );
        assert_eq!(view.service_sources().len(), 1);
        assert_eq!(
            view.service_sources()[0].outcome,
            SourceOutcome::Unavailable(FailureKind::TimedOut)
        );
        assert_eq!(view.service_sources()[0].item_count, 0);

        let services = view.services_rc().clone();
        let sources = view.service_sources_rc().clone();
        let generation = view.services_generation();
        let unchanged = view.apply_platform_event_batch(PlatformEventBatch::default(), cx);
        assert!(!unchanged.services);
        assert_eq!(view.services_generation(), generation);
        assert!(std::rc::Rc::ptr_eq(&services, view.services_rc()));
        assert!(std::rc::Rc::ptr_eq(&sources, view.service_sources_rc()));

        let _ = view.apply_platform_event_batch(
            PlatformEventBatch {
                failures: vec![hardware_failure],
                ..PlatformEventBatch::default()
            },
            cx,
        );
        assert_eq!(view.hardware_generation(), 1);
        assert_eq!(
            view.hardware_generation(),
            view.projection().system_revision
        );
        assert_eq!(
            view.hardware_sources()[0].outcome,
            SourceOutcome::Unavailable(FailureKind::MissingDependency),
        );
    });
}

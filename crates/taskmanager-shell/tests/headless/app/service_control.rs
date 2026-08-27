//! Service-control confirmation round-trip coverage, split out of the main
//! tests module so the file stays under the source-line ceiling.
use super::super::*;
use std::sync::{Arc, Mutex};
use taskmanager_application::{
    CapabilityCatalog, CapabilityId, CapabilitySnapshot, ConfirmationKind, ControlRequestId,
    CorrelatedEvent, EventEnvelope, EventPort, EventPortError, EventSequence, FailureKind,
    PlatformClient, PlatformEvent, PlatformEventContext, PlatformFacets, PlatformHandle,
    RequestEnvelope, RequestId, RequestPort, ServiceControlOutcome, ServiceControlRequest,
    ServiceEvent, ServiceFacets, ServiceId, ServiceItem, ServiceStatus, ServiceUpdate,
    SubmissionError, SurfaceKind,
};

#[test]
fn service_control_confirmation_records_pending_target_until_dismissal_or_page_change() {
    let mut app = crate::demo_app();
    let service = app.data.services.as_ref().expect("demo services")[0].clone();
    assert!(app.select_service_control(&service, ServiceAction::Stop));
    let expected = ServiceControlTarget {
        service_id: service.id.clone(),
        action: ServiceAction::Stop,
    };

    assert_eq!(app.apply_action(AppAction::RequestServiceControl), None);
    assert_eq!(app.pending_service_control(), Some(&expected));
    assert_eq!(
        app.interaction_surface(),
        Some(SurfaceKind::Confirmation(ConfirmationKind::ServiceControl))
    );

    // Dismissal (Cancel / Escape / scrim) clears the recorded confirmation.
    let _ = app.apply_action(AppAction::DismissOverlay);
    assert_eq!(app.pending_service_control(), None);
    assert_eq!(app.interaction_surface(), None);

    // A page change also releases a still-pending confirmation.
    assert!(app.select_service_control(&service, ServiceAction::Stop));
    let _ = app.apply_action(AppAction::RequestServiceControl);
    assert_eq!(app.pending_service_control(), Some(&expected));
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert_eq!(app.pending_service_control(), None);
    assert_eq!(app.interaction_surface(), None);
}

#[test]
fn service_control_round_trip_flows_request_confirm_and_clears_pending() {
    let mut app = crate::demo_app();
    let service = app.data.services.as_ref().expect("demo services")[1].clone();
    assert!(app.select_service_control(&service, ServiceAction::Restart));
    assert_eq!(app.apply_action(AppAction::RequestServiceControl), None);
    assert!(app.pending_service_control().is_some());

    let confirmed = app.apply_action(AppAction::ConfirmServiceControl);
    assert!(matches!(
        confirmed,
        Some(PlatformEffect::ServiceControl(ref target))
            if target.service_id == service.id && target.action == ServiceAction::Restart
    ));
    assert_eq!(app.pending_service_control(), None);
    assert_eq!(app.interaction_surface(), None);
}

#[test]
fn select_service_control_rejects_read_only_rows_without_provider_target() {
    let mut app = crate::demo_app();
    let read_only =
        ServiceItem::from_inventory("", "legacy-service", ServiceStatus::Unknown, "", "", "", "");

    assert!(!app.select_service_control(&read_only, ServiceAction::Stop));
    assert_eq!(app.application.selected_service_control, None);
}

#[derive(Default)]
struct EmptyCapabilities;

impl CapabilityCatalog for EmptyCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

#[derive(Default)]
struct EmptyEvents;

impl EventPort for EmptyEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

#[derive(Default)]
struct RecordingServiceControl(Mutex<Vec<ServiceControlRequest>>);

impl RequestPort for RecordingServiceControl {
    type Request = ServiceControlRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .expect("recorded service-control requests")
            .push(request.payload);
        Ok(())
    }
}

fn service_control_client(port: Arc<RecordingServiceControl>) -> PlatformClient {
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default().with_service(ServiceFacets::default().with_control(port)),
    ))
}

fn control_target(name: &str, action: ServiceAction) -> ServiceControlTarget {
    ServiceControlTarget {
        service_id: ServiceId::new(name),
        action,
    }
}

#[test]
fn service_control_effect_submits_typed_request_with_generated_correlation_id() {
    let recorded = Arc::new(RecordingServiceControl::default());
    let mut app = crate::demo_app();
    let target = control_target(
        "fixture.service:NetworkManager.service",
        ServiceAction::Stop,
    );
    let mut client = service_control_client(recorded.clone());

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::ServiceControl(target.clone()),
    );

    let request_id = {
        let requests = recorded.0.lock().expect("recorded requests");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].service_id, target.service_id);
        assert_eq!(requests[0].action, target.action);
        assert_ne!(requests[0].request_id.get(), 0);
        requests[0].request_id
    };
    assert!(app.feedback_text().contains("queued"));
    assert_eq!(
        app.data.service_control_requests.pending(),
        Some((request_id, &target.service_id, target.action))
    );
}

#[test]
fn service_control_outcome_accepts_only_the_latest_matching_intent() {
    let recorded = Arc::new(RecordingServiceControl::default());
    let mut app = crate::demo_app();
    let mut client = service_control_client(recorded.clone());

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::ServiceControl(control_target(
            "fixture.service:NetworkManager.service",
            ServiceAction::Stop,
        )),
    );
    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::ServiceControl(control_target(
            "fixture.service:docker.service",
            ServiceAction::Restart,
        )),
    );
    let ids: Vec<_> = recorded
        .0
        .lock()
        .expect("recorded requests")
        .iter()
        .map(|request| request.request_id)
        .collect();
    let (superseded, latest) = (ids[0], ids[1]);

    // A superseded completion (old request id, matching target) cannot replace
    // feedback for the latest intent.
    app.set_feedback_activity("baseline");
    app.clear_feedback_notice();
    app.apply_platform_batch(service_control_batch(
        superseded,
        "fixture.service:NetworkManager.service",
        ServiceAction::Stop,
        Ok(()),
    ));
    assert_eq!(app.feedback_text(), "baseline");

    // The latest completion lands and clears the pending tracker.
    app.apply_platform_batch(service_control_batch(
        latest,
        "fixture.service:docker.service",
        ServiceAction::Restart,
        Ok(()),
    ));
    assert!(app.feedback_text().contains("completed"));
    assert!(app.feedback_text().contains("Restart"));
    assert_eq!(app.data.service_control_requests.pending(), None);

    // A repeated stale completion after acceptance is still ignored.
    app.set_feedback_activity("baseline");
    app.clear_feedback_notice();
    app.apply_platform_batch(service_control_batch(
        latest,
        "fixture.service:docker.service",
        ServiceAction::Restart,
        Ok(()),
    ));
    assert_eq!(app.feedback_text(), "baseline");
}

fn service_control_batch(
    request_id: ControlRequestId,
    service: &str,
    action: ServiceAction,
    result: Result<(), FailureKind>,
) -> PlatformEventBatch {
    let mut batch = PlatformEventBatch::default();
    batch.service_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(2).expect("fixture request ID"),
            capability: CapabilityId::SERVICE_CONTROL,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 100,
        },
        ServiceEvent::Update(ServiceUpdate::Action(ServiceControlOutcome {
            request_id,
            service_id: ServiceId::new(service),
            action,
            result,
        })),
    ));
    batch
}

#[test]
fn service_control_effect_reports_submission_failure_honestly() {
    let mut app = crate::demo_app();
    let mut client = PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    ));

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::ServiceControl(control_target(
            "fixture.service:NetworkManager.service",
            ServiceAction::Stop,
        )),
    );

    assert!(!app.feedback_text().contains("queued"));
    assert!(app.feedback_text().contains("services.control"));
    assert_eq!(app.data.service_control_requests.pending(), None);
}

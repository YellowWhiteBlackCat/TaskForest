//! Independent inventory-source failure projections.

use super::super::*;
use taskmanager_application::{CorrelatedEvent, PlatformEventContext, ServiceEvent};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, OperationFailure, PartialSourceSnapshot, ProviderFailure,
    RequestId,
};

#[test]
fn inventory_failures_project_independently_without_overwriting_sibling_pages() {
    let mut batch = PlatformEventBatch::default();
    for (request_id, capability) in [
        (10, CapabilityId::HARDWARE_INVENTORY),
        (11, CapabilityId::SERVICES),
        (12, CapabilityId::STARTUP),
        (13, CapabilityId::SESSIONS),
    ] {
        batch.failures.push(OperationFailure {
            request_id: RequestId::new(request_id).expect("fixture request id"),
            capability,
            sequence: EventSequence::new(request_id),
            kind: FailureKind::TimedOut,
            retry: ProviderFailure::from_kind(FailureKind::TimedOut).retry(),
            provider: Some(ProviderId::borrowed("fixture.inventory")),
            observed_at_ms: request_id,
        });
    }

    let mut app = crate::demo_app();
    let initial_revisions = (
        app.data.system_revision,
        app.data.services_revision,
        app.data.startup_revision,
        app.data.sessions_revision,
    );
    app.apply_platform_batch(batch);

    assert_eq!(app.data.system_revision, initial_revisions.0 + 1);
    assert_eq!(app.data.services_revision, initial_revisions.1 + 1);
    assert_eq!(app.data.startup_revision, initial_revisions.2 + 1);
    assert_eq!(app.data.sessions_revision, initial_revisions.3 + 1);

    for (source, expected_count) in [
        (
            app.data.hardware_source.as_deref(),
            usize::from(app.data.hardware.is_some()),
        ),
        (app.data.services_source.as_deref(), 5),
        (app.data.startup_source.as_deref(), 2),
        (app.data.sessions_source.as_deref(), 2),
    ] {
        let source = source.expect("each inventory failure gets its own source slot");
        assert_eq!(source.len(), 1);
        assert_eq!(source[0].item_count, expected_count);
        assert_eq!(
            source[0].outcome,
            SourceOutcome::Unavailable(FailureKind::TimedOut)
        );
    }
    assert_eq!(app.data.services.as_ref().map(Vec::len), Some(5));
    assert_eq!(app.data.startup_entries.as_ref().map(Vec::len), Some(2));
    assert_eq!(app.data.sessions.as_ref().map(Vec::len), Some(2));
}

#[test]
fn successful_snapshot_in_one_batch_overrides_its_seeded_source_failure() {
    let request_id = RequestId::new(21).expect("fixture request id");
    let provider = ProviderId::borrowed("fixture.services");
    let mut app = crate::demo_app();
    let services = app.data.services.clone().expect("demo services");
    let initial_revision = app.data.services_revision;
    let initial_refresh_count = app.data.refresh_count;
    let mut batch = PlatformEventBatch::default();
    batch.failures.push(OperationFailure {
        request_id,
        capability: CapabilityId::SERVICES,
        sequence: EventSequence::new(20),
        kind: FailureKind::TimedOut,
        retry: ProviderFailure::from_kind(FailureKind::TimedOut).retry(),
        provider: Some(provider.clone()),
        observed_at_ms: 20,
    });
    batch.service_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id,
            capability: CapabilityId::SERVICES,
            provider: Some(provider.clone()),
            sequence: EventSequence::new(21),
            observed_at_ms: 21,
        },
        ServiceEvent::Snapshot(PartialSourceSnapshot::new(
            services,
            vec![SourceStatus {
                provider,
                outcome: SourceOutcome::Available,
                item_count: 5,
            }],
        )),
    ));

    app.apply_platform_batch(batch);

    let source = app
        .data
        .services_source
        .as_deref()
        .expect("successful snapshot replaces seeded failure");
    assert_eq!(source[0].outcome, SourceOutcome::Available);
    assert_eq!(app.data.services_revision, initial_revision + 1);
    assert_eq!(app.data.refresh_count, initial_refresh_count + 1);

    app.apply_platform_batch(PlatformEventBatch::default());
    assert_eq!(app.data.services_revision, initial_revision + 1);
    assert_eq!(app.data.refresh_count, initial_refresh_count + 1);
}

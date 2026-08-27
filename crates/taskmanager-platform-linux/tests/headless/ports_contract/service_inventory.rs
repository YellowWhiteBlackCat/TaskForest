use super::*;

#[test]
fn selected_service_source_status_survives_the_correlated_event() {
    let handle = spawn_complete(fake_registry(FakeProvider::default()));
    let mut ids = RequestIdGenerator::default();
    let request_id = ids.next_id();
    handle
        .service_inventory()
        .expect("service inventory facet")
        .try_submit(RequestEnvelope {
            id: request_id,
            capability: CapabilityId::SERVICES,
            submitted_at_ms: 11,
            payload: ServiceInventoryRequest::Refresh,
        })
        .expect("service inventory accepted");

    let event = wait_event(&handle);
    assert_eq!(event.request_id, request_id);
    assert_eq!(event.capability, CapabilityId::SERVICES);
    let Ok(PlatformEvent::Services(ServiceEvent::Snapshot(snapshot))) = event.outcome else {
        panic!("service snapshot event expected");
    };
    assert_eq!(snapshot.sources.len(), 1);
    assert_eq!(
        snapshot.sources[0].provider,
        ProviderId::borrowed("fixture.service.selected")
    );
    assert_eq!(snapshot.sources[0].outcome, SourceOutcome::Available);
}

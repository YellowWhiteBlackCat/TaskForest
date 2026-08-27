use taskmanager_application::{
    CapabilityId, CorrelatedEvent, DirectoryScanId, DirectoryScanStatus, DirectoryScanTotals,
    DirectoryUsageEvent, DirectoryUsageSnapshot, EventSequence, PartialSourceSnapshot,
    PlatformEventBatch, PlatformEventContext, RequestId, ServiceEvent,
};

use super::*;

fn context(sequence: u64, capability: CapabilityId) -> PlatformEventContext {
    PlatformEventContext {
        request_id: RequestId::new(sequence).expect("non-zero sequence fixture"),
        capability,
        provider: None,
        sequence: EventSequence::new(sequence),
        observed_at_ms: sequence,
    }
}

fn independent_batch() -> PlatformEventBatch {
    let mut batch = PlatformEventBatch::default();
    batch.service_events.push(CorrelatedEvent::new(
        context(3, CapabilityId::SERVICES),
        ServiceEvent::Snapshot(PartialSourceSnapshot::new(Vec::new(), Vec::new())),
    ));
    batch.directory_usage_events.push(CorrelatedEvent::new(
        context(7, CapabilityId::DIRECTORY_USAGE),
        DirectoryUsageEvent::Update(DirectoryUsageSnapshot {
            scan_id: DirectoryScanId::new(7),
            root: "/fixture".to_owned(),
            status: DirectoryScanStatus::Scanning,
            entries: Vec::new(),
            totals: DirectoryScanTotals::fresh(1),
        }),
    ));
    batch
}

fn fold_with_systems(
    batch: PlatformEventBatch,
    systems: impl IntoIterator<Item = IndependentDomainSystem>,
) -> (SystemProjectionStore, BatchFoldOutput) {
    let mut store = SystemProjectionStore::default();
    let output = BatchFoldMachine::new(batch)
        .seed_inventory_failures(&mut store)
        .apply_domains_in_order(&mut store, systems)
        .advance_revisions(&mut store)
        .evaluate_alerts(&mut store)
        .apply_failure_feedback(&mut store)
        .finish();
    (store, output)
}

#[test]
fn independent_domain_systems_commute_under_the_production_registry() {
    let batch = independent_batch();
    let (forward, forward_output) = fold_with_systems(batch.clone(), IndependentDomainSystem::ALL);
    let (reverse, reverse_output) =
        fold_with_systems(batch, IndependentDomainSystem::ALL.into_iter().rev());

    assert_eq!(forward.services, reverse.services);
    assert_eq!(forward.services_source, reverse.services_source);
    assert_eq!(forward.directory_usage, reverse.directory_usage);
    assert_eq!(forward.services_revision, reverse.services_revision);
    assert_eq!(forward.refresh_count, reverse.refresh_count);
    assert_eq!(forward_output.changes, reverse_output.changes);
    assert_eq!(forward_output.activity, reverse_output.activity);
}

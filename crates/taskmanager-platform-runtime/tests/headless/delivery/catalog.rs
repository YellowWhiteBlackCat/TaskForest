use std::collections::BTreeMap;

use taskmanager_application::{
    CapabilityId, CapabilityStatus, ProviderId, RequestId, SidebandPolicy,
};

use super::*;
use crate::config::{CapabilityRoute, DeliveryClass, RuntimeDomain};
use crate::ecs::{CompletionOwner, CompletionRejection};

fn fixed_clock() -> u64 {
    10
}

fn request_id(value: u64) -> RequestId {
    RequestId::new(value).expect("fixture request id")
}

fn catalog() -> RuntimeCapabilityCatalog {
    RuntimeCapabilityCatalog::new(
        &[CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.catalog"),
            delivery: DeliveryClass::Observation,
            domain: RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        }],
        fixed_clock,
    )
}

#[test]
fn catalog_publishes_composition_status_before_first_request() {
    let route = CapabilityRoute {
        capability: CapabilityId::TELEMETRY_GPU_ENGINES,
        provider: ProviderId::borrowed("fixture.gpu-engines"),
        delivery: DeliveryClass::Observation,
        domain: RuntimeDomain::System,
        cadence_ms: None,
        sideband_policy: SidebandPolicy::Denied,
    };
    let initial_statuses = BTreeMap::from([(
        CapabilityId::TELEMETRY_GPU_ENGINES,
        CapabilityStatus::MissingDependency,
    )]);
    let catalog = RuntimeCapabilityCatalog::new(&[route], fixed_clock);
    catalog.seed_initial_statuses(&initial_statuses);

    assert_eq!(
        catalog
            .snapshot()
            .get(&CapabilityId::TELEMETRY_GPU_ENGINES)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::MissingDependency),
    );
}

#[test]
fn stale_completion_cannot_change_catalog_or_release_the_owner() {
    let catalog = catalog();
    let owner = request_id(1);
    let stale = request_id(2);
    let scheduler = catalog.ecs_scheduler_handle();
    assert!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .reserve_submission(&CapabilityId::TELEMETRY_CPU, owner, 0)
    );

    assert_eq!(
        catalog.record(
            &CapabilityId::TELEMETRY_CPU,
            CapabilityHealth::Available,
            10,
            stale,
        ),
        CompletionVerdict::Rejected(CompletionRejection::RequestMismatch)
    );
    assert_eq!(
        catalog
            .snapshot()
            .get(&CapabilityId::TELEMETRY_CPU)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::TemporarilyUnavailable),
        "a stale publication must not repaint capability health"
    );
    assert!(
        !scheduler
            .lock()
            .expect("scheduler lock")
            .reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(3), 11),
        "the stale request must not release the live owner"
    );
    assert_eq!(
        catalog.record(
            &CapabilityId::TELEMETRY_CPU,
            CapabilityHealth::Available,
            11,
            owner,
        ),
        CompletionVerdict::Rejected(CompletionRejection::InactiveOwner),
        "lifecycle retirement requires the publisher's terminal delivery claim"
    );
    assert!(
        catalog
            .claim_terminal_delivery(&CapabilityId::TELEMETRY_CPU, owner)
            .is_accepted()
    );
    assert_eq!(
        catalog.record(
            &CapabilityId::TELEMETRY_CPU,
            CapabilityHealth::Available,
            12,
            owner,
        ),
        CompletionVerdict::Accepted(CompletionOwner::Capability)
    );
}

#[test]
fn poisoned_catalog_lock_cannot_retain_completed_ecs_work() {
    let catalog = catalog();
    let owner = request_id(10);
    let scheduler = catalog.ecs_scheduler_handle();
    assert!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .reserve_submission(&CapabilityId::TELEMETRY_CPU, owner, 0)
    );

    let poison = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let _guard = catalog.descriptors.write().expect("descriptor lock");
                panic!("poison catalog fixture");
            })
            .join()
    });
    assert!(poison.is_err(), "fixture must poison the catalog lock");

    assert!(
        catalog
            .claim_terminal_delivery(&CapabilityId::TELEMETRY_CPU, owner)
            .is_accepted()
    );
    assert_eq!(
        catalog.record(
            &CapabilityId::TELEMETRY_CPU,
            CapabilityHealth::Available,
            10,
            owner,
        ),
        CompletionVerdict::Accepted(CompletionOwner::Capability)
    );
    assert!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(11), 11),
        "catalog presentation failure must not retain the completed owner"
    );
}

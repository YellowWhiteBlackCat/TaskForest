use std::collections::BTreeMap;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus, RequestId, SidebandPolicy};

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

/// The catalog answers for the whole product-expected surface: a registered
/// route keeps its provider attribution, every other expected identity keeps
/// its typed absence, and unknown identities are never fabricated.
#[test]
fn unregistered_product_capabilities_keep_typed_absence_descriptors() {
    let catalog = catalog();
    let snapshot = catalog.snapshot();

    let registered = snapshot
        .get(&CapabilityId::TELEMETRY_CPU)
        .expect("the registered route must stay in the catalog");
    assert_eq!(registered.status, CapabilityStatus::TemporarilyUnavailable);
    assert_eq!(
        registered.providers,
        [ProviderId::borrowed("fixture.catalog")]
    );
    assert!(!registered.is_typed_absence());

    for expected in CapabilityId::EXPECTED_SURFACE {
        let descriptor = snapshot
            .get(&expected)
            .unwrap_or_else(|| panic!("missing product capability {expected}"));
        if expected == CapabilityId::TELEMETRY_CPU {
            continue;
        }
        assert!(
            descriptor.is_typed_absence(),
            "{expected} has no registered route and must stay a typed absence"
        );
        assert_eq!(descriptor.status, CapabilityStatus::Unsupported);
        assert!(descriptor.providers.is_empty(), "{expected}");
        assert_eq!(descriptor.observed_at_ms, 0, "{expected}");
        assert!(descriptor.last_success_at_ms.is_none(), "{expected}");
    }

    let linux_only = snapshot
        .get(&CapabilityId::TELEMETRY_PRESSURE)
        .expect("a Linux-only family must still be addressable everywhere");
    assert!(linux_only.is_typed_absence());

    assert!(
        snapshot
            .get(&CapabilityId::owned("vendor.not-a-product-capability"))
            .is_none(),
        "identities outside the product surface are not fabricated"
    );
}

/// A platform registration replaces the product-surface absence seed for its
/// own capability only; the remaining expected identities stay typed-absent.
#[test]
fn platform_registration_replaces_only_its_own_typed_absence() {
    let route = CapabilityRoute {
        capability: CapabilityId::TELEMETRY_PRESSURE,
        provider: ProviderId::borrowed("fixture.pressure"),
        delivery: DeliveryClass::Observation,
        domain: RuntimeDomain::System,
        cadence_ms: None,
        sideband_policy: SidebandPolicy::Denied,
    };
    let catalog = RuntimeCapabilityCatalog::new(&[route], fixed_clock);
    let snapshot = catalog.snapshot();

    let registered = snapshot
        .get(&CapabilityId::TELEMETRY_PRESSURE)
        .expect("the registered Linux-only capability must be present");
    assert_eq!(
        registered.providers,
        [ProviderId::borrowed("fixture.pressure")]
    );
    assert!(!registered.is_typed_absence());
    assert!(
        snapshot
            .get(&CapabilityId::PROCESS_LIST)
            .is_some_and(|descriptor| descriptor.is_typed_absence()),
        "an unrelated expected capability keeps its typed absence"
    );
}

/// Unknown identities are rejected as typed unknowns: no panic, no fabricated
/// descriptor, and no scheduler effect.
#[test]
fn unknown_capability_ids_never_panic_or_fabricate_state() {
    let catalog = catalog();
    let unknown = CapabilityId::owned("vendor.not-a-product-capability");
    assert!(!unknown.is_expected());
    assert!(catalog.snapshot().get(&unknown).is_none());

    assert_eq!(
        CapabilityScheduler::request_recovery(
            &catalog,
            &unknown,
            CapabilityRecoveryTrigger::ExplicitRetry,
        ),
        CapabilityRecoveryOutcome::UnknownCapability,
    );
    CapabilityScheduler::set_cadence_ms(&catalog, &unknown, Some(1_000));
    assert!(
        !CapabilityScheduler::poll_due(&catalog, 0).contains(&unknown),
        "an unknown identity cannot become scheduled work"
    );
    assert!(catalog.snapshot().get(&unknown).is_none());
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
fn capability_status_keeps_escalation_and_permission_states_distinct() {
    for (failure, expected) in [
        (
            ProviderFailure::PermissionDenied,
            CapabilityStatus::PermissionRequired,
        ),
        (
            ProviderFailure::RequiresEscalation,
            CapabilityStatus::RequiresEscalation,
        ),
    ] {
        let catalog = catalog();
        let owner = request_id(1);
        let scheduler = catalog.ecs_scheduler_handle();
        assert!(
            scheduler
                .lock()
                .expect("scheduler lock")
                .reserve_submission(&CapabilityId::TELEMETRY_CPU, owner, 0)
        );
        assert!(
            catalog
                .claim_terminal_delivery(&CapabilityId::TELEMETRY_CPU, owner)
                .is_accepted()
        );
        assert_eq!(
            catalog.record(
                &CapabilityId::TELEMETRY_CPU,
                CapabilityHealth::Unavailable(failure),
                10,
                owner,
            ),
            CompletionVerdict::Accepted(CompletionOwner::Capability),
        );
        assert_eq!(
            catalog
                .snapshot()
                .get(&CapabilityId::TELEMETRY_CPU)
                .map(|descriptor| descriptor.status),
            Some(expected),
            "{failure:?} must not be folded onto the other permission state",
        );
    }
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

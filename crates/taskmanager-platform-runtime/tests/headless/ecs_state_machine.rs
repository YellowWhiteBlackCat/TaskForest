use std::collections::BTreeMap;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{
    CapabilityId, RequestId, RequestScope, RequestTracking, SidebandPolicy,
};

use crate::config::{
    CapabilityRoute, DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY, DeliveryClass, RuntimeDomain,
};
use crate::health::CapabilityHealth;

use super::{
    CompletionOwner, CompletionVerdict, DEFAULT_IN_FLIGHT_LEASE_MS, EcsAdmissionError,
    RuntimeEcsScheduler, StalledSubject,
};

#[derive(Clone)]
struct ModelJob {
    scope: RequestScope,
    deadline_ms: u64,
    stalled: bool,
}

struct FixedSeed(u64);

impl FixedSeed {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
}

fn request_id(value: u64) -> RequestId {
    RequestId::new(value).expect("state-machine IDs are nonzero")
}

fn scope(value: u64) -> RequestScope {
    RequestScope::try_owned(format!("target:{value}")).expect("bounded model scope")
}

fn scheduler() -> RuntimeEcsScheduler {
    // This model proves ownership bounds, not the scheduler's own
    // stall-abandonment authority: retain stalled owners forever here so the
    // two stay in lockstep (abandonment has its own behavior tests).
    RuntimeEcsScheduler::from_runtime_routes_with_budgets(
        &[CapabilityRoute {
            capability: CapabilityId::DIRECTORY_USAGE,
            provider: ProviderId::borrowed("fixture.state-machine"),
            delivery: DeliveryClass::Observation,
            domain: RuntimeDomain::Storage,
            cadence_ms: None,
            sideband_policy: SidebandPolicy::Idempotent,
        }],
        0,
        crate::config::RuntimeBudgets {
            max_stalled_lifetime_ms: u64::MAX,
            ..crate::config::RuntimeBudgets::DEFAULT
        },
    )
}

#[test]
fn fixed_seed_lifecycle_model_preserves_ownership_bounds_and_monotonic_leases() {
    let capability = CapabilityId::DIRECTORY_USAGE;
    let mut scheduler = scheduler();
    let mut model = BTreeMap::<u64, ModelJob>::new();
    let mut now_ms = 0_u64;

    for index in 0..DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY {
        let value = index as u64 + 1;
        assert_eq!(
            scheduler.admit_submission_with_tracking(
                &capability,
                request_id(value),
                now_ms,
                RequestTracking::Target(scope(value)),
            ),
            Ok(())
        );
        model.insert(
            value,
            ModelJob {
                scope: scope(value),
                deadline_ms: DEFAULT_IN_FLIGHT_LEASE_MS,
                stalled: false,
            },
        );
    }
    assert_eq!(
        scheduler.admit_submission_with_tracking(
            &capability,
            request_id(10_000),
            now_ms,
            RequestTracking::Target(scope(10_000)),
        ),
        Err(EcsAdmissionError::TargetCapacity)
    );

    for value in 1..=16 {
        assert!(scheduler.cancel_submission(&capability, request_id(value), now_ms));
        model.remove(&value);
    }

    let mut random = FixedSeed(0x5eed_cafe_f00d_beef);
    for _step in 0..4_096 {
        match random.next() % 5 {
            0 => {
                let value = random.next() % 160 + 1;
                let scope_value = random.next() % 96 + 1;
                let request = request_id(value);
                let target = scope(scope_value);
                let expected = if model.contains_key(&value) {
                    Err(EcsAdmissionError::DuplicateRequest)
                } else if model.values().any(|job| job.scope == target) {
                    Err(EcsAdmissionError::TargetInFlight)
                } else if model.len() == DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY {
                    Err(EcsAdmissionError::TargetCapacity)
                } else {
                    Ok(())
                };
                let actual = scheduler.admit_submission_with_tracking(
                    &capability,
                    request,
                    now_ms,
                    RequestTracking::Target(target.clone()),
                );
                assert_eq!(actual, expected);
                if actual.is_ok() {
                    model.insert(
                        value,
                        ModelJob {
                            scope: target,
                            deadline_ms: now_ms.saturating_add(DEFAULT_IN_FLIGHT_LEASE_MS),
                            stalled: false,
                        },
                    );
                }
            }
            1 => {
                let value = random.next() % 160 + 1;
                let expected_owner = model.contains_key(&value);
                let verdict = scheduler.record_health(
                    &capability,
                    request_id(value),
                    CapabilityHealth::Available,
                    now_ms,
                );
                match (expected_owner, verdict) {
                    (true, CompletionVerdict::Accepted(CompletionOwner::Target)) => {
                        model.remove(&value);
                    }
                    (false, CompletionVerdict::Rejected(_)) => {}
                    (_, unexpected) => {
                        panic!("completion changed the wrong owner: {unexpected:?}")
                    }
                }
            }
            2 => {
                let value = random.next() % 160 + 1;
                assert_eq!(
                    scheduler.cancel_submission(&capability, request_id(value), now_ms),
                    model.remove(&value).is_some(),
                    "rollback must retire only its exact RequestId"
                );
            }
            3 => {
                now_ms = now_ms.saturating_add(random.next() % 5_000);
                let expected_stalls = model
                    .iter_mut()
                    .filter_map(|(value, job)| {
                        (!job.stalled && job.deadline_ms <= now_ms).then(|| {
                            job.stalled = true;
                            request_id(*value)
                        })
                    })
                    .collect::<Vec<_>>();
                let actual_stalls = scheduler
                    .tick_plan(now_ms)
                    .stalled
                    .into_iter()
                    .filter_map(|subject| match subject {
                        StalledSubject::Target { request_id, .. } => Some(request_id),
                        StalledSubject::Capability { .. } => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual_stalls, expected_stalls);
            }
            _ => {
                let value = random.next() % 160 + 1;
                let renewed = scheduler.renew_target_lease(&capability, request_id(value), now_ms);
                assert_eq!(renewed.is_ok(), model.contains_key(&value));
                if let Some(job) = model.get_mut(&value) {
                    job.deadline_ms = now_ms.saturating_add(DEFAULT_IN_FLIGHT_LEASE_MS);
                    job.stalled = false;
                }
            }
        }

        let snapshot = scheduler.scheduling_snapshot();
        assert_eq!(scheduler.target_job_count(), model.len());
        assert_eq!(snapshot.active_target_jobs, model.len() as u64);
        assert!(model.len() <= DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY);
        assert!(snapshot.target_high_water <= DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY as u64);
        assert!(
            snapshot.recent_stalls.len()
                <= taskmanager_platform_contract::MAX_RECENT_SCHEDULING_STALLS
        );
    }
}

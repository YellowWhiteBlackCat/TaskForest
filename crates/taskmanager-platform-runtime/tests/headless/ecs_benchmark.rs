//! Small, dependency-free ECS workload fixture.
//!
//! The sample is intentionally a trend input rather than a promotion gate:
//! it measures the same typed plan/submission/completion loop without a native
//! provider or an operating-system dependency. Cross-platform claims still
//! require running the fixture on each target build.

use std::time::Instant;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{CapabilityId, RequestId, SidebandPolicy};

use crate::config::{CapabilityRoute, DeliveryClass};

use crate::health::CapabilityHealth;

use super::RuntimeEcsScheduler;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BenchmarkSample {
    pub(super) iterations: u32,
    pub(super) planned_items: u64,
    pub(super) accepted_submissions: u64,
    pub(super) accepted_health: u64,
    pub(super) elapsed_ns: u128,
}

pub(super) fn run_fixture(iterations: u32) -> BenchmarkSample {
    let provider = ProviderId::borrowed("fixture.ecs.benchmark");
    let routes = [
        CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: provider.clone(),
            delivery: DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
        CapabilityRoute {
            capability: CapabilityId::PROCESS_CONTROL,
            provider,
            delivery: DeliveryClass::Control,
            domain: crate::config::RuntimeDomain::Process,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
    ];
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes, 0);
    let started = Instant::now();
    let mut planned_items: u64 = 0;
    let mut accepted_submissions: u64 = 0;
    let mut accepted_health: u64 = 0;
    for iteration in 0..iterations {
        let now_ms = u64::from(iteration).saturating_mul(1_000);
        let plan = scheduler.tick_plan(now_ms);
        planned_items = planned_items.saturating_add(plan.items.len() as u64);
        for (offset, item) in plan.items.into_iter().enumerate() {
            let request_id = RequestId::new(
                u64::from(iteration)
                    .saturating_mul(4)
                    .saturating_add(offset as u64 + 1),
            )
            .expect("benchmark request id is non-zero");
            accepted_submissions +=
                u64::from(scheduler.reserve_submission(&item.capability, request_id, now_ms));
            accepted_health += u64::from(
                scheduler
                    .record_health(
                        &item.capability,
                        request_id,
                        CapabilityHealth::Available,
                        now_ms,
                    )
                    .is_accepted(),
            );
        }
    }
    BenchmarkSample {
        iterations,
        planned_items,
        accepted_submissions,
        accepted_health,
        elapsed_ns: started.elapsed().as_nanos(),
    }
}
